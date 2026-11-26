//! The link between the app and its query engine.
//!
//! The engine is a Java program (`engine/` in the repository) that talks to
//! databases through JDBC. The app starts it as a child process and sends
//! it one JSON object per line on standard input; answers come back the
//! same way on standard output. Nothing here waits: [`Engine::poll`]
//! returns the answers that have arrived so far.

pub mod locate;
pub mod protocol;

use std::{
    collections::VecDeque,
    ffi::OsStr,
    io::{self, BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

use serde_json::json;

pub use serde_json::Value;

// The shared libraries form a chain: this one takes Rust's standard library
// and serde from `dbpadu_core` instead of carrying a second copy.
pub use dbpadu_core::APP_NAME;

/// Memory the engine may use. Together with the row limit this keeps a
/// large table from taking the computer down.
const ENGINE_MEMORY: &str = "-Xmx512m";

/// Something the engine said.
#[derive(Debug)]
pub enum Event {
    /// The answer to the request with this id.
    Answer {
        id: u64,
        result: Result<Value, String>,
    },
    /// A line the engine or one of its drivers wrote to its error output.
    Log(String),
    /// The engine is no longer running. The text is what it last wrote to
    /// its error output, if anything.
    Stopped(String),
}

/// A running engine. Dropping it stops the engine.
pub struct Engine {
    child: Child,
    stdin: Option<ChildStdin>,
    events: Receiver<Event>,
    next_id: u64,
}

impl Engine {
    /// Start the engine: `java -jar <jar>`. `wake` is called from another
    /// thread whenever there is something new for [`Engine::poll`].
    pub fn start(
        java: &OsStr,
        jar: &Path,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> io::Result<Self> {
        let mut command = Command::new(java);
        command
            .arg(ENGINE_MEMORY)
            .arg("-Dfile.encoding=UTF-8")
            .arg("-jar")
            .arg(jar)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // No console window for the engine.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn()?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");

        // The last lines of the error output explain why an engine stopped.
        let errors = Arc::new(Mutex::new(VecDeque::<String>::new()));
        let kept = errors.clone();
        let (sender, events) = mpsc::channel();
        let log = sender.clone();
        let wake = Arc::new(wake);
        let wake_log = wake.clone();
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                // The JVM announces its options there; that is not an error.
                if line.starts_with("Picked up ") {
                    continue;
                }
                {
                    let mut kept = kept.lock().expect("no panic while held");
                    if kept.len() == 12 {
                        kept.pop_front();
                    }
                    kept.push_back(line.clone());
                }
                if log.send(Event::Log(line)).is_err() {
                    return;
                }
                wake_log();
            }
        });

        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let Some(event) = parse_answer(&line) else {
                    continue;
                };
                if sender.send(event).is_err() {
                    return;
                }
                wake();
            }
            // Give the error output a moment to arrive in full.
            thread::sleep(Duration::from_millis(100));
            let text = errors
                .lock()
                .expect("no panic while held")
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            let _ = sender.send(Event::Stopped(text));
            wake();
        });

        Ok(Self {
            child,
            stdin,
            events,
            next_id: 1,
        })
    }

    /// Send a request. Returns its id; the answer carries the same id.
    pub fn send(&mut self, op: &str, mut params: Value) -> io::Result<u64> {
        let id = self.next_id;
        self.next_id += 1;
        let request = params
            .as_object_mut()
            .expect("request parameters are a JSON object");
        request.insert("id".to_string(), json!(id));
        request.insert("op".to_string(), json!(op));
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| io::Error::other("the engine is not running"))?;
        let mut line = serde_json::to_string(&params).map_err(io::Error::other)?;
        line.push('\n');
        stdin.write_all(line.as_bytes())?;
        stdin.flush()?;
        Ok(id)
    }

    /// Everything the engine said since the last call.
    pub fn poll(&mut self) -> Vec<Event> {
        self.events.try_iter().collect()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // Closing its input asks the engine to roll back what is open,
        // close its connections and leave.
        self.stdin = None;
        let deadline = Instant::now() + Duration::from_millis(800);
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn parse_answer(line: &str) -> Option<Event> {
    let mut answer: Value = serde_json::from_str(line).ok()?;
    let id = answer.get("id")?.as_u64()?;
    let result = if answer.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(answer
            .get_mut("result")
            .map(Value::take)
            .unwrap_or(Value::Null))
    } else {
        Err(answer
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("The engine reported an error without a message.")
            .to_string())
    };
    Some(Event::Answer { id, result })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_are_read() {
        let Some(Event::Answer { id, result }) =
            parse_answer(r#"{"id":3,"ok":true,"result":{"a":1}}"#)
        else {
            panic!("an answer");
        };
        assert_eq!((id, result), (3, Ok(json!({"a": 1}))));

        let Some(Event::Answer { id, result }) =
            parse_answer(r#"{"id":4,"ok":false,"error":"no"}"#)
        else {
            panic!("an answer");
        };
        assert_eq!((id, result), (4, Err("no".to_string())));

        assert!(parse_answer("driver chatter").is_none());
    }
}
