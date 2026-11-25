//! Saved connections and settings.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Rows a SELECT may return when nothing else is chosen.
pub const DEFAULT_ROW_LIMIT: u32 = 500;
/// The choices offered for the row limit. The engine refuses more than the
/// last one.
pub const ROW_LIMITS: [u32; 5] = [100, 500, 1_000, 5_000, 10_000];

/// The kinds of database dbpadu can connect to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DbKind {
    #[default]
    Postgres,
    MySql,
    MariaDb,
    SqlServer,
    Oracle,
    Sqlite,
}

impl DbKind {
    pub const ALL: [DbKind; 6] = [
        DbKind::Postgres,
        DbKind::MySql,
        DbKind::MariaDb,
        DbKind::SqlServer,
        DbKind::Oracle,
        DbKind::Sqlite,
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
        }
    }

    /// `None` for a database that is a file.
    pub fn default_port(self) -> Option<u16> {
        match self {
            DbKind::Postgres => Some(5432),
            DbKind::MySql | DbKind::MariaDb => Some(3306),
            DbKind::SqlServer => Some(1433),
            DbKind::Oracle => Some(1521),
            DbKind::Sqlite => None,
        }
    }

    /// A database that is a file on this computer, not a server.
    pub fn is_file(self) -> bool {
        self.default_port().is_none()
    }

    pub fn default_user(self) -> &'static str {
        match self {
            DbKind::Postgres => "postgres",
            DbKind::MySql | DbKind::MariaDb => "root",
            DbKind::SqlServer => "sa",
            DbKind::Oracle => "system",
            DbKind::Sqlite => "",
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
    pub host: String,
    pub port: u16,
    /// Database name, Oracle service name, or the path of a SQLite file.
    pub database: String,
    pub username: String,
    /// Keep the password in the operating system's credential store.
    pub save_password: bool,
    /// Extra driver settings, written as they appear in the JDBC URL.
    pub params: String,
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
            host: "localhost".to_string(),
            port: kind.default_port().unwrap_or(0),
            database: String::new(),
            username: kind.default_user().to_string(),
            save_password: false,
            params: kind.default_params().to_string(),
        }
    }

    /// Switch to another kind of database. Fields that still hold the old
    /// kind's defaults move to the new kind's defaults; anything the user
    /// typed stays.
    pub fn set_kind(&mut self, kind: DbKind) {
        let old = self.kind;
        if self.port == old.default_port().unwrap_or(0) {
            self.port = kind.default_port().unwrap_or(0);
        }
        if self.username == old.default_user() {
            self.username = kind.default_user().to_string();
        }
        if self.params == old.default_params() {
            self.params = kind.default_params().to_string();
        }
        if old.is_file() != kind.is_file() {
            self.database.clear();
        }
        self.kind = kind;
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

    /// The address the engine's JDBC driver connects to.
    pub fn jdbc_url(&self) -> String {
        let host = self.host.trim();
        let database = self.database.trim();
        let params = self.params.trim().trim_start_matches(['?', ';', '&']);
        let query = if params.is_empty() {
            String::new()
        } else {
            format!("?{params}")
        };
        match self.kind {
            DbKind::Postgres => format!("jdbc:postgresql://{host}:{}/{database}{query}", self.port),
            DbKind::MySql => format!("jdbc:mysql://{host}:{}/{database}{query}", self.port),
            DbKind::MariaDb => format!("jdbc:mariadb://{host}:{}/{database}{query}", self.port),
            DbKind::SqlServer => {
                let mut url = format!("jdbc:sqlserver://{host}:{}", self.port);
                if !database.is_empty() {
                    url.push_str(&format!(";databaseName={database}"));
                }
                if !params.is_empty() {
                    url.push_str(&format!(";{params}"));
                }
                url
            }
            DbKind::Oracle => format!("jdbc:oracle:thin:@//{host}:{}/{database}", self.port),
            DbKind::Sqlite => format!("jdbc:sqlite:{database}{query}"),
        }
    }

    /// What is still missing before it can be saved, in words for the user.
    pub fn problem(&self) -> Option<&'static str> {
        if self.kind.is_file() {
            if self.database.trim().is_empty() {
                return Some("Choose the database file.");
            }
            return None;
        }
        if self.host.trim().is_empty() {
            return Some("Enter the host.");
        }
        if self.port == 0 {
            return Some("Enter the port.");
        }
        if self.kind == DbKind::Oracle && self.database.trim().is_empty() {
            return Some("Enter the service name.");
        }
        None
    }
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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            row_limit: DEFAULT_ROW_LIMIT,
            java_path: String::new(),
        }
    }
}

/// Everything that is saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Data {
    pub connections: Vec<Connection>,
    pub settings: Settings,
}

impl Data {
    pub fn connection(&self, id: Uuid) -> Option<&Connection> {
        self.connections
            .iter()
            .find(|connection| connection.id == id)
    }

    /// Add the connection, or replace the saved one with the same id.
    pub fn upsert(&mut self, connection: Connection) {
        match self
            .connections
            .iter_mut()
            .find(|saved| saved.id == connection.id)
        {
            Some(saved) => *saved = connection,
            None => self.connections.push(connection),
        }
    }

    pub fn remove(&mut self, id: Uuid) {
        self.connections.retain(|connection| connection.id != id);
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
            connection(DbKind::Postgres).jdbc_url(),
            "jdbc:postgresql://db.example.com:5432/shop"
        );
        assert_eq!(
            connection(DbKind::MySql).jdbc_url(),
            "jdbc:mysql://db.example.com:3306/shop"
        );
        assert_eq!(
            connection(DbKind::MariaDb).jdbc_url(),
            "jdbc:mariadb://db.example.com:3306/shop"
        );
        assert_eq!(
            connection(DbKind::SqlServer).jdbc_url(),
            "jdbc:sqlserver://db.example.com:1433;databaseName=shop;encrypt=true;trustServerCertificate=true"
        );
        assert_eq!(
            connection(DbKind::Oracle).jdbc_url(),
            "jdbc:oracle:thin:@//db.example.com:1521/shop"
        );
        let file = Connection {
            database: "C:\\data\\shop.db".into(),
            ..Connection::new(DbKind::Sqlite)
        };
        assert_eq!(file.jdbc_url(), "jdbc:sqlite:C:\\data\\shop.db");
    }

    #[test]
    fn extra_settings_are_added_to_the_url() {
        let mut pg = connection(DbKind::Postgres);
        pg.params = "?sslmode=require".into();
        assert_eq!(
            pg.jdbc_url(),
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
        assert_eq!(c.problem(), None);
        c.host.clear();
        assert!(c.problem().is_some());
        assert!(Connection::new(DbKind::Sqlite).problem().is_some());
    }

    #[test]
    fn old_files_without_newer_fields_still_load() {
        let data: Data = serde_json::from_str(r#"{"connections":[{"name":"x"}]}"#).unwrap();
        assert_eq!(data.connections[0].name, "x");
        assert_eq!(data.settings.row_limit, DEFAULT_ROW_LIMIT);
    }
}
