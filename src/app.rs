//! Application state and main layout: the tree on the left, tabs on top,
//! the status bar at the bottom and the open page in the middle.

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    time::Instant,
};

use eframe::egui::{self, RichText};
use uuid::Uuid;

use dbpadu_core::{
    model::{Appearance, Connection, Data},
    secrets::Secrets,
    sql,
    store::Store,
};
use dbpadu_engine::{
    Engine, Event, locate,
    protocol::{
        self, Connected, Hello, QueryResult, Request, Schemas, Table, TableColumn, TableColumns,
        Tables, Target, TxAnswer, TxState,
    },
};

use crate::{
    theme::Theme,
    ui::{self, editor::ConnectionEditor},
};

/// Whether the query engine is there to do work.
pub enum EngineState {
    Starting,
    Ready(Hello),
    /// Not running, with the reason.
    Failed(String),
}

/// Something that is fetched from the database when it is first looked at.
pub enum Load<T> {
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}

pub struct TableNode {
    pub info: Table,
    pub columns: Load<Vec<TableColumn>>,
}

pub struct SchemaNode {
    pub name: String,
    pub tables: Load<Vec<TableNode>>,
}

pub enum LinkState {
    Connecting,
    Connected { product: String, version: String },
    Failed(String),
}

/// An open (or opening) connection to a database.
pub struct Link {
    pub state: LinkState,
    pub tx: TxState,
    /// How the database groups its tables: `schema`, `catalog` or `none`.
    pub mode: String,
    pub schemas: Load<Vec<SchemaNode>>,
}

impl Link {
    pub fn connected(&self) -> bool {
        matches!(self.state, LinkState::Connected { .. })
    }
}

/// What the last run of a query tab gave.
pub enum Outcome {
    Result(QueryResult),
    Error(String),
}

pub struct Running {
    pub since: Instant,
}

/// One query tab: an SQL editor and the result under it.
pub struct QueryTab {
    pub id: u64,
    pub connection: Uuid,
    pub title: String,
    pub sql: String,
    /// Where the text cursor is, in characters.
    pub cursor: usize,
    /// The selected text, in characters, if any.
    pub selection: Option<Range<usize>>,
    /// The most rows a run of this tab may return.
    pub limit: u32,
    pub running: Option<Running>,
    pub outcome: Option<Outcome>,
    /// Counts the runs, so each result gets fresh column widths.
    pub runs: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// The connection cards.
    Home,
    Tab(u64),
}

/// What an answer from the engine is for.
enum Pending {
    Hello,
    Test,
    Connect(Uuid),
    Execute(u64),
    Tx(Uuid),
    Schemas(Uuid),
    Tables(Uuid, String),
    Columns(Uuid, String, String),
    /// Nothing to do with the answer.
    Ignore,
}

/// A message shown in a bar at the top of the window until dismissed.
#[derive(Clone)]
pub struct Notice {
    pub text: String,
    pub error: bool,
}

/// Asks for the password of a connection before connecting.
pub struct PasswordPrompt {
    pub connection: Uuid,
    pub password: String,
    pub remember: bool,
}

pub struct App {
    store: Store,
    pub data: Data,
    pub theme: Theme,
    pub secrets: Secrets,
    ctx: egui::Context,
    engine: Option<Engine>,
    pub engine_state: EngineState,
    pending: HashMap<u64, Pending>,
    /// Open connections, by connection id.
    pub links: HashMap<Uuid, Link>,
    pub tabs: Vec<QueryTab>,
    pub view: View,
    next_tab: u64,
    /// The connection form, while it is open.
    pub editor: Option<ConnectionEditor>,
    pub prompt: Option<PasswordPrompt>,
    pub notice: Option<Notice>,
    /// Opened nodes of the tree.
    pub expanded: HashSet<String>,
    /// Text typed in the tree's filter box.
    pub tree_filter: String,
    /// Text typed in the search box above the cards.
    pub search: String,
    /// Passwords typed in this run of the app, so a reconnect does not ask
    /// again. Never written anywhere.
    passwords: HashMap<Uuid, String>,
    /// The dbpadu logo, drawn at the top of the tree.
    pub logo: egui::TextureHandle,
}

/// The app's look for the `appearance` setting.
fn ui_theme(appearance: Appearance, ctx: &egui::Context) -> Theme {
    let dark = match appearance {
        Appearance::Dark => true,
        Appearance::Light => false,
        // Dark when the system does not say.
        Appearance::System => ctx.system_theme() != Some(egui::Theme::Light),
    };
    Theme::ui(dark)
}

pub fn connection_key(connection: Uuid) -> String {
    format!("c:{connection}")
}

pub fn schema_key(connection: Uuid, schema: &str) -> String {
    format!("s:{connection}:{schema}")
}

pub fn table_key(connection: Uuid, schema: &str, table: &str) -> String {
    format!("t:{connection}:{schema}:{table}")
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let store = Store::open_default();
        let (data, notice) = match store.load() {
            Ok(data) => (data, None),
            Err(err) => {
                // Keep a copy of the unreadable file before it gets overwritten.
                let backup = store.path().map(|path| {
                    let backup = path.with_extension("corrupt.json");
                    let _ = std::fs::copy(path, &backup);
                    backup
                });
                let text = match backup {
                    Some(backup) => format!(
                        "Saved connections could not be read ({err}). A copy was kept at {}.",
                        backup.display()
                    ),
                    None => format!("Saved connections could not be read ({err})."),
                };
                (Data::default(), Some(Notice { text, error: true }))
            }
        };
        let theme = ui_theme(data.settings.appearance, &cc.egui_ctx);
        theme.apply(&cc.egui_ctx);
        let mut app = Self {
            store,
            data,
            theme,
            secrets: Secrets::os(),
            ctx: cc.egui_ctx.clone(),
            engine: None,
            engine_state: EngineState::Starting,
            pending: HashMap::new(),
            links: HashMap::new(),
            tabs: Vec::new(),
            view: View::Home,
            next_tab: 1,
            editor: None,
            prompt: None,
            notice,
            expanded: HashSet::new(),
            tree_filter: String::new(),
            search: String::new(),
            passwords: HashMap::new(),
            logo: crate::icon::logo_texture(&cc.egui_ctx),
        };
        app.start_engine();
        app
    }

    pub fn save(&mut self) {
        if let Err(err) = self.store.save(&self.data) {
            self.notify(format!("Could not save changes: {err}"));
        }
    }

    /// Show an error in the bar at the top of the window.
    pub fn notify(&mut self, text: String) {
        self.notice = Some(Notice { text, error: true });
    }

    pub fn set_appearance(&mut self, appearance: Appearance) {
        self.data.settings.appearance = appearance;
        self.theme = ui_theme(appearance, &self.ctx);
        self.theme.apply(&self.ctx);
        self.save();
    }

    /// With the "System" setting, switch when the operating system does.
    fn follow_system_appearance(&mut self) {
        if self.data.settings.appearance != Appearance::System {
            return;
        }
        let theme = ui_theme(Appearance::System, &self.ctx);
        if theme != self.theme {
            self.theme = theme;
            theme.apply(&self.ctx);
        }
    }

    // ---- Engine -----------------------------------------------------

    /// Start the engine, or start it again after it stopped. Connections
    /// that were open are gone with the old engine.
    pub fn start_engine(&mut self) {
        self.engine = None;
        self.pending.clear();
        self.links.clear();
        for tab in &mut self.tabs {
            if tab.running.take().is_some() {
                tab.outcome = Some(Outcome::Error("The engine was restarted.".to_string()));
            }
        }
        let Some(jar) = locate::jar() else {
            self.engine_state = EngineState::Failed(format!(
                "{} was not found next to the program. Build it with build.sh or build.ps1.",
                locate::JAR_NAME
            ));
            return;
        };
        let java = locate::java(&self.data.settings.java_path);
        let ctx = self.ctx.clone();
        match Engine::start(&java, &jar, move || ctx.request_repaint()) {
            Ok(engine) => {
                self.engine = Some(engine);
                self.engine_state = EngineState::Starting;
                self.request(protocol::hello(), Pending::Hello);
            }
            Err(err) => {
                self.engine_state = EngineState::Failed(format!(
                    "Java could not be started ({err}). Install Java 17 or newer, or set JAVA_HOME."
                ));
            }
        }
    }

    fn request(&mut self, (op, params): Request, pending: Pending) -> bool {
        let Some(engine) = self.engine.as_mut() else {
            return false;
        };
        match engine.send(op, params) {
            Ok(id) => {
                self.pending.insert(id, pending);
                true
            }
            Err(err) => {
                self.notify(format!("The engine could not be reached: {err}"));
                false
            }
        }
    }

    fn poll_engine(&mut self) {
        let Some(engine) = self.engine.as_mut() else {
            return;
        };
        for event in engine.poll() {
            match event {
                Event::Answer { id, result } => {
                    if let Some(pending) = self.pending.remove(&id) {
                        self.answer(pending, result);
                    }
                }
                Event::Stopped(text) => self.engine_stopped(text),
            }
        }
    }

    fn engine_stopped(&mut self, text: String) {
        self.engine = None;
        self.pending.clear();
        self.links.clear();
        for tab in &mut self.tabs {
            if tab.running.take().is_some() {
                tab.outcome = Some(Outcome::Error(
                    "The engine stopped while this was running.".to_string(),
                ));
            }
        }
        let reason = match text.trim() {
            "" => "The engine stopped.".to_string(),
            text => format!("The engine stopped: {text}"),
        };
        self.engine_state = EngineState::Failed(reason);
    }

    fn answer(&mut self, pending: Pending, result: Result<dbpadu_engine::Value, String>) {
        match pending {
            Pending::Ignore => {}
            Pending::Hello => {
                self.engine_state = match result.and_then(protocol::read::<Hello>) {
                    Ok(hello) => EngineState::Ready(hello),
                    Err(err) => EngineState::Failed(err),
                };
            }
            Pending::Test => {
                if let Some(editor) = &mut self.editor {
                    editor.testing = false;
                    editor.test = Some(result.and_then(protocol::read::<Connected>).map(
                        |connected| {
                            format!("It works: {} {}", connected.product, connected.version)
                        },
                    ));
                }
            }
            Pending::Connect(id) => match result.and_then(protocol::read::<Connected>) {
                Ok(connected) => {
                    if let Some(link) = self.links.get_mut(&id) {
                        link.state = LinkState::Connected {
                            product: connected.product,
                            version: connected.version,
                        };
                        link.tx = connected.tx;
                        link.schemas = Load::Loading;
                    }
                    self.expanded.insert(connection_key(id));
                    self.request(protocol::schemas(&id.to_string()), Pending::Schemas(id));
                    // Straight to work: a query tab, unless one is waiting.
                    match self.tabs.iter().find(|tab| tab.connection == id) {
                        Some(tab) => self.view = View::Tab(tab.id),
                        None => {
                            self.open_tab(id, String::new(), None);
                        }
                    }
                }
                Err(err) => {
                    // A wrong password is asked for again next time.
                    self.passwords.remove(&id);
                    if let Some(link) = self.links.get_mut(&id) {
                        link.state = LinkState::Failed(err);
                    }
                }
            },
            Pending::Execute(tab_id) => {
                let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
                    return;
                };
                tab.running = None;
                tab.runs += 1;
                match result.and_then(protocol::read::<QueryResult>) {
                    Ok(result) => {
                        if let Some(link) = self.links.get_mut(&tab.connection) {
                            link.tx = result.tx;
                        }
                        tab.outcome = Some(Outcome::Result(result));
                    }
                    Err(err) => tab.outcome = Some(Outcome::Error(err)),
                }
            }
            Pending::Tx(id) => match result.and_then(protocol::read::<TxAnswer>) {
                Ok(answer) => {
                    if let Some(link) = self.links.get_mut(&id) {
                        link.tx = answer.tx;
                    }
                }
                Err(err) => self.notify(err),
            },
            Pending::Schemas(id) => {
                let Some(link) = self.links.get_mut(&id) else {
                    return;
                };
                match result.and_then(protocol::read::<Schemas>) {
                    Ok(schemas) => {
                        link.mode = schemas.mode;
                        // Tables are fetched now for a database without
                        // schemas, and for the schema the connection is in.
                        let open = if link.mode == "none" {
                            Some(String::new())
                        } else {
                            schemas
                                .current
                                .filter(|current| schemas.items.contains(current))
                        };
                        let names = if link.mode == "none" {
                            vec![String::new()]
                        } else {
                            schemas.items
                        };
                        link.schemas = Load::Ready(
                            names
                                .into_iter()
                                .map(|name| SchemaNode {
                                    name,
                                    tables: Load::Idle,
                                })
                                .collect(),
                        );
                        if let Some(schema) = open {
                            self.expanded.insert(schema_key(id, &schema));
                            self.load_tables(id, &schema);
                        }
                    }
                    Err(err) => link.schemas = Load::Failed(err),
                }
            }
            Pending::Tables(id, schema) => {
                let Some(node) = self.schema_mut(id, &schema) else {
                    return;
                };
                node.tables = match result.and_then(protocol::read::<Tables>) {
                    Ok(tables) => Load::Ready(
                        tables
                            .items
                            .into_iter()
                            .map(|info| TableNode {
                                info,
                                columns: Load::Idle,
                            })
                            .collect(),
                    ),
                    Err(err) => Load::Failed(err),
                };
            }
            Pending::Columns(id, schema, table) => {
                let Some(node) = self.table_mut(id, &schema, &table) else {
                    return;
                };
                node.columns = match result.and_then(protocol::read::<TableColumns>) {
                    Ok(columns) => Load::Ready(columns.items),
                    Err(err) => Load::Failed(err),
                };
            }
        }
    }

    fn schema_mut(&mut self, id: Uuid, schema: &str) -> Option<&mut SchemaNode> {
        match &mut self.links.get_mut(&id)?.schemas {
            Load::Ready(schemas) => schemas.iter_mut().find(|node| node.name == schema),
            _ => None,
        }
    }

    fn table_mut(&mut self, id: Uuid, schema: &str, table: &str) -> Option<&mut TableNode> {
        match &mut self.schema_mut(id, schema)?.tables {
            Load::Ready(tables) => tables.iter_mut().find(|node| node.info.name == table),
            _ => None,
        }
    }

    // ---- Connections ------------------------------------------------

    /// Check the details in the connection form against the database.
    pub fn test_connection(&mut self, connection: &Connection, password: &str) {
        let url = connection.jdbc_url();
        let target = Target {
            kind: connection.kind.engine_name(),
            url: &url,
            user: connection.username.trim(),
            password,
        };
        let sent = self.request(protocol::test(&target), Pending::Test);
        if let Some(editor) = &mut self.editor {
            editor.testing = sent;
            editor.test = if sent {
                None
            } else {
                Some(Err("The engine is not running.".to_string()))
            };
        }
    }

    /// Remember a password for this run of the app.
    pub fn keep_password(&mut self, connection: Uuid, password: String) {
        self.passwords.insert(connection, password);
    }

    /// Connect to a saved connection, asking for the password if it is not
    /// known yet.
    pub fn connect(&mut self, id: Uuid) {
        let Some(connection) = self.data.connection(id) else {
            return;
        };
        if self
            .links
            .get(&id)
            .is_some_and(|link| !matches!(link.state, LinkState::Failed(_)))
        {
            return;
        }
        if !matches!(self.engine_state, EngineState::Ready(_)) {
            self.notify(
                "The engine is not running yet. See the status at the bottom right.".into(),
            );
            return;
        }
        let needs_password = !connection.kind.is_file() && !connection.username.trim().is_empty();
        let password = if !needs_password {
            Some(String::new())
        } else if let Some(password) = self.passwords.get(&id) {
            Some(password.clone())
        } else if connection.save_password {
            self.secrets.get(id)
        } else {
            None
        };
        match password {
            Some(password) => self.connect_with(id, password),
            None => {
                self.prompt = Some(PasswordPrompt {
                    connection: id,
                    password: String::new(),
                    remember: false,
                });
            }
        }
    }

    pub fn connect_with(&mut self, id: Uuid, password: String) {
        let Some(connection) = self.data.connection(id) else {
            return;
        };
        let url = connection.jdbc_url();
        let target = Target {
            kind: connection.kind.engine_name(),
            url: &url,
            user: connection.username.trim(),
            password: &password,
        };
        let request = protocol::connect(&id.to_string(), &target);
        if self.request(request, Pending::Connect(id)) {
            self.passwords.insert(id, password);
            self.links.insert(
                id,
                Link {
                    state: LinkState::Connecting,
                    tx: TxState::default(),
                    mode: String::new(),
                    schemas: Load::Idle,
                },
            );
        }
    }

    /// Close the connection. Changes that were not committed are rolled
    /// back by the engine.
    pub fn disconnect(&mut self, id: Uuid) {
        if self.links.remove(&id).is_some() {
            self.request(protocol::disconnect(&id.to_string()), Pending::Ignore);
        }
        for tab in &mut self.tabs {
            if tab.connection == id {
                tab.running = None;
            }
        }
    }

    pub fn delete_connection(&mut self, id: Uuid) {
        self.disconnect(id);
        self.tabs.retain(|tab| tab.connection != id);
        if let View::Tab(tab) = self.view
            && !self.tabs.iter().any(|open| open.id == tab)
        {
            self.view = View::Home;
        }
        self.passwords.remove(&id);
        self.secrets.delete(id);
        self.data.remove(id);
        self.save();
    }

    // ---- Tree -------------------------------------------------------

    pub fn load_tables(&mut self, id: Uuid, schema: &str) {
        let mode = match self.links.get(&id) {
            Some(link) => link.mode.clone(),
            None => return,
        };
        let Some(node) = self.schema_mut(id, schema) else {
            return;
        };
        if matches!(node.tables, Load::Loading | Load::Ready(_)) {
            return;
        }
        node.tables = Load::Loading;
        self.request(
            protocol::tables(&id.to_string(), &mode, schema),
            Pending::Tables(id, schema.to_string()),
        );
    }

    pub fn load_columns(&mut self, id: Uuid, schema: &str, table: &str) {
        let mode = match self.links.get(&id) {
            Some(link) => link.mode.clone(),
            None => return,
        };
        let Some(node) = self.table_mut(id, schema, table) else {
            return;
        };
        if matches!(node.columns, Load::Loading | Load::Ready(_)) {
            return;
        }
        node.columns = Load::Loading;
        self.request(
            protocol::columns(&id.to_string(), &mode, schema, table),
            Pending::Columns(id, schema.to_string(), table.to_string()),
        );
    }

    /// Fetch the tree of a connection again.
    pub fn refresh_tree(&mut self, id: Uuid) {
        if let Some(link) = self.links.get_mut(&id)
            && link.connected()
        {
            link.schemas = Load::Loading;
            self.request(protocol::schemas(&id.to_string()), Pending::Schemas(id));
        }
    }

    // ---- Tabs and queries -------------------------------------------

    /// Open a query tab on a connection and show it. With `title`, the tab
    /// is named after it (a table); otherwise after the connection.
    pub fn open_tab(&mut self, connection: Uuid, sql: String, title: Option<String>) -> u64 {
        let id = self.next_tab;
        self.next_tab += 1;
        let title = title.unwrap_or_else(|| {
            let name = self
                .data
                .connection(connection)
                .map(Connection::display_name)
                .unwrap_or_default();
            let others = self
                .tabs
                .iter()
                .filter(|tab| tab.connection == connection)
                .count();
            if others == 0 {
                name
            } else {
                format!("{name} ({})", others + 1)
            }
        });
        self.tabs.push(QueryTab {
            id,
            connection,
            title,
            cursor: sql.chars().count(),
            sql,
            selection: None,
            limit: self.data.settings.row_limit,
            running: None,
            outcome: None,
            runs: 0,
        });
        self.view = View::Tab(id);
        id
    }

    pub fn close_tab(&mut self, id: u64) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        self.tabs.remove(index);
        if self.view == View::Tab(id) {
            // Move to the neighbouring tab, or back to the cards.
            self.view = self
                .tabs
                .get(index.min(self.tabs.len().saturating_sub(1)))
                .map_or(View::Home, |tab| View::Tab(tab.id));
        }
    }

    /// Run the selected text of a tab, or the statement the cursor is in.
    pub fn run(&mut self, tab_id: u64) {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == tab_id) else {
            return;
        };
        if tab.running.is_some() {
            return;
        }
        let connection = tab.connection;
        if !self.links.get(&connection).is_some_and(Link::connected) {
            self.connect(connection);
            return;
        }
        let selected = tab.selection.clone().map(|range| {
            tab.sql
                .chars()
                .skip(range.start)
                .take(range.end - range.start)
                .collect::<String>()
        });
        let statement = match &selected {
            Some(selected) if !selected.trim().is_empty() => Some(selected.trim().to_string()),
            _ => sql::statement_at(&tab.sql, tab.cursor).map(str::to_string),
        };
        let limit = tab.limit;
        let Some(statement) = statement else {
            self.tab_mut(tab_id).outcome = Some(Outcome::Error(
                "There is nothing to run. Type an SQL statement first.".to_string(),
            ));
            return;
        };
        let request = protocol::execute(&connection.to_string(), &statement, limit);
        if self.request(request, Pending::Execute(tab_id)) {
            self.tab_mut(tab_id).running = Some(Running {
                since: Instant::now(),
            });
        }
    }

    fn tab_mut(&mut self, id: u64) -> &mut QueryTab {
        self.tabs
            .iter_mut()
            .find(|tab| tab.id == id)
            .expect("the tab is open")
    }

    /// Ask the database to stop what a connection is running.
    pub fn cancel(&mut self, connection: Uuid) {
        self.request(protocol::cancel(&connection.to_string()), Pending::Ignore);
    }

    pub fn set_auto_commit(&mut self, connection: Uuid, value: bool) {
        self.request(
            protocol::auto_commit(&connection.to_string(), value),
            Pending::Tx(connection),
        );
    }

    pub fn commit(&mut self, connection: Uuid) {
        self.request(
            protocol::commit(&connection.to_string()),
            Pending::Tx(connection),
        );
    }

    pub fn rollback(&mut self, connection: Uuid) {
        self.request(
            protocol::rollback(&connection.to_string()),
            Pending::Tx(connection),
        );
    }

    /// The tab that is shown, if the page is a tab.
    pub fn current_tab(&self) -> Option<&QueryTab> {
        match self.view {
            View::Tab(id) => self.tabs.iter().find(|tab| tab.id == id),
            View::Home => None,
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.follow_system_appearance();
        self.poll_engine();
        let theme = self.theme;

        if self.tabs.iter().any(|tab| tab.running.is_some()) {
            // Keeps the running time on screen moving.
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }

        egui::Panel::bottom("status")
            .exact_size(30.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(theme.panel)
                    .inner_margin(egui::Margin::symmetric(12, 0)),
            )
            .show(ui, |ui| ui::statusbar::show(self, ui));

        egui::Panel::left("tree")
            .default_size(250.0)
            .size_range(180.0..=480.0)
            .resizable(true)
            .frame(
                egui::Frame::new()
                    .fill(theme.panel)
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(ui, |ui| ui::tree::show(self, ui));

        egui::Panel::top("tabs")
            .exact_size(38.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(theme.panel)
                    .inner_margin(egui::Margin::symmetric(8, 0)),
            )
            .show(ui, |ui| ui::tabs::show(self, ui));

        if let Some(notice) = self.notice.clone() {
            egui::Panel::top("notice")
                .frame(
                    egui::Frame::new()
                        .fill(theme.surface)
                        .inner_margin(egui::Margin::symmetric(16, 8)),
                )
                .show(ui, |ui| {
                    // Button first, from the right, so a long message wraps
                    // instead of pushing it off the window.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Dismiss").clicked() {
                            self.notice = None;
                        }
                        let color = if notice.error {
                            theme.danger
                        } else {
                            theme.text
                        };
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(notice.text).color(color)).wrap(),
                            );
                        });
                    });
                });
        }

        match self.view {
            View::Tab(id) if self.tabs.iter().any(|tab| tab.id == id) => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(theme.bg))
                    .show(ui, |ui| ui::query::show(self, ui, id));
            }
            _ => {
                self.view = View::Home;
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::new()
                            .fill(theme.bg)
                            .inner_margin(egui::Margin::same(24)),
                    )
                    .show(ui, |ui| ui::home::show(self, ui));
            }
        }

        ui::editor::show(self, ui.ctx());
        ui::editor::password_prompt(self, ui.ctx());
    }
}
