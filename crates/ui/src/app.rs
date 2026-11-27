//! Application state and main layout: the tree on the left, tabs on top,
//! the log and the status bar at the bottom and the open page in the middle.

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    time::Instant,
};

use eframe::egui::{self, RichText};
use uuid::Uuid;

use dbpadu_core::{
    model::{self, Appearance, Connection, Data, DbKind, SshAuth},
    secrets::{Secret, Secrets},
    sql,
    store::Store,
};
use dbpadu_engine::{
    Engine, Event, locate,
    protocol::{
        self, Connected, Driver, Exported, Hello, JumpTarget, OwnDriverInfo, QueryResult, Request,
        Routine, Routines, Schemas, Sequences, SshTarget, Table, TableColumn, TableColumns, Tables,
        Target, TxAnswer, TxState,
    },
};

use crate::{
    theme::Theme,
    ui::{
        self, drivers::DriverEditor, editor::ConnectionEditor, export::ExportDialog,
        grid::GridState, home::GroupDialog,
    },
};

/// Lines kept in each log; the oldest go first.
const LOG_LINES: usize = 1_000;

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
    /// Tables and views together, as the database lists them.
    pub tables: Load<Vec<TableNode>>,
    /// Functions and procedures.
    pub routines: Load<Vec<Routine>>,
    pub sequences: Load<Vec<String>>,
}

/// The folders under a schema in the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Tables,
    Views,
    Routines,
    Sequences,
}

impl Category {
    pub const ALL: [Category; 4] = [
        Category::Tables,
        Category::Views,
        Category::Routines,
        Category::Sequences,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Tables => "Tables",
            Category::Views => "Views",
            Category::Routines => "Routines",
            Category::Sequences => "Sequences",
        }
    }
}

/// One line of the SQL log: a statement that was sent to a database.
pub struct SqlLog {
    pub time: String,
    /// Name of the connection it ran on.
    pub connection: String,
    /// The SQL as it was sent, with anything dbpadu added.
    pub sql: String,
    /// What it gave, or why it failed.
    pub outcome: Result<String, String>,
}

/// One line of the engine log.
pub struct EngineLog {
    pub time: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogTab {
    Sql,
    Engine,
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
    /// Every row is being written to a file; the result on screen stays.
    pub exporting: bool,
}

/// An export of every row, from the request to the file.
struct ExportJob {
    tab: u64,
    /// The connection's name and the SQL, for the log.
    name: String,
    statement: String,
    /// Where the engine writes the rows.
    rows: PathBuf,
    /// The file the user asked for.
    target: PathBuf,
    format: dbpadu_export::Format,
    options: dbpadu_export::Options,
}

/// How an export of every row ended: the rows written, and the file.
type ExportDone = (Result<u64, String>, PathBuf);

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
    /// The SQL the outcome came from, as the user wrote it.
    pub statement: String,
    /// Counts the runs, so each result gets fresh column widths.
    pub runs: u32,
    /// What is selected in the result grid.
    pub grid: GridState,
    /// The table and key columns the SQL formats write, for this result.
    pub extract: dbpadu_export::Options,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// The connection cards.
    Home,
    /// The JDBC drivers.
    Drivers,
    Settings,
    About,
    Tab(u64),
}

/// What an answer from the engine is for.
enum Pending {
    Hello,
    Test,
    Connect(Uuid),
    /// A run of a tab: the connection's name and the SQL, for the log.
    Execute(u64, String, String),
    /// Every row of a result, on its way to a file.
    Export(Box<ExportJob>),
    /// A change of the transaction, with what to call it in the log.
    Tx(Uuid, &'static str),
    Schemas(Uuid),
    Tables(Uuid, String),
    Columns(Uuid, String, String),
    Routines(Uuid, String),
    Sequences(Uuid, String),
    /// A look into the files of one of the user's own drivers.
    DriverInfo(Uuid),
    /// A look into the files of the driver in the driver form. The number
    /// tells the answers of earlier looks apart.
    DriverProbe(u64),
    /// Nothing to do with the answer.
    Ignore,
}

/// A message shown in a bar at the top of the window until dismissed.
#[derive(Clone)]
pub struct Notice {
    pub text: String,
    pub error: bool,
}

/// Asks for what is needed to sign in before connecting: the database
/// password, the SSH password, or both.
pub struct PasswordPrompt {
    pub connection: Uuid,
    /// `None` when the database password is known or not needed.
    pub password: Option<String>,
    /// `None` when the SSH password is known or not needed.
    pub ssh: Option<String>,
    /// The jump servers whose password is not known yet, by id.
    pub jumps: Vec<(Uuid, String)>,
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
    /// SSH passwords and key passphrases, kept the same way.
    ssh_secrets: HashMap<Uuid, String>,
    /// The same for jump servers, by the id of the jump server.
    jump_secrets: HashMap<Uuid, String>,
    pub sql_log: Vec<SqlLog>,
    pub engine_log: Vec<EngineLog>,
    pub log_tab: LogTab,
    /// What the engine found in each of the user's own drivers.
    pub driver_checks: HashMap<Uuid, Load<OwnDriverInfo>>,
    /// The driver form, while it is open.
    pub driver_editor: Option<DriverEditor>,
    /// Text typed in the search box of the Drivers page.
    pub driver_search: String,
    /// The kind of database the Drivers page is narrowed to.
    pub driver_kind: Option<DbKind>,
    /// Counts the looks into the driver form's files.
    probes: u64,
    /// The box that names a new group or renames one, while it is open.
    pub group_dialog: Option<GroupDialog>,
    /// The Export box, while it is open.
    pub export: Option<ExportDialog>,
    /// Exports whose file is being written.
    exports: Vec<Receiver<ExportDone>>,
}

/// The time of day, for a log line.
fn now() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

fn push_capped<T>(log: &mut Vec<T>, line: T) {
    if log.len() >= LOG_LINES {
        log.remove(0);
    }
    log.push(line);
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

/// What a look into a driver's files gave. Files without a usable driver
/// class are a failure, with the reason.
fn driver_check(result: Result<dbpadu_engine::Value, String>) -> Load<OwnDriverInfo> {
    match result.and_then(protocol::read::<OwnDriverInfo>) {
        Ok(info) if info.class_name.is_empty() && info.classes.is_empty() => Load::Failed(
            info.error
                .unwrap_or_else(|| "No JDBC driver was found in the files.".to_string()),
        ),
        Ok(info) => Load::Ready(info),
        Err(err) => Load::Failed(err),
    }
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

pub fn category_key(connection: Uuid, schema: &str, category: Category) -> String {
    format!("f:{connection}:{schema}:{}", category.label())
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
            ssh_secrets: HashMap::new(),
            jump_secrets: HashMap::new(),
            sql_log: Vec::new(),
            engine_log: Vec::new(),
            log_tab: LogTab::Sql,
            driver_checks: HashMap::new(),
            driver_editor: None,
            driver_search: String::new(),
            driver_kind: None,
            probes: 0,
            group_dialog: None,
            export: None,
            exports: Vec::new(),
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

    // ---- Logs -------------------------------------------------------

    /// Add a line of the app's own to the engine log.
    pub fn log_engine(&mut self, text: impl Into<String>) {
        push_capped(
            &mut self.engine_log,
            EngineLog {
                time: now(),
                text: text.into(),
            },
        );
    }

    fn log_sql(&mut self, connection: String, sql: String, outcome: Result<String, String>) {
        push_capped(
            &mut self.sql_log,
            SqlLog {
                time: now(),
                connection,
                sql,
                outcome,
            },
        );
    }

    pub fn set_log_open(&mut self, open: bool) {
        if self.data.settings.log_open != open {
            self.data.settings.log_open = open;
            self.save();
        }
    }

    fn connection_name(&self, id: Uuid) -> String {
        self.data
            .connection(id)
            .map(Connection::display_name)
            .unwrap_or_default()
    }

    // ---- Drivers ----------------------------------------------------

    /// The driver inside the engine for a kind of database.
    pub fn bundled_driver(&self, kind: DbKind) -> Option<&Driver> {
        match &self.engine_state {
            EngineState::Ready(hello) => hello
                .drivers
                .iter()
                .find(|driver| driver.kind == kind.engine_name()),
            _ => None,
        }
    }

    /// Ask the engine to look into the files of one of the user's own
    /// drivers: is there a driver in them, and which version.
    pub fn check_driver(&mut self, id: Uuid) {
        let Some(driver) = self.data.driver(id) else {
            self.driver_checks.remove(&id);
            return;
        };
        let request = protocol::driver_info(&driver.library_paths(), driver.class_name.trim());
        if self.request(request, Pending::DriverInfo(id)) {
            self.driver_checks.insert(id, Load::Loading);
        }
    }

    /// Ask the engine to look into the files of the driver in the driver
    /// form. Returns the number the answer will carry, or `None` when the
    /// engine is not running.
    pub fn probe_driver(&mut self, driver: &model::Driver) -> Option<u64> {
        self.probes += 1;
        let request = protocol::driver_info(&driver.library_paths(), driver.class_name.trim());
        self.request(request, Pending::DriverProbe(self.probes))
            .then_some(self.probes)
    }

    /// The version of one of the user's own drivers: the one typed for it,
    /// else the one the engine read from its files.
    pub fn driver_version(&self, driver: &model::Driver) -> String {
        let typed = driver.version.trim();
        if !typed.is_empty() {
            return typed.to_string();
        }
        match self.driver_checks.get(&driver.id) {
            Some(Load::Ready(info)) => info.version.clone(),
            _ => String::new(),
        }
    }

    /// Keep a driver from the driver form. Connections opened from now on
    /// use it as it is now.
    pub fn save_driver(&mut self, driver: model::Driver) {
        let id = driver.id;
        self.data.upsert_driver(driver);
        self.save();
        self.check_driver(id);
        // A connection form that is open moves to the new driver's kind.
        if let Some(editor) = &mut self.editor
            && editor.draft.driver == Some(id)
            && let Some(driver) = self.data.driver(id)
        {
            editor.draft.kind = driver.kind;
        }
    }

    pub fn delete_driver(&mut self, id: Uuid) {
        match self.data.remove_driver(id) {
            Ok(()) => {
                self.driver_checks.remove(&id);
                self.save();
            }
            Err(why) => self.notify(why.to_string()),
        }
    }

    /// Where the keys of SSH servers seen before are remembered.
    pub fn known_hosts(&self) -> Option<std::path::PathBuf> {
        self.store.dir().map(|dir| dir.join("known_hosts"))
    }

    pub fn data_path(&self) -> Option<&std::path::PathBuf> {
        self.store.path()
    }

    // ---- Engine -----------------------------------------------------

    /// Start the engine, or start it again after it stopped. Connections
    /// that were open are gone with the old engine.
    pub fn start_engine(&mut self) {
        self.engine = None;
        self.pending.clear();
        self.links.clear();
        for tab in &mut self.tabs {
            // An export leaves the result on screen as it is.
            if tab.running.take().is_some_and(|running| !running.exporting) {
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
        self.log_engine(format!(
            "[app] starting the engine: {} -jar {}",
            java.display(),
            jar.display()
        ));
        self.driver_checks.clear();
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
                Event::Log(text) => {
                    push_capped(&mut self.engine_log, EngineLog { time: now(), text })
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
            // An export leaves the result on screen as it is.
            if tab.running.take().is_some_and(|running| !running.exporting) {
                tab.outcome = Some(Outcome::Error(
                    "The engine stopped while this was running.".to_string(),
                ));
            }
        }
        let reason = match text.trim() {
            "" => "The engine stopped.".to_string(),
            text => format!("The engine stopped: {text}"),
        };
        self.log_engine(format!("[app] {reason}"));
        self.engine_state = EngineState::Failed(reason);
    }

    fn answer(&mut self, pending: Pending, result: Result<dbpadu_engine::Value, String>) {
        match pending {
            Pending::Ignore => {}
            Pending::Hello => match result.and_then(protocol::read::<Hello>) {
                Ok(hello) => {
                    self.log_engine(format!(
                        "[app] engine {} is ready on Java {}",
                        hello.version, hello.java
                    ));
                    self.engine_state = EngineState::Ready(hello);
                    let ids: Vec<Uuid> = self.data.drivers.iter().map(|d| d.id).collect();
                    for id in ids {
                        self.check_driver(id);
                    }
                }
                Err(err) => self.engine_state = EngineState::Failed(err),
            },
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
                    self.ssh_secrets.remove(&id);
                    self.forget_jump_secrets(id);
                    if let Some(link) = self.links.get_mut(&id) {
                        link.state = LinkState::Failed(err);
                    }
                }
            },
            Pending::Execute(tab_id, name, statement) => {
                let result = result.and_then(protocol::read::<QueryResult>);
                // The log gets the run even when its tab was closed meanwhile.
                match &result {
                    Ok(result) => self.log_sql(
                        name,
                        result.executed_sql.clone(),
                        Ok(ui::statusbar::summary(result)),
                    ),
                    Err(err) => self.log_sql(name, statement.clone(), Err(err.clone())),
                }
                let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
                    return;
                };
                tab.running = None;
                tab.runs += 1;
                tab.grid = GridState::default();
                match result {
                    Ok(result) => {
                        if let Some(link) = self.links.get_mut(&tab.connection) {
                            link.tx = result.tx;
                        }
                        let quote = match self.data.connection(tab.connection).map(|c| c.kind) {
                            Some(DbKind::MySql | DbKind::MariaDb) => dbpadu_export::Quote::Backtick,
                            Some(DbKind::SqlServer) => dbpadu_export::Quote::Bracket,
                            _ => dbpadu_export::Quote::Double,
                        };
                        tab.extract = dbpadu_export::Options {
                            table: dbpadu_export::guess_table(&statement).unwrap_or_default(),
                            quote,
                            ..Default::default()
                        };
                        tab.statement = statement;
                        tab.outcome = Some(Outcome::Result(result));
                    }
                    Err(err) => tab.outcome = Some(Outcome::Error(err)),
                }
            }
            Pending::Export(job) => {
                if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == job.tab) {
                    tab.running = None;
                }
                match result.and_then(protocol::read::<Exported>) {
                    Ok(exported) => {
                        if let Some(link) = self
                            .tabs
                            .iter()
                            .find(|tab| tab.id == job.tab)
                            .and_then(|tab| self.links.get_mut(&tab.connection))
                        {
                            link.tx = exported.tx;
                        }
                        self.log_sql(
                            job.name,
                            job.statement,
                            Ok(format!(
                                "{} read for {} · {}",
                                ui::count(exported.rows as usize, "row", "rows"),
                                job.target.display(),
                                ui::duration(exported.elapsed_ms)
                            )),
                        );
                        // Writing the file can take a while: not here.
                        let (sender, receiver) = mpsc::channel();
                        let ctx = self.ctx.clone();
                        let (rows, target, format, options) =
                            (job.rows, job.target, job.format, job.options);
                        std::thread::spawn(move || {
                            let written = dbpadu_export::convert(&rows, &target, format, &options)
                                .map_err(|err| err.to_string());
                            let _ = std::fs::remove_file(&rows);
                            let _ = sender.send((written, target));
                            ctx.request_repaint();
                        });
                        self.exports.push(receiver);
                    }
                    Err(err) => {
                        let _ = std::fs::remove_file(&job.rows);
                        self.log_sql(job.name, job.statement, Err(err.clone()));
                        self.notice = Some(Notice {
                            text: format!("The export did not finish: {err}"),
                            error: true,
                        });
                    }
                }
            }
            Pending::Tx(id, what) => {
                let name = self.connection_name(id);
                match result.and_then(protocol::read::<TxAnswer>) {
                    Ok(answer) => {
                        if let Some(link) = self.links.get_mut(&id) {
                            link.tx = answer.tx;
                        }
                        self.log_sql(name, what.to_string(), Ok("Done".to_string()));
                    }
                    Err(err) => {
                        self.log_sql(name, what.to_string(), Err(err.clone()));
                        self.notify(err);
                    }
                }
            }
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
                                    routines: Load::Idle,
                                    sequences: Load::Idle,
                                })
                                .collect(),
                        );
                        if let Some(schema) = open {
                            self.expanded.insert(schema_key(id, &schema));
                            self.expanded
                                .insert(category_key(id, &schema, Category::Tables));
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
            Pending::Routines(id, schema) => {
                let Some(node) = self.schema_mut(id, &schema) else {
                    return;
                };
                node.routines = match result.and_then(protocol::read::<Routines>) {
                    Ok(routines) => Load::Ready(routines.items),
                    Err(err) => Load::Failed(err),
                };
            }
            Pending::Sequences(id, schema) => {
                let Some(node) = self.schema_mut(id, &schema) else {
                    return;
                };
                node.sequences = match result.and_then(protocol::read::<Sequences>) {
                    Ok(sequences) => Load::Ready(sequences.items),
                    Err(err) => Load::Failed(err),
                };
            }
            Pending::DriverInfo(id) => {
                // Only if the driver is still there to be told about.
                if self.data.driver(id).is_some() {
                    self.driver_checks.insert(id, driver_check(result));
                }
            }
            Pending::DriverProbe(probe) => {
                if let Some(editor) = &mut self.driver_editor {
                    editor.probed(probe, driver_check(result));
                }
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

    /// Ask the engine to reach a database: a test from the connection form,
    /// or with `session` a connection that stays open.
    fn send_target(
        &mut self,
        connection: &Connection,
        password: &str,
        ssh_secret: &str,
        jump_secrets: &HashMap<Uuid, String>,
        session: Option<Uuid>,
    ) -> bool {
        let driver = self.data.driver_of(connection).cloned();
        if let Some(problem) = connection.problem(driver.as_ref()) {
            self.notify(format!("{}: {problem}", connection.display_name()));
            return false;
        }
        let tunnel = connection.uses_ssh();
        let url = if tunnel {
            connection.jdbc_url_through_tunnel(driver.as_ref())
        } else {
            connection.jdbc_url(driver.as_ref())
        };
        let known_hosts = self
            .known_hosts()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let (jars, class, properties) = match &driver {
            Some(driver) => (
                driver.library_paths(),
                driver.class_name.trim(),
                driver.set_properties(),
            ),
            None => (Vec::new(), "", Vec::new()),
        };
        let ssh = tunnel.then(|| SshTarget {
            host: connection.ssh.host.trim(),
            port: connection.ssh.port,
            user: connection.ssh.username.trim(),
            key_file: match connection.ssh.auth {
                SshAuth::Key => connection.ssh.key_file.trim(),
                SshAuth::Password => "",
            },
            secret: ssh_secret,
            known_hosts: &known_hosts,
            remote_host: connection.host.trim(),
            remote_port: connection.port,
            jumps: connection
                .ssh
                .jumps
                .iter()
                .map(|jump| JumpTarget {
                    host: jump.host.trim(),
                    port: jump.port,
                    user: jump.username.trim(),
                    key_file: match jump.auth {
                        SshAuth::Key => jump.key_file.trim(),
                        SshAuth::Password => "",
                    },
                    secret: jump_secrets.get(&jump.id).map_or("", String::as_str),
                })
                .collect(),
        });
        let target = Target {
            kind: connection.kind.engine_name(),
            url: &url,
            user: connection.username.trim(),
            password,
            ssh,
            driver_jars: &jars,
            driver_class: class,
            properties: &properties,
        };
        match session {
            Some(id) => self.request(
                protocol::connect(&id.to_string(), &target),
                Pending::Connect(id),
            ),
            None => self.request(protocol::test(&target), Pending::Test),
        }
    }

    /// Check the details in the connection form against the database.
    pub fn test_connection(
        &mut self,
        connection: &Connection,
        password: &str,
        ssh_secret: &str,
        jump_secrets: &HashMap<Uuid, String>,
    ) {
        let sent = self.send_target(connection, password, ssh_secret, jump_secrets, None);
        if let Some(editor) = &mut self.editor {
            editor.testing = sent;
            editor.test = if sent {
                None
            } else {
                Some(Err("The engine is not running.".to_string()))
            };
        }
    }

    /// Remember what was typed in the connection form for this run of the
    /// app. An empty value forgets.
    pub fn keep_secrets(&mut self, connection: Uuid, password: String, ssh_secret: String) {
        if password.is_empty() {
            self.passwords.remove(&connection);
        } else {
            self.passwords.insert(connection, password);
        }
        if ssh_secret.is_empty() {
            self.ssh_secrets.remove(&connection);
        } else {
            self.ssh_secrets.insert(connection, ssh_secret);
        }
    }

    /// The same for the jump servers, by the id of the jump server.
    pub fn keep_jump_secrets(&mut self, secrets: impl IntoIterator<Item = (Uuid, String)>) {
        for (jump, secret) in secrets {
            if secret.is_empty() {
                self.jump_secrets.remove(&jump);
            } else {
                self.jump_secrets.insert(jump, secret);
            }
        }
    }

    /// What is known of the jump server passwords of a connection in this
    /// run of the app.
    pub fn known_jump_secrets(&self, connection: &Connection) -> HashMap<Uuid, String> {
        connection
            .ssh
            .jumps
            .iter()
            .filter_map(|jump| Some((jump.id, self.jump_secrets.get(&jump.id)?.clone())))
            .collect()
    }

    fn forget_jump_secrets(&mut self, connection: Uuid) {
        let jumps: Vec<Uuid> = self
            .data
            .connection(connection)
            .map(|connection| connection.ssh.jumps.iter().map(|jump| jump.id).collect())
            .unwrap_or_default();
        for jump in jumps {
            self.jump_secrets.remove(&jump);
        }
    }

    /// Connect to a saved connection, asking for the passwords that are not
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
            self.secrets.get(id, Secret::Password)
        } else {
            None
        };
        // A key may have no passphrase, so only an SSH password is asked
        // for; a passphrase is typed in the connection form.
        let with_key = connection.ssh.auth == SshAuth::Key;
        let ssh = if !connection.uses_ssh() {
            Some(String::new())
        } else if let Some(secret) = self.ssh_secrets.get(&id) {
            Some(secret.clone())
        } else {
            let saved = if connection.save_password {
                self.secrets.get(id, Secret::Ssh)
            } else {
                None
            };
            saved.or(with_key.then(String::new))
        };
        // The jump servers, the same way.
        let mut found = Vec::new();
        let mut asked = Vec::new();
        if connection.uses_ssh() {
            for jump in &connection.ssh.jumps {
                if self.jump_secrets.contains_key(&jump.id) {
                    continue;
                }
                let saved = if connection.save_password {
                    self.secrets.get(id, Secret::Jump(jump.id))
                } else {
                    None
                };
                match saved {
                    Some(secret) => found.push((jump.id, secret)),
                    None if jump.auth == SshAuth::Key => {}
                    None => asked.push((jump.id, String::new())),
                }
            }
        }
        self.keep_jump_secrets(found);
        match (password, ssh) {
            (Some(password), Some(ssh)) if asked.is_empty() => self.connect_with(id, password, ssh),
            (password, ssh) => {
                self.prompt = Some(PasswordPrompt {
                    connection: id,
                    password: password.is_none().then(String::new),
                    ssh: ssh.is_none().then(String::new),
                    jumps: asked,
                    remember: false,
                });
                // What is known already is kept for when the box is answered.
                if let Some(password) = password.filter(|_| needs_password) {
                    self.passwords.insert(id, password);
                }
                if let Some(ssh) = ssh.filter(|ssh| !ssh.is_empty()) {
                    self.ssh_secrets.insert(id, ssh);
                }
            }
        }
    }

    /// Open the connection form on a saved connection.
    pub fn edit_connection(&mut self, id: Uuid) {
        if let Some(connection) = self.data.connection(id) {
            self.editor = Some(ConnectionEditor::edit(
                connection,
                &self.secrets,
                self.known_secrets(id),
                self.known_jump_secrets(connection),
            ));
        }
    }

    /// What is known of a connection's passwords in this run of the app.
    pub fn known_secrets(&self, id: Uuid) -> (String, String) {
        (
            self.passwords.get(&id).cloned().unwrap_or_default(),
            self.ssh_secrets.get(&id).cloned().unwrap_or_default(),
        )
    }

    pub fn connect_with(&mut self, id: Uuid, password: String, ssh_secret: String) {
        let Some(connection) = self.data.connection(id).cloned() else {
            return;
        };
        let jump_secrets = self.known_jump_secrets(&connection);
        if self.send_target(&connection, &password, &ssh_secret, &jump_secrets, Some(id)) {
            self.keep_secrets(id, password, ssh_secret);
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
        self.ssh_secrets.remove(&id);
        if let Some(connection) = self.data.connection(id) {
            for jump in &connection.ssh.jumps {
                self.jump_secrets.remove(&jump.id);
                self.secrets.delete(id, Secret::Jump(jump.id));
            }
        }
        self.secrets.delete(id, Secret::Password);
        self.secrets.delete(id, Secret::Ssh);
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

    /// Fetch what a folder of the tree lists, the first time it is opened.
    pub fn load_category(&mut self, id: Uuid, schema: &str, category: Category) {
        let mode = match self.links.get(&id) {
            Some(link) => link.mode.clone(),
            None => return,
        };
        let session = id.to_string();
        match category {
            Category::Tables | Category::Views => self.load_tables(id, schema),
            Category::Routines => {
                let Some(node) = self.schema_mut(id, schema) else {
                    return;
                };
                if matches!(node.routines, Load::Loading | Load::Ready(_)) {
                    return;
                }
                node.routines = Load::Loading;
                self.request(
                    protocol::routines(&session, &mode, schema),
                    Pending::Routines(id, schema.to_string()),
                );
            }
            Category::Sequences => {
                let Some(node) = self.schema_mut(id, schema) else {
                    return;
                };
                if matches!(node.sequences, Load::Loading | Load::Ready(_)) {
                    return;
                }
                node.sequences = Load::Loading;
                self.request(
                    protocol::sequences(&session, schema),
                    Pending::Sequences(id, schema.to_string()),
                );
            }
        }
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
            statement: String::new(),
            runs: 0,
            grid: GridState::default(),
            extract: dbpadu_export::Options::default(),
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
        let name = self.connection_name(connection);
        if self.request(request, Pending::Execute(tab_id, name, statement)) {
            self.tab_mut(tab_id).running = Some(Running {
                since: Instant::now(),
                exporting: false,
            });
        }
    }

    /// Write every row of a tab's result to a file: the SQL runs again as
    /// the user wrote it, without the row limit, and the rows go from the
    /// database to the file without being held in memory.
    pub fn export_all(
        &mut self,
        tab_id: u64,
        target: PathBuf,
        format: dbpadu_export::Format,
        options: dbpadu_export::Options,
    ) {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == tab_id) else {
            return;
        };
        if tab.running.is_some() {
            return;
        }
        let connection = tab.connection;
        if !self.links.get(&connection).is_some_and(Link::connected) {
            self.notify("Connect first: the export reads the rows from the database.".to_string());
            return;
        }
        let statement = tab.statement.clone();
        let rows = std::env::temp_dir().join(format!("dbpadu-export-{}.jsonl", Uuid::new_v4()));
        let request =
            protocol::export(&connection.to_string(), &statement, &rows.to_string_lossy());
        let job = ExportJob {
            tab: tab_id,
            name: self.connection_name(connection),
            statement,
            rows,
            target,
            format,
            options,
        };
        if self.request(request, Pending::Export(Box::new(job))) {
            self.tab_mut(tab_id).running = Some(Running {
                since: Instant::now(),
                exporting: true,
            });
        }
    }

    /// Tell how the exports that were writing their file ended.
    fn poll_exports(&mut self) {
        let mut done = Vec::new();
        self.exports.retain(|export| match export.try_recv() {
            Ok(result) => {
                done.push(result);
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => false,
        });
        for (written, target) in done {
            self.export_done(written, &target);
        }
    }

    /// Say in the bar at the top how an export ended.
    pub fn export_done(&mut self, written: Result<u64, String>, target: &std::path::Path) {
        self.notice = Some(match written {
            Ok(rows) => Notice {
                text: format!(
                    "Exported {} to {}",
                    ui::count(rows as usize, "row", "rows"),
                    target.display()
                ),
                error: false,
            },
            Err(err) => Notice {
                text: format!("{} could not be written: {err}", target.display()),
                error: true,
            },
        });
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
            Pending::Tx(
                connection,
                if value {
                    "-- auto-commit on"
                } else {
                    "-- auto-commit off"
                },
            ),
        );
    }

    pub fn commit(&mut self, connection: Uuid) {
        self.request(
            protocol::commit(&connection.to_string()),
            Pending::Tx(connection, "COMMIT"),
        );
    }

    pub fn rollback(&mut self, connection: Uuid) {
        self.request(
            protocol::rollback(&connection.to_string()),
            Pending::Tx(connection, "ROLLBACK"),
        );
    }

    /// The tab that is shown, if the page is a tab.
    pub fn current_tab(&self) -> Option<&QueryTab> {
        match self.view {
            View::Tab(id) => self.tabs.iter().find(|tab| tab.id == id),
            _ => None,
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.follow_system_appearance();
        self.poll_engine();
        self.poll_exports();
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

        // After the tree, so the log sits under the page, not under the tree.
        if self.data.settings.log_open {
            egui::Panel::bottom("log")
                .default_size(170.0)
                .size_range(90.0..=500.0)
                .resizable(true)
                .frame(
                    egui::Frame::new()
                        .fill(theme.panel)
                        .inner_margin(egui::Margin::symmetric(10, 6)),
                )
                .show(ui, |ui| ui::log::show(self, ui));
        }

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

        let page = egui::Frame::new()
            .fill(theme.bg)
            .inner_margin(egui::Margin::same(24));
        match self.view {
            View::Tab(id) if self.tabs.iter().any(|tab| tab.id == id) => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(theme.bg))
                    .show(ui, |ui| ui::query::show(self, ui, id));
            }
            View::Drivers => {
                egui::CentralPanel::default()
                    .frame(page)
                    .show(ui, |ui| ui::drivers::show(self, ui));
            }
            View::Settings => {
                egui::CentralPanel::default()
                    .frame(page)
                    .show(ui, |ui| ui::settings::show(self, ui));
            }
            View::About => {
                egui::CentralPanel::default()
                    .frame(page)
                    .show(ui, |ui| ui::about::show(self, ui));
            }
            _ => {
                self.view = View::Home;
                egui::CentralPanel::default()
                    .frame(page)
                    .show(ui, |ui| ui::home::show(self, ui));
            }
        }

        ui::editor::show(self, ui.ctx());
        // After the connection form: a driver can be added from there.
        ui::drivers::editor(self, ui.ctx());
        ui::editor::password_prompt(self, ui.ctx());
        ui::home::group_dialog(self, ui.ctx());
        ui::export::dialog(self, ui.ctx());
    }
}
