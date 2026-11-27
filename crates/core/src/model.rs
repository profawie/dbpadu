//! Saved connections, their groups and settings.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stands for the tunnel's port in an address sent to the engine.
pub const TUNNEL_PORT: &str = "${TUNNEL_PORT}";
/// Between the levels of a group's path.
pub const GROUP_SEPARATOR: char = '/';
/// Colors a group can be given, as `#RRGGBB`.
pub const GROUP_COLORS: [(&str, &str); 8] = [
    ("Red", "#D9363E"),
    ("Orange", "#E8710A"),
    ("Yellow", "#C99A00"),
    ("Green", "#2E9E52"),
    ("Teal", "#1A9E9E"),
    ("Blue", "#2F7FE0"),
    ("Purple", "#8B5CF6"),
    ("Pink", "#D6409F"),
];

/// Rows a SELECT may return when nothing else is chosen.
pub const DEFAULT_ROW_LIMIT: u32 = 500;
/// The choices offered for the row limit. The engine refuses more than the
/// last one.
pub const ROW_LIMITS: [u32; 5] = [100, 500, 1_000, 5_000, 10_000];

/// The kinds of database dbpadu can connect to.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "lowercase")]
pub enum DbKind {
    #[default]
    Postgres,
    MySql,
    MariaDb,
    SqlServer,
    Oracle,
    Sqlite,
    /// Any other database, through a JDBC driver the user adds. There is no
    /// driver for it inside the engine.
    Other,
}

impl DbKind {
    /// The kinds with a driver inside the engine.
    pub const ALL: [DbKind; 6] = [
        DbKind::Postgres,
        DbKind::MySql,
        DbKind::MariaDb,
        DbKind::SqlServer,
        DbKind::Oracle,
        DbKind::Sqlite,
    ];

    /// The kinds a driver of the user's own can be for.
    pub const WITH_OTHER: [DbKind; 7] = [
        DbKind::Postgres,
        DbKind::MySql,
        DbKind::MariaDb,
        DbKind::SqlServer,
        DbKind::Oracle,
        DbKind::Sqlite,
        DbKind::Other,
    ];

    /// Name shown to the user.
    pub fn label(self) -> &'static str {
        match self {
            DbKind::Postgres => "PostgreSQL",
            DbKind::MySql => "MySQL",
            DbKind::MariaDb => "MariaDB",
            DbKind::SqlServer => "SQL Server",
            DbKind::Oracle => "Oracle",
            DbKind::Sqlite => "SQLite",
            DbKind::Other => "Other",
        }
    }

    /// Two or three letters for the badge on a card.
    pub fn short(self) -> &'static str {
        match self {
            DbKind::Postgres => "PG",
            DbKind::MySql => "MY",
            DbKind::MariaDb => "MA",
            DbKind::SqlServer => "MS",
            DbKind::Oracle => "OR",
            DbKind::Sqlite => "SL",
            DbKind::Other => "DB",
        }
    }

    /// Name the engine knows this kind by.
    pub fn engine_name(self) -> &'static str {
        match self {
            DbKind::Postgres => "postgres",
            DbKind::MySql => "mysql",
            DbKind::MariaDb => "mariadb",
            DbKind::SqlServer => "sqlserver",
            DbKind::Oracle => "oracle",
            DbKind::Sqlite => "sqlite",
            DbKind::Other => "other",
        }
    }

    /// `None` for a database that is a file, and for one whose port is
    /// not known.
    pub fn default_port(self) -> Option<u16> {
        match self {
            DbKind::Postgres => Some(5432),
            DbKind::MySql | DbKind::MariaDb => Some(3306),
            DbKind::SqlServer => Some(1433),
            DbKind::Oracle => Some(1521),
            DbKind::Sqlite | DbKind::Other => None,
        }
    }

    /// A database that is a file on this computer, not a server.
    pub fn is_file(self) -> bool {
        self == DbKind::Sqlite
    }

    /// The address of a database of this kind, with `{host}`, `{port}` and
    /// `{database}` where the connection's details go. See
    /// [`expand_template`].
    pub fn url_template(self) -> &'static str {
        match self {
            DbKind::Postgres => "jdbc:postgresql://{host}:{port}/{database}",
            DbKind::MySql => "jdbc:mysql://{host}:{port}/{database}",
            DbKind::MariaDb => "jdbc:mariadb://{host}:{port}/{database}",
            DbKind::SqlServer => "jdbc:sqlserver://{host}:{port}[;databaseName={database}]",
            DbKind::Oracle => "jdbc:oracle:thin:@//{host}:{port}/{database}",
            DbKind::Sqlite => "jdbc:sqlite:{database}",
            DbKind::Other => "",
        }
    }

    pub fn default_user(self) -> &'static str {
        match self {
            DbKind::Postgres => "postgres",
            DbKind::MySql | DbKind::MariaDb => "root",
            DbKind::SqlServer => "sa",
            DbKind::Oracle => "system",
            DbKind::Sqlite | DbKind::Other => "",
        }
    }

    /// What the "Database" field is called for this kind.
    pub fn database_label(self) -> &'static str {
        match self {
            DbKind::Oracle => "Service name",
            DbKind::Sqlite => "Database file",
            _ => "Database",
        }
    }

    /// Extra settings a new connection starts with. They are shown in the
    /// connection form, so nothing is switched on out of sight.
    pub fn default_params(self) -> &'static str {
        match self {
            // The driver insists on encryption; most development servers
            // have a certificate it cannot check.
            DbKind::SqlServer => "encrypt=true;trustServerCertificate=true",
            _ => "",
        }
    }
}

/// One saved connection. The password is never part of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Connection {
    pub id: Uuid,
    pub name: String,
    pub kind: DbKind,
    /// The driver of the user's own it connects with. `None`: the driver
    /// for its kind inside the engine.
    pub driver: Option<Uuid>,
    pub host: String,
    pub port: u16,
    /// Database name, Oracle service name, or the path of a SQLite file.
    pub database: String,
    pub username: String,
    /// Keep the password in the operating system's credential store.
    pub save_password: bool,
    /// Extra driver settings, written as they appear in the JDBC URL.
    pub params: String,
    /// The group it is filed under, such as `Company1/Production`. Empty
    /// for a connection without a group.
    pub group: String,
    /// Reach the database through an SSH server.
    pub ssh: Ssh,
}

/// How to sign in to the SSH server of a tunnel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SshAuth {
    #[default]
    Password,
    /// A private key file, with or without a passphrase.
    Key,
}

/// A jump server: an SSH server to go through first to reach the next
/// one (what OpenSSH calls ProxyJump). The password or passphrase is never
/// part of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Jump {
    /// Names its password in the credential store.
    pub id: Uuid,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SshAuth,
    pub key_file: String,
}

impl Default for Jump {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            host: String::new(),
            port: 22,
            username: String::new(),
            auth: SshAuth::default(),
            key_file: String::new(),
        }
    }
}

/// An SSH tunnel: the engine signs in to `host` and reaches the database
/// from there. The password or passphrase is never part of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ssh {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SshAuth,
    pub key_file: String,
    /// Servers to go through to reach `host`, in the order they are
    /// passed: the first is reached from this computer. Empty when `host`
    /// is reached directly.
    pub jumps: Vec<Jump>,
}

impl Default for Ssh {
    fn default() -> Self {
        Self {
            enabled: false,
            host: String::new(),
            port: 22,
            username: String::new(),
            auth: SshAuth::default(),
            key_file: String::new(),
            jumps: Vec::new(),
        }
    }
}

impl Default for Connection {
    fn default() -> Self {
        Self::new(DbKind::default())
    }
}

impl Connection {
    pub fn new(kind: DbKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: String::new(),
            kind,
            driver: None,
            host: "localhost".to_string(),
            port: kind.default_port().unwrap_or(0),
            database: String::new(),
            username: kind.default_user().to_string(),
            save_password: false,
            params: kind.default_params().to_string(),
            group: String::new(),
            ssh: Ssh::default(),
        }
    }

    /// Whether it goes through an SSH tunnel. A file needs none.
    pub fn uses_ssh(&self) -> bool {
        self.ssh.enabled && !self.kind.is_file()
    }

    /// Switch to another driver: the one inside the engine for `kind`, or
    /// `driver`, one of the user's own. `old` is the user's own driver it
    /// had until now, if any. Fields that still hold the old driver's
    /// defaults move to the new one's; anything the user typed stays.
    pub fn set_driver(&mut self, old: Option<&Driver>, kind: DbKind, driver: Option<&Driver>) {
        let before = Defaults::of(self.kind, old);
        let after = Defaults::of(kind, driver);
        if self.port == before.port {
            self.port = after.port;
        }
        if self.username == before.user {
            self.username = after.user;
        }
        if self.params == before.params {
            self.params = after.params;
        }
        if self.kind.is_file() != kind.is_file() {
            self.database.clear();
        }
        if self.database == before.database {
            self.database = after.database;
        }
        self.kind = kind;
        self.driver = driver.map(|driver| driver.id);
    }

    /// Switch to the driver inside the engine for another kind of database.
    pub fn set_kind(&mut self, kind: DbKind) {
        self.set_driver(None, kind, None);
    }

    /// The name on the card: the name given, or where it connects to.
    pub fn display_name(&self) -> String {
        let name = self.name.trim();
        if name.is_empty() {
            self.target()
        } else {
            name.to_string()
        }
    }

    /// Where it connects to, in short: `user@host:port/database` or a file.
    pub fn target(&self) -> String {
        if self.kind.is_file() {
            return self.database.trim().to_string();
        }
        let mut text = String::new();
        if !self.username.trim().is_empty() {
            text.push_str(self.username.trim());
            text.push('@');
        }
        text.push_str(self.host.trim());
        text.push_str(&format!(":{}", self.port));
        if !self.database.trim().is_empty() {
            text.push('/');
            text.push_str(self.database.trim());
        }
        text
    }

    /// The address the engine's JDBC driver connects to. `driver` is the
    /// user's own driver of this connection, if it has one.
    pub fn jdbc_url(&self, driver: Option<&Driver>) -> String {
        self.jdbc_url_at(driver, self.host.trim(), &self.port.to_string())
    }

    /// The address when the database is reached through an SSH tunnel: the
    /// tunnel's end on this computer. The engine fills in the port once the
    /// tunnel is open.
    pub fn jdbc_url_through_tunnel(&self, driver: Option<&Driver>) -> String {
        self.jdbc_url_at(driver, "127.0.0.1", TUNNEL_PORT)
    }

    fn jdbc_url_at(&self, driver: Option<&Driver>, host: &str, port: &str) -> String {
        let template = match driver {
            Some(driver) => driver.url_template.trim(),
            None => self.kind.url_template(),
        };
        // No port typed: the optional parts of the template that need it go.
        let port = if port == "0" { "" } else { port };
        let url = expand_template(
            template,
            host,
            port,
            self.database.trim(),
            self.username.trim(),
        );
        let params = self.params.trim().trim_start_matches(['?', ';', '&']);
        if params.is_empty() {
            return url;
        }
        match self.kind {
            DbKind::SqlServer => format!("{url};{params}"),
            // The thin driver's address has no place for them.
            DbKind::Oracle => url,
            _ if url.contains('?') => format!("{url}&{params}"),
            _ => format!("{url}?{params}"),
        }
    }

    /// What is still missing before it can be saved, in words for the user.
    /// `driver` is the user's own driver of this connection, if it has one.
    pub fn problem(&self, driver: Option<&Driver>) -> Option<&'static str> {
        if self.driver.is_some() != driver.is_some()
            || (self.kind == DbKind::Other && driver.is_none())
        {
            return Some("Choose a driver. The one this connection used was deleted.");
        }
        if self.kind.is_file() {
            if self.database.trim().is_empty() {
                return Some("Choose the database file.");
            }
            return None;
        }
        // One of the user's own drivers says in its template what it needs.
        let template = driver.map(|driver| driver.url_template.as_str());
        let needs = |name: &str| template.is_none_or(|template| required(template, name));
        if self.host.trim().is_empty() && (needs("host") || self.ssh.enabled) {
            return Some("Enter the host.");
        }
        if self.port == 0 && (needs("port") || self.ssh.enabled) {
            return Some("Enter the port.");
        }
        if self.kind == DbKind::Oracle && self.database.trim().is_empty() {
            return Some("Enter the service name.");
        }
        if self.ssh.enabled {
            if self.ssh.host.trim().is_empty() {
                return Some("Enter the SSH host.");
            }
            if self.ssh.username.trim().is_empty() {
                return Some("Enter the SSH user name.");
            }
            if self.ssh.auth == SshAuth::Key && self.ssh.key_file.trim().is_empty() {
                return Some("Choose the SSH private key file.");
            }
            for jump in &self.ssh.jumps {
                if jump.host.trim().is_empty() {
                    return Some("Enter the host of the jump server.");
                }
                if jump.port == 0 {
                    return Some("Enter the port of the jump server.");
                }
                if jump.username.trim().is_empty() {
                    return Some("Enter the user name of the jump server.");
                }
                if jump.auth == SshAuth::Key && jump.key_file.trim().is_empty() {
                    return Some("Choose the private key file of the jump server.");
                }
            }
        }
        None
    }
}

/// What a new connection starts with.
struct Defaults {
    port: u16,
    user: String,
    database: String,
    params: String,
}

impl Defaults {
    fn of(kind: DbKind, driver: Option<&Driver>) -> Self {
        match driver {
            Some(driver) => Self {
                port: driver.default_port,
                user: driver.default_user.trim().to_string(),
                database: driver.default_database.trim().to_string(),
                params: String::new(),
            },
            None => Self {
                port: kind.default_port().unwrap_or(0),
                user: kind.default_user().to_string(),
                database: String::new(),
                params: kind.default_params().to_string(),
            },
        }
    }
}

/// A setting handed to a JDBC driver when it connects, such as
/// `useSSL` = `false`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Property {
    pub name: String,
    pub value: String,
}

/// A JDBC driver the user added: its `.jar` files and how to connect with
/// it. Several drivers for the same kind of database live side by side, for
/// example one for MySQL 5 and one for MySQL 8; each connection names the
/// one it uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Driver {
    pub id: Uuid,
    /// The name it is chosen by, such as `MySQL 5`.
    pub name: String,
    /// The version to show next to the name. Empty: the version the engine
    /// reads from the `.jar` file.
    pub version: String,
    /// The kind of database it talks to. Decides how the row limit is
    /// written and how the tree is read.
    pub kind: DbKind,
    /// The driver class. Empty: the one the `.jar` file announces.
    pub class_name: String,
    /// See [`expand_template`].
    pub url_template: String,
    /// 0 for none.
    pub default_port: u16,
    pub default_database: String,
    pub default_user: String,
    pub description: String,
    /// `.jar` files, and folders of `.jar` files.
    pub libraries: Vec<String>,
    /// Handed to the driver with every connection.
    pub properties: Vec<Property>,
}

/// What a field missing from the saved file is read as: nothing. A new
/// driver starts from [`Driver::new`] instead.
impl Default for Driver {
    fn default() -> Self {
        Self {
            url_template: String::new(),
            default_port: 0,
            default_user: String::new(),
            ..Self::new(DbKind::default())
        }
    }
}

impl Driver {
    /// A driver for `kind`, set up like the one inside the engine.
    pub fn new(kind: DbKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: String::new(),
            version: String::new(),
            kind,
            class_name: String::new(),
            url_template: kind.url_template().to_string(),
            default_port: kind.default_port().unwrap_or(0),
            default_database: String::new(),
            default_user: kind.default_user().to_string(),
            description: String::new(),
            libraries: Vec::new(),
            properties: Vec::new(),
        }
    }

    /// Switch to another kind of database. The template, port and user move
    /// to the new kind's while they still hold the old kind's.
    pub fn set_kind(&mut self, kind: DbKind) {
        let old = self.kind;
        if self.url_template.trim() == old.url_template() {
            self.url_template = kind.url_template().to_string();
        }
        if self.default_port == old.default_port().unwrap_or(0) {
            self.default_port = kind.default_port().unwrap_or(0);
        }
        if self.default_user.trim() == old.default_user() {
            self.default_user = kind.default_user().to_string();
        }
        self.kind = kind;
    }

    /// The `.jar` files and folders, without empty lines.
    pub fn library_paths(&self) -> Vec<String> {
        self.libraries
            .iter()
            .map(|path| path.trim().to_string())
            .filter(|path| !path.is_empty())
            .collect()
    }

    /// The properties that have a name.
    pub fn set_properties(&self) -> Vec<(String, String)> {
        self.properties
            .iter()
            .filter(|property| !property.name.trim().is_empty())
            .map(|property| (property.name.trim().to_string(), property.value.clone()))
            .collect()
    }

    /// An address made from the template, to show what it gives.
    pub fn example_url(&self) -> String {
        let port = match self.default_port {
            0 => String::new(),
            port => port.to_string(),
        };
        let database = match self.default_database.trim() {
            "" if self.kind.is_file() => "shop.db",
            "" => "shop",
            database => database,
        };
        expand_template(
            self.url_template.trim(),
            "localhost",
            &port,
            database,
            self.default_user.trim(),
        )
    }

    /// What is still missing before it can be saved, in words for the user.
    pub fn problem(&self) -> Option<&'static str> {
        if self.name.trim().is_empty() {
            return Some("Enter a name for the driver.");
        }
        if self.library_paths().is_empty() {
            return Some("Add the driver's .jar file.");
        }
        let template = self.url_template.trim();
        if template.is_empty() {
            return Some("Enter the URL template.");
        }
        if !template.starts_with("jdbc:") {
            return Some("The URL template starts with jdbc:");
        }
        if template.matches('[').count() != template.matches(']').count() {
            return Some("The URL template has a [ without its ].");
        }
        None
    }
}

/// The value a `{name}` of a template stands for, if the name is known.
fn template_value<'a>(
    name: &str,
    host: &'a str,
    port: &'a str,
    database: &'a str,
    user: &'a str,
) -> Option<&'a str> {
    match name {
        "host" | "server" => Some(host),
        "port" => Some(port),
        "database" | "file" | "folder" => Some(database),
        "user" => Some(user),
        _ => None,
    }
}

/// Fill in a URL template, the way DBeaver writes them:
///
/// - `{host}`, `{port}`, `{database}` and `{user}` are replaced by the
///   connection's details (`{server}` and `{file}` work too);
/// - a part in `[` `]` is left out when something it names is empty, so
///   `jdbc:mysql://{host}[:{port}]/[{database}]` gives
///   `jdbc:mysql://localhost/` for a connection without port and database.
///
/// Anything else, an unknown `{name}` included, is kept as it is.
pub fn expand_template(
    template: &str,
    host: &str,
    port: &str,
    database: &str,
    user: &str,
) -> String {
    // One entry per open `[`: the text so far and whether it stays.
    let mut stack: Vec<(String, bool)> = vec![(String::new(), true)];
    let mut rest = template;
    while let Some(c) = rest.chars().next() {
        rest = &rest[c.len_utf8()..];
        match c {
            '[' => stack.push((String::new(), true)),
            ']' if stack.len() > 1 => {
                let (text, keep) = stack.pop().expect("more than one");
                if keep {
                    stack.last_mut().expect("the base").0.push_str(&text);
                }
            }
            '{' => {
                let value = rest.split_once('}').and_then(|(name, after)| {
                    template_value(name, host, port, database, user).map(|value| (value, after))
                });
                let top = stack.last_mut().expect("the base");
                match value {
                    Some((value, after)) => {
                        top.0.push_str(value);
                        top.1 &= !value.is_empty();
                        rest = after;
                    }
                    None => top.0.push(c),
                }
            }
            c => stack.last_mut().expect("the base").0.push(c),
        }
    }
    // A `[` that was never closed: keep what it holds.
    let mut url = String::new();
    for (text, _) in stack {
        url.push_str(&text);
    }
    url
}

/// Whether a template cannot do without `{name}`: it names it outside
/// every `[` `]`.
pub fn required(template: &str, name: &str) -> bool {
    let mut depth = 0usize;
    let mut rest = template;
    let wanted = format!("{{{name}}}");
    while let Some(c) = rest.chars().next() {
        if depth == 0 && rest.starts_with(&wanted) {
            return true;
        }
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
        rest = &rest[c.len_utf8()..];
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Dark,
    Light,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::System, Appearance::Dark, Appearance::Light];

    pub fn label(self) -> &'static str {
        match self {
            Appearance::System => "System",
            Appearance::Dark => "Dark",
            Appearance::Light => "Light",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub appearance: Appearance,
    /// Row limit a new query tab starts with.
    pub row_limit: u32,
    /// The `java` program to start the engine with. Empty: look in
    /// `JAVA_HOME`, then on `PATH`.
    pub java_path: String,
    /// From version 0.1.1: one driver `.jar` per kind of database. Turned
    /// into [`Data::drivers`] when the file is loaded, and not written again.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub drivers: BTreeMap<DbKind, OwnDriver>,
    /// Whether the log panel at the bottom is open.
    pub log_open: bool,
    /// The format Ctrl+C in the result grid copies in, by its name in the
    /// export library. Empty: the first one (tabs, for spreadsheets).
    pub copy_format: String,
    /// The format the Export box opens with, the same way.
    pub export_format: String,
}

/// How version 0.1.1 kept a JDBC driver of the user's choosing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct OwnDriver {
    pub jar: String,
    /// The driver class. Empty: the one the jar announces.
    pub class_name: String,
}

/// A group of connections. Groups nest: `Company1/Production` is inside
/// `Company1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Group {
    /// Full path, levels joined by [`GROUP_SEPARATOR`].
    pub path: String,
    /// `#RRGGBB`. `None` takes the color of the group it is inside.
    pub color: Option<String>,
    pub collapsed: bool,
}

/// A group path tidied up: levels trimmed, empty levels dropped.
pub fn clean_group(path: &str) -> String {
    path.split([GROUP_SEPARATOR, '>'])
        .map(str::trim)
        .filter(|level| !level.is_empty())
        .collect::<Vec<_>>()
        .join(&GROUP_SEPARATOR.to_string())
}

/// The last level of a group path.
pub fn group_name(path: &str) -> &str {
    path.rsplit(GROUP_SEPARATOR).next().unwrap_or(path)
}

/// The path of the group this one is inside, if any.
pub fn group_parent(path: &str) -> Option<&str> {
    path.rsplit_once(GROUP_SEPARATOR).map(|(parent, _)| parent)
}

/// How many groups this one is inside.
pub fn group_depth(path: &str) -> usize {
    path.matches(GROUP_SEPARATOR).count()
}

/// A group path for reading: `Company1 › Production`.
pub fn group_label(path: &str) -> String {
    path.replace(GROUP_SEPARATOR, " › ")
}

/// Whether `path` is `group` or inside it.
pub fn in_group(path: &str, group: &str) -> bool {
    path == group
        || path
            .strip_prefix(group)
            .is_some_and(|rest| rest.starts_with(GROUP_SEPARATOR))
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            row_limit: DEFAULT_ROW_LIMIT,
            java_path: String::new(),
            drivers: BTreeMap::new(),
            log_open: true,
            copy_format: String::new(),
            export_format: String::new(),
        }
    }
}

/// Everything that is saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Data {
    pub connections: Vec<Connection>,
    pub groups: Vec<Group>,
    /// The JDBC drivers the user added.
    pub drivers: Vec<Driver>,
    pub settings: Settings,
    /// The connections are in the order the user gave their cards. A file
    /// from before 0.3.1 does not say so: its cards were shown by name.
    pub cards_ordered: bool,
}

impl Data {
    pub fn driver(&self, id: Uuid) -> Option<&Driver> {
        self.drivers.iter().find(|driver| driver.id == id)
    }

    /// The user's own driver a connection uses. `None` for the driver
    /// inside the engine, and for a driver that was deleted.
    pub fn driver_of(&self, connection: &Connection) -> Option<&Driver> {
        connection.driver.and_then(|id| self.driver(id))
    }

    /// Add the driver, or replace the saved one with the same id. The list
    /// stays sorted by kind of database, then by name.
    pub fn upsert_driver(&mut self, mut driver: Driver) {
        driver.name = driver.name.trim().to_string();
        driver.libraries = driver.library_paths();
        driver
            .properties
            .retain(|property| !property.name.trim().is_empty());
        // Connections follow the driver to another kind of database.
        for connection in &mut self.connections {
            if connection.driver == Some(driver.id) {
                connection.kind = driver.kind;
            }
        }
        match self.drivers.iter_mut().find(|saved| saved.id == driver.id) {
            Some(saved) => *saved = driver,
            None => self.drivers.push(driver),
        }
        self.drivers
            .sort_by_cached_key(|driver| (driver.kind, driver.name.to_lowercase()));
    }

    /// How many connections use one of the user's own drivers.
    pub fn driver_uses(&self, id: Uuid) -> usize {
        self.connections
            .iter()
            .filter(|connection| connection.driver == Some(id))
            .count()
    }

    /// Delete a driver. Refused while a connection uses it.
    pub fn remove_driver(&mut self, id: Uuid) -> Result<(), &'static str> {
        if self.driver_uses(id) > 0 {
            return Err("A connection uses this driver. Give the connection another driver first.");
        }
        self.drivers.retain(|driver| driver.id != id);
        Ok(())
    }

    /// A copy of a driver under a new name, to change into another version.
    pub fn duplicate_driver(&self, id: Uuid) -> Option<Driver> {
        let driver = self.driver(id)?;
        Some(Driver {
            id: Uuid::new_v4(),
            name: format!("{} (copy)", driver.name),
            ..driver.clone()
        })
    }

    /// Bring a file of an older version up to date: the one driver `.jar`
    /// per kind of 0.1.1 becomes a driver in the list, and the connections
    /// of that kind use it, as they did.
    pub fn migrate(&mut self) {
        if !self.cards_ordered {
            // Up to 0.3.0 the cards were shown by name; start from that.
            self.connections
                .sort_by_cached_key(|connection| connection.display_name().to_lowercase());
            self.cards_ordered = true;
        }
        for (kind, own) in std::mem::take(&mut self.settings.drivers) {
            if own.jar.trim().is_empty() {
                continue;
            }
            let driver = Driver {
                name: format!("{} (own driver)", kind.label()),
                class_name: own.class_name.trim().to_string(),
                libraries: vec![own.jar.trim().to_string()],
                ..Driver::new(kind)
            };
            for connection in &mut self.connections {
                if connection.kind == kind && connection.driver.is_none() {
                    connection.driver = Some(driver.id);
                }
            }
            self.upsert_driver(driver);
        }
    }

    pub fn connection(&self, id: Uuid) -> Option<&Connection> {
        self.connections
            .iter()
            .find(|connection| connection.id == id)
    }

    /// Add the connection, or replace the saved one with the same id.
    pub fn upsert(&mut self, mut connection: Connection) {
        connection.group = clean_group(&connection.group);
        match self
            .connections
            .iter_mut()
            .find(|saved| saved.id == connection.id)
        {
            Some(saved) => *saved = connection,
            None => self.connections.push(connection),
        }
        self.sync_groups();
    }

    /// Put a connection's card in `group`, in front of the card of `before`,
    /// or after the last card of the group for `None`. Returns whether
    /// anything changed.
    pub fn move_connection(&mut self, id: Uuid, group: &str, before: Option<Uuid>) -> bool {
        if before == Some(id) {
            return false;
        }
        let Some(from) = self.connections.iter().position(|c| c.id == id) else {
            return false;
        };
        let old = self.connections.clone();
        let mut moved = self.connections.remove(from);
        moved.group = clean_group(group);
        let to = before
            .and_then(|before| self.connections.iter().position(|c| c.id == before))
            .or_else(|| {
                // Right after the last card of the group, so the cards of a
                // group stay together in the file too.
                self.connections
                    .iter()
                    .rposition(|c| c.group == moved.group)
                    .map(|last| last + 1)
            })
            .unwrap_or(self.connections.len());
        self.connections.insert(to, moved);
        if self.connections == old {
            return false;
        }
        self.sync_groups();
        true
    }

    pub fn remove(&mut self, id: Uuid) {
        self.connections.retain(|connection| connection.id != id);
    }

    /// Make sure every group a connection is in exists, with the groups it
    /// is inside, and keep the list in tree order.
    pub fn sync_groups(&mut self) {
        for group in &mut self.groups {
            group.path = clean_group(&group.path);
        }
        let mut wanted: Vec<String> = self
            .groups
            .iter()
            .map(|group| group.path.clone())
            .chain(self.connections.iter().map(|c| c.group.clone()))
            .filter(|path| !path.is_empty())
            .collect();
        // Every level above a group is a group too.
        let mut parents = Vec::new();
        for path in &wanted {
            let mut path = path.as_str();
            while let Some(parent) = group_parent(path) {
                parents.push(parent.to_string());
                path = parent;
            }
        }
        wanted.extend(parents);
        for path in wanted {
            if !self.groups.iter().any(|group| group.path == path) {
                self.groups.push(Group {
                    path,
                    ..Group::default()
                });
            }
        }
        self.groups.retain(|group| !group.path.is_empty());
        // Tree order: a group, then what is inside it, by name.
        self.groups.sort_by_cached_key(|group| {
            group
                .path
                .split(GROUP_SEPARATOR)
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
        });
        self.groups.dedup_by(|a, b| a.path == b.path);
    }

    pub fn group(&self, path: &str) -> Option<&Group> {
        self.groups.iter().find(|group| group.path == path)
    }

    pub fn group_mut(&mut self, path: &str) -> Option<&mut Group> {
        self.groups.iter_mut().find(|group| group.path == path)
    }

    /// Add an empty group. Returns its tidied path, or `None` if the name
    /// is empty.
    pub fn add_group(&mut self, path: &str) -> Option<String> {
        let path = clean_group(path);
        if path.is_empty() {
            return None;
        }
        if self.group(&path).is_none() {
            self.groups.push(Group {
                path: path.clone(),
                ..Group::default()
            });
        }
        self.sync_groups();
        Some(path)
    }

    /// The color of a group: its own, or that of the nearest group it is
    /// inside that has one.
    pub fn group_color(&self, path: &str) -> Option<&str> {
        let mut path = Some(path);
        while let Some(current) = path.filter(|path| !path.is_empty()) {
            if let Some(color) = self.group(current).and_then(|g| g.color.as_deref()) {
                return Some(color);
            }
            path = group_parent(current);
        }
        None
    }

    /// Rename a group (its last level). Everything inside moves with it.
    /// Fails if the name is empty or a group with the new path exists.
    pub fn rename_group(&mut self, path: &str, name: &str) -> Result<String, &'static str> {
        let name = clean_group(&name.replace([GROUP_SEPARATOR, '>'], " "));
        if name.is_empty() {
            return Err("Enter a name.");
        }
        let new_path = match group_parent(path) {
            Some(parent) => format!("{parent}{GROUP_SEPARATOR}{name}"),
            None => name,
        };
        if new_path == path {
            return Ok(new_path);
        }
        if self.group(&new_path).is_some() {
            return Err("A group with this name is already here.");
        }
        let moved = |old: &str| format!("{new_path}{}", &old[path.len()..]);
        for group in &mut self.groups {
            if in_group(&group.path, path) {
                group.path = moved(&group.path);
            }
        }
        for connection in &mut self.connections {
            if in_group(&connection.group, path) {
                connection.group = moved(&connection.group);
            }
        }
        self.sync_groups();
        Ok(new_path)
    }

    /// Delete a group and the groups inside it. Their connections are not
    /// deleted: they move to the group this one was inside.
    pub fn delete_group(&mut self, path: &str) {
        let parent = group_parent(path).unwrap_or("").to_string();
        for connection in &mut self.connections {
            if in_group(&connection.group, path) {
                connection.group = parent.clone();
            }
        }
        self.groups.retain(|group| !in_group(&group.path, path));
        self.sync_groups();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection(kind: DbKind) -> Connection {
        Connection {
            host: "db.example.com".into(),
            database: "shop".into(),
            ..Connection::new(kind)
        }
    }

    #[test]
    fn jdbc_urls() {
        assert_eq!(
            connection(DbKind::Postgres).jdbc_url(None),
            "jdbc:postgresql://db.example.com:5432/shop"
        );
        assert_eq!(
            connection(DbKind::MySql).jdbc_url(None),
            "jdbc:mysql://db.example.com:3306/shop"
        );
        assert_eq!(
            connection(DbKind::MariaDb).jdbc_url(None),
            "jdbc:mariadb://db.example.com:3306/shop"
        );
        assert_eq!(
            connection(DbKind::SqlServer).jdbc_url(None),
            "jdbc:sqlserver://db.example.com:1433;databaseName=shop;encrypt=true;trustServerCertificate=true"
        );
        assert_eq!(
            connection(DbKind::Oracle).jdbc_url(None),
            "jdbc:oracle:thin:@//db.example.com:1521/shop"
        );
        let file = Connection {
            database: "C:\\data\\shop.db".into(),
            ..Connection::new(DbKind::Sqlite)
        };
        assert_eq!(file.jdbc_url(None), "jdbc:sqlite:C:\\data\\shop.db");
    }

    #[test]
    fn a_tunnel_address_points_at_this_computer() {
        assert_eq!(
            connection(DbKind::Postgres).jdbc_url_through_tunnel(None),
            "jdbc:postgresql://127.0.0.1:${TUNNEL_PORT}/shop"
        );
    }

    #[test]
    fn group_paths_are_tidied() {
        assert_eq!(
            clean_group(" Company1 / Production /"),
            "Company1/Production"
        );
        assert_eq!(clean_group("Company1 > Production"), "Company1/Production");
        assert_eq!(clean_group("  "), "");
        assert_eq!(group_name("Company1/Production"), "Production");
        assert_eq!(group_parent("Company1/Production"), Some("Company1"));
        assert_eq!(group_parent("Company1"), None);
        assert_eq!(group_label("Company1/Production"), "Company1 › Production");
        assert!(in_group("Company1/Production", "Company1"));
        assert!(!in_group("Company10", "Company1"));
    }

    fn grouped() -> Data {
        let mut data = Data::default();
        for (name, group) in [
            ("prod1", "Company1/Production"),
            ("dev1", "Company1/Development"),
            ("loose", ""),
        ] {
            data.upsert(Connection {
                name: name.into(),
                group: group.into(),
                ..Connection::new(DbKind::Postgres)
            });
        }
        data
    }

    fn names(data: &Data) -> Vec<&str> {
        data.connections.iter().map(|c| c.name.as_str()).collect()
    }

    fn id_of(data: &Data, name: &str) -> Uuid {
        data.connections.iter().find(|c| c.name == name).unwrap().id
    }

    #[test]
    fn a_card_is_moved_in_front_of_another_or_to_the_end_of_a_group() {
        let mut data = Data::default();
        for (name, group) in [
            ("a", ""),
            ("b", ""),
            ("c", ""),
            ("p", "Prod"),
            ("q", "Prod"),
        ] {
            data.upsert(Connection {
                name: name.into(),
                group: group.into(),
                ..Connection::new(DbKind::Postgres)
            });
        }
        let id = |data: &Data, name: &str| id_of(data, name);
        // In front of another card of the same group.
        assert!(data.move_connection(id(&data, "c"), "", Some(id(&data, "a"))));
        assert_eq!(names(&data), ["c", "a", "b", "p", "q"]);
        // To the end of its group.
        assert!(data.move_connection(id(&data, "c"), "", None));
        assert_eq!(names(&data), ["a", "b", "c", "p", "q"]);
        // Into another group, in front of a card there.
        assert!(data.move_connection(id(&data, "a"), "Prod", Some(id(&data, "q"))));
        assert_eq!(names(&data), ["b", "c", "p", "a", "q"]);
        assert_eq!(group_of(&data, "a"), "Prod");
        // Into a group that has no cards yet.
        assert!(data.move_connection(id(&data, "b"), "Prod / Asia", None));
        assert_eq!(group_of(&data, "b"), "Prod/Asia");
        assert!(data.groups.iter().any(|group| group.path == "Prod/Asia"));
        // Where it already is: nothing changes.
        assert!(!data.move_connection(id(&data, "a"), "Prod", Some(id(&data, "q"))));
        assert!(!data.move_connection(id(&data, "q"), "Prod", None));
        assert!(!data.move_connection(id(&data, "q"), "Prod", Some(id(&data, "q"))));
        assert!(!data.move_connection(Uuid::new_v4(), "Prod", None));
    }

    #[test]
    fn cards_of_an_older_file_start_in_the_order_they_were_shown() {
        let mut data: Data = serde_json::from_str(
            r#"{"connections":[{"name":"zeta"},{"name":"Alpha"},{"name":"beta"}]}"#,
        )
        .unwrap();
        data.migrate();
        assert_eq!(names(&data), ["Alpha", "beta", "zeta"]);
        // From then on the order is the user's.
        let zeta = id_of(&data, "zeta");
        let alpha = id_of(&data, "Alpha");
        data.move_connection(zeta, "", Some(alpha));
        let mut again: Data = serde_json::from_str(&serde_json::to_string(&data).unwrap()).unwrap();
        again.migrate();
        assert_eq!(names(&again), ["zeta", "Alpha", "beta"]);
    }

    fn group_of<'a>(data: &'a Data, name: &str) -> &'a str {
        &data
            .connections
            .iter()
            .find(|c| c.name == name)
            .unwrap()
            .group
    }

    #[test]
    fn groups_follow_the_connections_in_tree_order() {
        let data = grouped();
        let paths: Vec<&str> = data.groups.iter().map(|g| g.path.as_str()).collect();
        assert_eq!(
            paths,
            ["Company1", "Company1/Development", "Company1/Production"]
        );
    }

    #[test]
    fn a_group_takes_the_color_of_the_group_it_is_inside() {
        let mut data = grouped();
        assert_eq!(data.group_color("Company1/Production"), None);
        data.group_mut("Company1").unwrap().color = Some("#2F7FE0".into());
        assert_eq!(data.group_color("Company1/Production"), Some("#2F7FE0"));
        data.group_mut("Company1/Production").unwrap().color = Some("#D9363E".into());
        assert_eq!(data.group_color("Company1/Production"), Some("#D9363E"));
        assert_eq!(data.group_color("Company1/Development"), Some("#2F7FE0"));
        assert_eq!(data.group_color(""), None);
    }

    #[test]
    fn renaming_a_group_moves_everything_inside() {
        let mut data = grouped();
        assert_eq!(
            data.rename_group("Company1", "Acme"),
            Ok("Acme".to_string())
        );
        assert_eq!(group_of(&data, "prod1"), "Acme/Production");
        assert!(data.group("Company1").is_none());
        assert!(data.group("Acme/Development").is_some());
        assert!(data.rename_group("Acme/Production", "Development").is_err());
        assert!(data.rename_group("Acme", "  ").is_err());
    }

    #[test]
    fn deleting_a_group_keeps_its_connections() {
        let mut data = grouped();
        data.delete_group("Company1/Production");
        assert_eq!(group_of(&data, "prod1"), "Company1");
        data.delete_group("Company1");
        assert_eq!(group_of(&data, "prod1"), "");
        assert_eq!(group_of(&data, "dev1"), "");
        assert!(data.groups.is_empty());
        assert_eq!(data.connections.len(), 3);
    }

    #[test]
    fn an_ssh_tunnel_needs_its_details() {
        let mut c = connection(DbKind::Postgres);
        c.ssh.enabled = true;
        assert_eq!(c.problem(None), Some("Enter the SSH host."));
        c.ssh.host = "bastion".into();
        c.ssh.username = "me".into();
        assert_eq!(c.problem(None), None);
        c.ssh.auth = SshAuth::Key;
        assert!(c.problem(None).is_some());
    }

    #[test]
    fn a_jump_server_needs_its_details() {
        let mut c = connection(DbKind::Postgres);
        c.ssh.enabled = true;
        c.ssh.host = "db-gateway".into();
        c.ssh.username = "me".into();
        c.ssh.jumps.push(Jump::default());
        assert_eq!(c.problem(None), Some("Enter the host of the jump server."));
        c.ssh.jumps[0].host = "bastion".into();
        assert_eq!(
            c.problem(None),
            Some("Enter the user name of the jump server.")
        );
        c.ssh.jumps[0].username = "me".into();
        assert_eq!(c.problem(None), None);
    }

    #[test]
    fn a_connection_saved_before_jump_servers_still_opens() {
        let ssh: Ssh = serde_json::from_str(
            r#"{"enabled":true,"host":"bastion","port":22,"username":"me","auth":"password","key_file":""}"#,
        )
        .unwrap();
        assert!(ssh.jumps.is_empty());
        assert_eq!(ssh.host, "bastion");
    }

    #[test]
    fn extra_settings_are_added_to_the_url() {
        let mut pg = connection(DbKind::Postgres);
        pg.params = "?sslmode=require".into();
        assert_eq!(
            pg.jdbc_url(None),
            "jdbc:postgresql://db.example.com:5432/shop?sslmode=require"
        );
    }

    #[test]
    fn changing_kind_moves_defaults_but_keeps_what_was_typed() {
        let mut c = Connection::new(DbKind::Postgres);
        c.set_kind(DbKind::MySql);
        assert_eq!((c.port, c.username.as_str()), (3306, "root"));

        c.port = 3307;
        c.username = "app".into();
        c.set_kind(DbKind::SqlServer);
        assert_eq!((c.port, c.username.as_str()), (3307, "app"));
        assert_eq!(c.params, DbKind::SqlServer.default_params());
    }

    #[test]
    fn the_password_is_never_saved_with_the_connection() {
        let json = serde_json::to_string(&Connection::default()).unwrap();
        assert!(!json.contains("password\":\""), "{json}");
    }

    #[test]
    fn missing_fields_are_reported() {
        let mut c = Connection::new(DbKind::Postgres);
        assert_eq!(c.problem(None), None);
        c.host.clear();
        assert!(c.problem(None).is_some());
        assert!(Connection::new(DbKind::Sqlite).problem(None).is_some());
    }

    #[test]
    fn templates_are_filled_in() {
        let mysql = "jdbc:mysql://{host}[:{port}]/[{database}]";
        assert_eq!(
            expand_template(mysql, "db", "3306", "shop", ""),
            "jdbc:mysql://db:3306/shop"
        );
        assert_eq!(expand_template(mysql, "db", "", "", ""), "jdbc:mysql://db/");
        // A part inside a part, DBeaver's names, and what is not known.
        assert_eq!(
            expand_template(
                "jdbc:x:{server}[:{port}[/{file}]]{other}",
                "h",
                "1",
                "",
                "u"
            ),
            "jdbc:x:h:1{other}"
        );
        assert_eq!(
            expand_template("jdbc:x:[{user}@]{host}", "h", "", "", "me"),
            "jdbc:x:me@h"
        );
        assert_eq!(
            expand_template("jdbc:x:{host", "h", "", "", ""),
            "jdbc:x:{host"
        );
        assert_eq!(
            expand_template("jdbc:x:[{host}", "h", "", "", ""),
            "jdbc:x:h"
        );
        assert!(required(mysql, "host"));
        assert!(!required(mysql, "port"));
        assert!(!required("jdbc:h2:{file}", "host"));
    }

    fn mysql5() -> Driver {
        Driver {
            name: "MySQL 5".into(),
            class_name: "com.mysql.jdbc.Driver".into(),
            url_template: "jdbc:mysql://{host}[:{port}]/[{database}]".into(),
            libraries: vec!["/drivers/mysql-connector-java-5.1.49.jar".into()],
            ..Driver::new(DbKind::MySql)
        }
    }

    #[test]
    fn a_connection_uses_the_template_of_its_own_driver() {
        let driver = mysql5();
        let mut c = connection(DbKind::MySql);
        c.driver = Some(driver.id);
        c.database.clear();
        c.params = "useSSL=false".into();
        assert_eq!(
            c.jdbc_url(Some(&driver)),
            "jdbc:mysql://db.example.com:3306/?useSSL=false"
        );
        assert_eq!(
            c.jdbc_url_through_tunnel(Some(&driver)),
            "jdbc:mysql://127.0.0.1:${TUNNEL_PORT}/?useSSL=false"
        );
        assert_eq!(c.problem(Some(&driver)), None);
        // The driver was deleted.
        assert!(c.problem(None).is_some());
    }

    #[test]
    fn drivers_for_the_same_database_live_side_by_side() {
        let mut data = Data::default();
        let old = mysql5();
        let new = Driver {
            name: "MySQL 8".into(),
            libraries: vec!["/drivers/mysql-connector-j-8.4.0.jar".into()],
            ..Driver::new(DbKind::MySql)
        };
        data.upsert_driver(new.clone());
        data.upsert_driver(old.clone());
        let names: Vec<&str> = data.drivers.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["MySQL 5", "MySQL 8"]);

        let mut legacy = Connection::new(DbKind::MySql);
        legacy.set_driver(None, DbKind::MySql, Some(&old));
        data.upsert(legacy.clone());
        data.upsert(Connection::new(DbKind::MySql));
        assert_eq!(
            data.driver_of(&legacy).map(|d| d.name.as_str()),
            Some("MySQL 5")
        );
        assert_eq!(data.driver_of(&data.connections[1]), None);
        assert_eq!((data.driver_uses(old.id), data.driver_uses(new.id)), (1, 0));

        // A driver in use stays; a copy is a driver of its own.
        assert!(data.remove_driver(old.id).is_err());
        assert_eq!(data.remove_driver(new.id), Ok(()));
        let copy = data.duplicate_driver(old.id).unwrap();
        assert_ne!(copy.id, old.id);
        assert_eq!(copy.name, "MySQL 5 (copy)");
    }

    #[test]
    fn choosing_a_driver_moves_defaults_but_keeps_what_was_typed() {
        let h2 = Driver {
            name: "H2".into(),
            url_template: "jdbc:h2:tcp://{host}[:{port}]/{database}".into(),
            default_port: 9092,
            default_user: "sa".into(),
            default_database: "test".into(),
            libraries: vec!["h2.jar".into()],
            ..Driver::new(DbKind::Other)
        };
        let mut c = Connection::new(DbKind::Postgres);
        c.set_driver(None, DbKind::Other, Some(&h2));
        assert_eq!(
            (c.port, c.username.as_str(), c.database.as_str()),
            (9092, "sa", "test")
        );
        assert_eq!(c.jdbc_url(Some(&h2)), "jdbc:h2:tcp://localhost:9092/test");
        c.username = "app".into();
        c.set_driver(Some(&h2), DbKind::SqlServer, None);
        assert_eq!(
            (c.port, c.username.as_str(), c.database.as_str()),
            (1433, "app", "")
        );
        assert_eq!(c.params, DbKind::SqlServer.default_params());
        assert_eq!(c.driver, None);
    }

    #[test]
    fn a_driver_says_what_is_missing() {
        let mut driver = Driver::new(DbKind::Other);
        assert_eq!(driver.problem(), Some("Enter a name for the driver."));
        driver.name = "H2".into();
        assert_eq!(driver.problem(), Some("Add the driver's .jar file."));
        driver.libraries.push(" h2.jar ".into());
        assert_eq!(driver.problem(), Some("Enter the URL template."));
        driver.url_template = "jdbc:h2:{file}".into();
        assert_eq!(driver.problem(), None);
        assert_eq!(driver.example_url(), "jdbc:h2:shop");
    }

    #[test]
    fn the_own_drivers_of_0_1_1_become_drivers_in_the_list() {
        let mut data: Data = serde_json::from_str(
            r#"{"connections":[{"name":"a","kind":"mysql"},{"name":"b","kind":"postgres"}],
                "settings":{"drivers":{"mysql":{"jar":"C:\\drivers\\my5.jar","class_name":"com.mysql.jdbc.Driver"}}}}"#,
        )
        .unwrap();
        data.migrate();
        assert_eq!(data.drivers.len(), 1);
        let driver = &data.drivers[0];
        assert_eq!(driver.name, "MySQL (own driver)");
        assert_eq!(driver.libraries, [r"C:\drivers\my5.jar"]);
        assert_eq!(data.connections[0].driver, Some(driver.id));
        assert_eq!(data.connections[1].driver, None);
        assert!(!serde_json::to_string(&data).unwrap().contains(r#""jar""#));
    }

    #[test]
    fn old_files_without_newer_fields_still_load() {
        let data: Data = serde_json::from_str(r#"{"connections":[{"name":"x"}]}"#).unwrap();
        assert_eq!(data.connections[0].name, "x");
        assert_eq!(data.settings.row_limit, DEFAULT_ROW_LIMIT);
    }
}
