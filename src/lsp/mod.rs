//! A generic, synchronous Language Server Protocol client.
//!
//! This module is independent of any particular language server. It speaks
//! JSON-RPC over a server subprocess's standard streams and exposes request,
//! notification, and event primitives.

mod client;
mod transport;

pub use client::LspClient;
