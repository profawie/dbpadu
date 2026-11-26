//! What the app asks the engine and what the engine answers.

use serde::Deserialize;
use serde_json::{Value, json};

/// The engine introducing itself.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Hello {
    pub version: String,
    pub java: String,
    /// The most rows the engine returns, whatever it is asked.
    pub max_rows: u32,
    /// The JDBC drivers inside the engine.
    pub drivers: Vec<Driver>,
}

/// A JDBC driver inside the engine.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Driver {
    /// The kind of database it is for, as the engine names it.
    pub kind: String,
    pub name: String,
    /// Where it comes from: `group:artifact` on Maven Central.
    pub package: String,
    pub class_name: String,
    pub version: String,
}

/// What the engine found in the jar files of a driver of the user's own.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OwnDriverInfo {
    /// The driver class that is used. Empty when none could be loaded.
    pub class_name: String,
    pub version: String,
    /// Every driver class in the files.
    pub classes: Vec<String>,
    /// Why the driver class could not be loaded, if it could not.
    pub error: Option<String>,
}

/// A function or a procedure.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Routine {
    pub name: String,
    /// `function` or `procedure`.
    pub kind: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Routines {
    pub items: Vec<Routine>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Sequences {
    pub items: Vec<String>,
}

/// Whether changes are committed one by one, and what is waiting.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TxState {
    pub auto_commit: bool,
    /// Statements run since the last commit or rollback.
    pub pending: u32,
}

impl Default for TxState {
    fn default() -> Self {
        Self {
            auto_commit: true,
            pending: 0,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Connected {
    pub product: String,
    pub version: String,
    pub tx: TxState,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TxAnswer {
    pub tx: TxState,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ResultColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// What running one statement gave.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct QueryResult {
    /// The SQL as it was sent to the database.
    pub executed_sql: String,
    /// What the engine added to the SQL, such as `LIMIT 500`.
    pub added: Option<String>,
    /// The most rows this run could return.
    pub limit: u32,
    /// `rows` or `update`.
    pub kind: String,
    pub columns: Vec<ResultColumn>,
    /// `None` is a NULL.
    pub rows: Vec<Vec<Option<String>>>,
    /// As many rows as the limit came back; the database may have more.
    pub limit_reached: bool,
    pub update_count: i64,
    pub elapsed_ms: u64,
    pub tx: TxState,
}

impl QueryResult {
    pub fn has_rows(&self) -> bool {
        self.kind == "rows"
    }
}

/// The first level under a connection in the tree.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Schemas {
    /// `schema`, `catalog` (MySQL, MariaDB) or `none` (SQLite).
    pub mode: String,
    pub items: Vec<String>,
    /// The one the connection is in.
    pub current: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Table {
    pub name: String,
    pub view: bool,
    /// Quoted and ready to be put after FROM.
    pub sql_name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Tables {
    pub items: Vec<Table>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TableColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub nullable: bool,
    /// Part of the primary key.
    pub key: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TableColumns {
    pub items: Vec<TableColumn>,
}

/// Read an answer as `T`.
pub fn read<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value)
        .map_err(|err| format!("The engine's answer could not be read: {err}"))
}

/// An SSH server to go through.
pub struct SshTarget<'a> {
    pub host: &'a str,
    pub port: u16,
    pub user: &'a str,
    /// Private key file; empty to sign in with a password.
    pub key_file: &'a str,
    /// The password, or the passphrase of the key.
    pub secret: &'a str,
    /// File that remembers the keys of servers seen before.
    pub known_hosts: &'a str,
    /// The database, as seen from the SSH server.
    pub remote_host: &'a str,
    pub remote_port: u16,
}

/// How to reach a database.
pub struct Target<'a> {
    pub kind: &'a str,
    pub url: &'a str,
    pub user: &'a str,
    pub password: &'a str,
    pub ssh: Option<SshTarget<'a>>,
    /// The jar files (or folders of jar files) of the user's own driver;
    /// empty for the driver inside the engine.
    pub driver_jars: &'a [String],
    pub driver_class: &'a str,
    /// Settings handed to the user's own driver, as name and value.
    pub properties: &'a [(String, String)],
}

impl Target<'_> {
    fn json(&self) -> Value {
        let mut value = json!({
            "kind": self.kind,
            "url": self.url,
            "user": self.user,
            "password": self.password,
        });
        if let Some(ssh) = &self.ssh {
            value["ssh"] = json!({
                "host": ssh.host,
                "port": ssh.port,
                "user": ssh.user,
                "keyFile": ssh.key_file,
                "secret": ssh.secret,
                "knownHosts": ssh.known_hosts,
                "remoteHost": ssh.remote_host,
                "remotePort": ssh.remote_port,
            });
        }
        if !self.driver_jars.is_empty() {
            value["driverJars"] = json!(self.driver_jars);
            value["driverClass"] = json!(self.driver_class);
        }
        if !self.properties.is_empty() {
            let properties: serde_json::Map<String, Value> = self
                .properties
                .iter()
                .map(|(name, value)| (name.clone(), json!(value)))
                .collect();
            value["properties"] = Value::Object(properties);
        }
        value
    }
}

/// A request: the operation and its parameters.
pub type Request = (&'static str, Value);

pub fn hello() -> Request {
    ("hello", json!({}))
}

pub fn test(target: &Target) -> Request {
    ("test", target.json())
}

pub fn connect(session: &str, target: &Target) -> Request {
    let mut value = target.json();
    value["session"] = json!(session);
    ("connect", value)
}

/// Look into the jar files of a driver of the user's own.
pub fn driver_info(jars: &[String], class_name: &str) -> Request {
    (
        "driverInfo",
        json!({"driverJars": jars, "driverClass": class_name}),
    )
}

pub fn routines(session: &str, mode: &str, schema: &str) -> Request {
    (
        "routines",
        json!({"session": session, "mode": mode, "schema": schema}),
    )
}

pub fn sequences(session: &str, schema: &str) -> Request {
    ("sequences", json!({"session": session, "schema": schema}))
}

pub fn disconnect(session: &str) -> Request {
    ("disconnect", json!({"session": session}))
}

pub fn execute(session: &str, sql: &str, limit: u32) -> Request {
    (
        "execute",
        json!({"session": session, "sql": sql, "limit": limit}),
    )
}

pub fn cancel(session: &str) -> Request {
    ("cancel", json!({"session": session}))
}

pub fn auto_commit(session: &str, value: bool) -> Request {
    ("autocommit", json!({"session": session, "value": value}))
}

pub fn commit(session: &str) -> Request {
    ("commit", json!({"session": session}))
}

pub fn rollback(session: &str) -> Request {
    ("rollback", json!({"session": session}))
}

pub fn schemas(session: &str) -> Request {
    ("schemas", json!({"session": session}))
}

pub fn tables(session: &str, mode: &str, schema: &str) -> Request {
    (
        "tables",
        json!({"session": session, "mode": mode, "schema": schema}),
    )
}

pub fn columns(session: &str, mode: &str, schema: &str, table: &str) -> Request {
    (
        "columns",
        json!({"session": session, "mode": mode, "schema": schema, "table": table}),
    )
}
