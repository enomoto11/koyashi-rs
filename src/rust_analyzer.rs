//! Drives a rust-analyzer instance through the generic [`LspClient`] to answer
//! reference queries for a workspace.

use std::env;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::lsp::LspClient;
use crate::model::Location;

/// Environment variable that overrides the rust-analyzer executable path.
pub const RUST_ANALYZER_ENV: &str = "KOYASHI_RUST_ANALYZER";

/// Maximum time to wait for rust-analyzer to finish its initial analysis.
const READY_TIMEOUT: Duration = Duration::from_secs(120);

/// Resolve the rust-analyzer executable.
///
/// `KOYASHI_RUST_ANALYZER` takes precedence; otherwise the `PATH` is searched.
pub fn find_rust_analyzer() -> Result<PathBuf> {
    if let Ok(explicit) = env::var(RUST_ANALYZER_ENV) {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Ok(path);
        }
        bail!(
            "{RUST_ANALYZER_ENV} points to a non-existent file: {}",
            path.display()
        );
    }

    find_on_path("rust-analyzer").ok_or_else(|| {
        anyhow!(
            "rust-analyzer was not found on PATH; \
             install it from https://rust-analyzer.github.io or set {RUST_ANALYZER_ENV}"
        )
    })
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// A rust-analyzer session scoped to a single workspace.
pub struct Analyzer {
    client: LspClient,
}

impl Analyzer {
    /// Start rust-analyzer for `workspace_root` and wait until analysis settles.
    pub fn start(workspace_root: &Path) -> Result<Self> {
        let program = find_rust_analyzer()?;
        let client = LspClient::spawn(&program, std::iter::empty::<&str>(), workspace_root)?;
        let mut analyzer = Analyzer { client };
        analyzer.handshake(workspace_root)?;
        Ok(analyzer)
    }

    /// Return every reference to the symbol defined at `location`.
    pub fn references(&mut self, location: &Location) -> Result<Vec<Location>> {
        let params = json!({
            "textDocument": { "uri": encode_file_uri(&location.file) },
            "position": {
                "line": location.line.saturating_sub(1),
                "character": location.character,
            },
            "context": { "includeDeclaration": false },
        });

        let raw: Option<Vec<RawLocation>> = self
            .client
            .request("textDocument/references", params)
            .context("textDocument/references request failed")?;

        raw.unwrap_or_default()
            .into_iter()
            .map(RawLocation::into_location)
            .collect()
    }

    /// Ask the server to shut down and exit.
    pub fn shutdown(mut self) -> Result<()> {
        let _: Value = self.client.request("shutdown", Value::Null)?;
        self.client.notify("exit", Value::Null)?;
        Ok(())
    }

    fn handshake(&mut self, workspace_root: &Path) -> Result<()> {
        let root_uri = encode_file_uri(workspace_root);
        let init = json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": { "references": {} },
                "experimental": { "serverStatusNotification": true },
            },
            "workspaceFolders": [ { "uri": root_uri, "name": "workspace" } ],
        });

        let _: Value = self
            .client
            .request("initialize", init)
            .context("initialize request failed")?;
        self.client.notify("initialized", json!({}))?;
        self.wait_until_ready()
    }

    fn wait_until_ready(&mut self) -> Result<()> {
        let deadline = Instant::now() + READY_TIMEOUT;
        while Instant::now() < deadline {
            match self.client.next_notification()? {
                Some((method, params)) if method == "experimental/serverStatus" => {
                    let quiescent = params
                        .get("quiescent")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    if quiescent {
                        return Ok(());
                    }
                }
                Some(_) => {}
                None => bail!("rust-analyzer exited before analysis completed"),
            }
        }
        bail!("timed out waiting for rust-analyzer to finish analysis")
    }
}

/// A `Location` as received over the protocol, before path/coordinate mapping.
#[derive(Deserialize)]
struct RawLocation {
    uri: String,
    range: RawRange,
}

#[derive(Deserialize)]
struct RawRange {
    start: RawPosition,
}

#[derive(Deserialize)]
struct RawPosition {
    line: u32,
    character: u32,
}

impl RawLocation {
    fn into_location(self) -> Result<Location> {
        Ok(Location {
            file: decode_file_uri(&self.uri)?,
            // The protocol is zero-based; `Location` lines are one-based.
            line: self.range.start.line + 1,
            character: self.range.start.character,
        })
    }
}

/// Encode a filesystem path as a `file://` URI.
fn encode_file_uri(path: &Path) -> String {
    format!("file://{}", percent_encode(&path.to_string_lossy()))
}

/// Decode a `file://` URI back into a filesystem path.
fn decode_file_uri(uri: &str) -> Result<PathBuf> {
    let path = uri
        .strip_prefix("file://")
        .with_context(|| format!("expected a file URI, got `{uri}`"))?;
    Ok(PathBuf::from(percent_decode(path)?))
}

fn percent_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn percent_decode(input: &str) -> Result<String> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 3 <= bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3])?;
            decoded.push(u8::from_str_radix(hex, 16).context("invalid percent-encoding")?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).context("decoded URI is not valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uri_round_trips() {
        let path = Path::new("/tmp/koyashi test/src/main.rs");
        let uri = encode_file_uri(path);
        assert!(uri.starts_with("file:///tmp/koyashi%20test/"));
        assert_eq!(decode_file_uri(&uri).unwrap(), path);
    }
}
