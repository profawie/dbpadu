//! Runs the real engine against a SQLite file. Needs Java and the engine
//! jar (`mvn -f engine/pom.xml package`); skipped when either is missing.

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use dbpadu_engine::{
    Engine, Event,
    protocol::{self, Connected, QueryResult, Target, TxAnswer},
};
use serde_json::Value;

fn ask(engine: &mut Engine, (op, params): (&str, Value)) -> Result<Value, String> {
    let sent = engine.send(op, params).expect("request is sent");
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        for event in engine.poll() {
            match event {
                Event::Answer { id, result } if id == sent => return result,
                Event::Answer { .. } | Event::Log(_) => {}
                Event::Stopped(why) => panic!("engine stopped: {why}"),
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no answer to {op}");
}

#[test]
fn select_is_always_limited() {
    let jar =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../engine/target/dbpadu-engine.jar");
    if !jar.is_file() {
        eprintln!("skipped: {} is not built", jar.display());
        return;
    }
    let java = dbpadu_engine::locate::java("");
    let Ok(mut engine) = Engine::start(&java, &jar, || {}) else {
        eprintln!("skipped: Java is not installed");
        return;
    };
    let file = std::env::temp_dir().join(format!("dbpadu-test-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&file);
    let url = format!("jdbc:sqlite:{}", file.display());
    let target = Target {
        kind: "sqlite",
        url: &url,
        user: "",
        password: "",
        ssh: None,
        driver_jars: &[],
        driver_class: "",
        properties: &[],
    };

    let hello: protocol::Hello =
        protocol::read(ask(&mut engine, protocol::hello()).unwrap()).unwrap();
    assert_eq!(hello.version, env!("CARGO_PKG_VERSION"));
    let sqlite = hello
        .drivers
        .iter()
        .find(|driver| driver.kind == "sqlite")
        .expect("the SQLite driver is listed");
    assert_eq!(sqlite.class_name, "org.sqlite.JDBC");
    assert!(sqlite.version.starts_with(|c: char| c.is_ascii_digit()));

    let connected: Connected =
        protocol::read(ask(&mut engine, protocol::connect("s", &target)).unwrap()).unwrap();
    assert_eq!(connected.product, "SQLite");
    assert!(connected.tx.auto_commit);

    ask(
        &mut engine,
        protocol::execute("s", "create table t (n integer)", 500),
    )
    .unwrap();
    let values: Vec<String> = (0..50).map(|n| format!("({n})")).collect();
    let insert = format!("insert into t values {}", values.join(","));
    let inserted: QueryResult =
        protocol::read(ask(&mut engine, protocol::execute("s", &insert, 500)).unwrap()).unwrap();
    assert_eq!((inserted.has_rows(), inserted.update_count), (false, 50));

    // No limit in the SQL: the engine adds one and says so.
    let rows: QueryResult =
        protocol::read(ask(&mut engine, protocol::execute("s", "select * from t;", 10)).unwrap())
            .unwrap();
    assert_eq!(rows.rows.len(), 10);
    assert_eq!(rows.added.as_deref(), Some("LIMIT 10"));
    assert_eq!(rows.executed_sql, "select * from t\nLIMIT 10");
    assert!(rows.limit_reached);

    // A larger limit in the SQL does not get past the engine either.
    let rows: QueryResult = protocol::read(
        ask(
            &mut engine,
            protocol::execute("s", "select * from t limit 40", 10),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(rows.rows.len(), 10);
    assert_eq!(rows.added, None);

    // Changes wait for a commit once auto-commit is off.
    let tx: TxAnswer =
        protocol::read(ask(&mut engine, protocol::auto_commit("s", false)).unwrap()).unwrap();
    assert!(!tx.tx.auto_commit);
    let deleted: QueryResult =
        protocol::read(ask(&mut engine, protocol::execute("s", "delete from t", 10)).unwrap())
            .unwrap();
    assert_eq!(deleted.tx.pending, 1);
    let tx: TxAnswer = protocol::read(ask(&mut engine, protocol::rollback("s")).unwrap()).unwrap();
    assert_eq!(tx.tx.pending, 0);
    let count: QueryResult = protocol::read(
        ask(
            &mut engine,
            protocol::execute("s", "select count(*) from t", 10),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(count.rows[0][0].as_deref(), Some("50"));

    let error = ask(&mut engine, protocol::execute("s", "selec", 10)).unwrap_err();
    assert!(error.contains("syntax"), "{error}");

    drop(engine);
    let _ = std::fs::remove_file(&file);
}

/// A `.jar` file Maven downloaded for the engine's build, to stand in for a
/// driver the user added.
fn maven_jar(folder: &str, prefix: &str) -> Option<String> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let versions = PathBuf::from(home).join(".m2/repository").join(folder);
    for version in std::fs::read_dir(versions).ok()?.flatten() {
        for file in std::fs::read_dir(version.path()).ok()?.flatten() {
            let name = file.file_name().to_string_lossy().to_string();
            if name.starts_with(prefix) && name.ends_with(".jar") {
                return Some(file.path().display().to_string());
            }
        }
    }
    None
}

/// A driver of the user's own: found in its `.jar` file, used to connect,
/// and handed its connection properties.
#[test]
fn a_driver_from_the_users_own_jar_connects() {
    let jar =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../engine/target/dbpadu-engine.jar");
    let Some(driver_jar) = maven_jar("org/xerial/sqlite-jdbc", "sqlite-jdbc-") else {
        eprintln!("skipped: no SQLite driver jar in ~/.m2");
        return;
    };
    if !jar.is_file() {
        eprintln!("skipped: {} is not built", jar.display());
        return;
    }
    let java = dbpadu_engine::locate::java("");
    let Ok(mut engine) = Engine::start(&java, &jar, || {}) else {
        eprintln!("skipped: Java is not installed");
        return;
    };
    let jars = [driver_jar];

    // No class named: the engine finds the one in the file.
    let info: protocol::OwnDriverInfo =
        protocol::read(ask(&mut engine, protocol::driver_info(&jars, "")).unwrap()).unwrap();
    assert_eq!(info.class_name, "org.sqlite.JDBC");
    assert!(info.classes.contains(&"org.sqlite.JDBC".to_string()));
    assert!(info.version.starts_with(|c: char| c.is_ascii_digit()));
    assert_eq!(info.error, None);

    // A class that is not there: the classes that are there are still listed.
    let info: protocol::OwnDriverInfo = protocol::read(
        ask(
            &mut engine,
            protocol::driver_info(&jars, "com.example.Nope"),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        info.error
            .is_some_and(|error| error.contains("com.example.Nope"))
    );
    assert!(info.classes.contains(&"org.sqlite.JDBC".to_string()));

    let missing = ["/nowhere/driver.jar".to_string()];
    let error = ask(&mut engine, protocol::driver_info(&missing, "")).unwrap_err();
    assert!(error.contains("not found"), "{error}");

    // `kind` is "other": nothing is added to the SQL, the driver alone
    // keeps the row limit. The property makes the connection read-only.
    let properties = [("open_mode".to_string(), "1".to_string())];
    let file = std::env::temp_dir().join(format!("dbpadu-own-{}.db", std::process::id()));
    std::fs::write(&file, b"").unwrap();
    let url = format!("jdbc:sqlite:{}", file.display());
    let target = Target {
        kind: "other",
        url: &url,
        user: "",
        password: "",
        ssh: None,
        driver_jars: &jars,
        driver_class: "org.sqlite.JDBC",
        properties: &properties,
    };
    let connected: Connected =
        protocol::read(ask(&mut engine, protocol::connect("own", &target)).unwrap()).unwrap();
    assert_eq!(connected.product, "SQLite");
    let rows: QueryResult = protocol::read(
        ask(
            &mut engine,
            protocol::execute(
                "own",
                "with recursive n(i) as (select 1 union all select i + 1 from n where i < 50) \
                 select i from n",
                10,
            ),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(rows.added, None);
    assert_eq!(rows.rows.len(), 10);
    let error = ask(
        &mut engine,
        protocol::execute("own", "create table t (n integer)", 10),
    )
    .unwrap_err();
    assert!(error.to_lowercase().contains("read"), "{error}");

    drop(engine);
    let _ = std::fs::remove_file(&file);
}

/// The app and the engine are released together, under one version.
#[test]
fn engine_has_the_same_version_as_the_app() {
    let pom = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../engine/pom.xml"),
    )
    .expect("engine/pom.xml is there");
    let version = format!("<version>{}</version>", env!("CARGO_PKG_VERSION"));
    assert!(
        pom.contains(&version),
        "engine/pom.xml should say {version}"
    );
}
