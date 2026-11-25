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

/// How to reach a database.
pub struct Target<'a> {
    pub kind: &'a str,
    pub url: &'a str,
    pub user: &'a str,
    pub password: &'a str,
}

/// A request: the operation and its parameters.
pub type Request = (&'static str, Value);

pub fn hello() -> Request {
    ("hello", json!({}))
}

pub fn test(target: &Target) -> Request {
    (
        "test",
        json!({"kind": target.kind, "url": target.url, "user": target.user, "password": target.password}),
    )
}

pub fn connect(session: &str, target: &Target) -> Request {
    (
        "connect",
        json!({"session": session, "kind": target.kind, "url": target.url, "user": target.user, "password": target.password}),
    )
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
