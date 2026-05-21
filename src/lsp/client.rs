//! A synchronous Language Server Protocol client driving a server subprocess.

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::io::BufReader;
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::transport;

/// A server notification: its method name paired with its `params` payload.
pub type Notification = (String, Value);

/// A client that drives a language server over its standard input and output.
pub struct LspClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
    notifications: VecDeque<Notification>,
}

impl LspClient {
    /// Spawn a language server process and connect to it over stdio.
    pub fn spawn<I, S>(program: &Path, args: I, working_dir: &Path) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut child = Command::new(program)
            .args(args)
            .current_dir(working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to spawn {}", program.display()))?;

        let stdin = child.stdin.take().context("server stdin was not captured")?;
        let stdout = child.stdout.take().context("server stdout was not captured")?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 0,
            notifications: VecDeque::new(),
        })
    }

    /// Send a request and block until the matching response arrives.
    ///
    /// Notifications received while waiting are queued for [`Self::next_notification`].
    /// Server-to-client requests are answered with default replies.
    pub fn request<P, R>(&mut self, method: &str, params: P) -> Result<R>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))?;

        loop {
            match self.read_incoming()? {
                Some(Incoming::Response {
                    id: response_id,
                    result,
                }) if response_id == id => {
                    return serde_json::from_value(result?)
                        .with_context(|| format!("unexpected result shape for `{method}`"));
                }
                Some(Incoming::Response { .. }) => {}
                Some(Incoming::Notification(notification)) => {
                    self.notifications.push_back(notification);
                }
                Some(Incoming::ServerRequest { id, method, params }) => {
                    self.answer_server_request(id, &method, &params)?;
                }
                None => bail!("server closed the connection while awaiting `{method}`"),
            }
        }
    }

    /// Send a notification, which expects no response.
    pub fn notify<P: Serialize>(&mut self, method: &str, params: P) -> Result<()> {
        self.send(json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
    }

    /// Return the next server notification, reading from the server as needed.
    ///
    /// Returns `Ok(None)` once the server closes the connection.
    pub fn next_notification(&mut self) -> Result<Option<Notification>> {
        if let Some(notification) = self.notifications.pop_front() {
            return Ok(Some(notification));
        }
        loop {
            match self.read_incoming()? {
                Some(Incoming::Notification(notification)) => return Ok(Some(notification)),
                Some(Incoming::ServerRequest { id, method, params }) => {
                    self.answer_server_request(id, &method, &params)?;
                }
                Some(Incoming::Response { .. }) => {}
                None => return Ok(None),
            }
        }
    }

    fn send(&mut self, message: Value) -> Result<()> {
        transport::write_message(&mut self.stdin, &message)
    }

    fn read_incoming(&mut self) -> Result<Option<Incoming>> {
        match transport::read_message(&mut self.stdout)? {
            Some(message) => classify(message).map(Some),
            None => Ok(None),
        }
    }

    /// Reply to a server-to-client request with a minimal valid result.
    fn answer_server_request(&mut self, id: Value, method: &str, params: &Value) -> Result<()> {
        let result = match method {
            // `workspace/configuration` expects one result entry per requested item.
            "workspace/configuration" => {
                let count = params
                    .get("items")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                Value::Array(vec![Value::Null; count])
            }
            _ => Value::Null,
        };
        self.send(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A message received from the server.
enum Incoming {
    Response { id: i64, result: Result<Value> },
    Notification(Notification),
    ServerRequest { id: Value, method: String, params: Value },
}

fn classify(message: Value) -> Result<Incoming> {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let params = || message.get("params").cloned().unwrap_or(Value::Null);

    match (method, message.get("id")) {
        (Some(method), Some(id)) => Ok(Incoming::ServerRequest {
            id: id.clone(),
            method,
            params: params(),
        }),
        (Some(method), None) => Ok(Incoming::Notification((method, params()))),
        (None, Some(id)) => {
            let id = id.as_i64().context("response carried a non-integer id")?;
            let result = match message.get("error") {
                Some(error) => {
                    let detail = error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown error");
                    Err(anyhow!("language server error: {detail}"))
                }
                None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
            };
            Ok(Incoming::Response { id, result })
        }
        (None, None) => bail!("malformed message from language server"),
    }
}
