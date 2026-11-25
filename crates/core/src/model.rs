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
        self.jdbc_url_at(self.host.trim(), &self.port.to_string())
    }

    /// The address when the database is reached through an SSH tunnel: the
    /// tunnel's end on this computer. The engine fills in the port once the
    /// tunnel is open.
    pub fn jdbc_url_through_tunnel(&self) -> String {
        self.jdbc_url_at("127.0.0.1", TUNNEL_PORT)
    }

    fn jdbc_url_at(&self, host: &str, port: &str) -> String {
        let database = self.database.trim();
        let params = self.params.trim().trim_start_matches(['?', ';', '&']);
        let query = if params.is_empty() {
            String::new()
        } else {
            format!("?{params}")
        };
        match self.kind {
            DbKind::Postgres => format!("jdbc:postgresql://{host}:{port}/{database}{query}"),
            DbKind::MySql => format!("jdbc:mysql://{host}:{port}/{database}{query}"),
            DbKind::MariaDb => format!("jdbc:mariadb://{host}:{port}/{database}{query}"),
            DbKind::SqlServer => {
                let mut url = format!("jdbc:sqlserver://{host}:{port}");
                if !database.is_empty() {
                    url.push_str(&format!(";databaseName={database}"));
                }
                if !params.is_empty() {
                    url.push_str(&format!(";{params}"));
                }
                url
            }
            DbKind::Oracle => format!("jdbc:oracle:thin:@//{host}:{port}/{database}"),
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
    /// The user's own JDBC drivers, used instead of the ones in the engine.
    pub drivers: BTreeMap<DbKind, OwnDriver>,
    /// Whether the log panel at the bottom is open.
    pub log_open: bool,
}

/// A JDBC driver from a jar file of the user's choosing.
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
        }
    }
}

/// Everything that is saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Data {
    pub connections: Vec<Connection>,
    pub groups: Vec<Group>,
    pub settings: Settings,
}

impl Data {
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
    fn a_tunnel_address_points_at_this_computer() {
        assert_eq!(
            connection(DbKind::Postgres).jdbc_url_through_tunnel(),
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
        assert_eq!(c.problem(), Some("Enter the SSH host."));
        c.ssh.host = "bastion".into();
        c.ssh.username = "me".into();
        assert_eq!(c.problem(), None);
        c.ssh.auth = SshAuth::Key;
        assert!(c.problem().is_some());
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
