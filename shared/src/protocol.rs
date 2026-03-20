use serde::{Deserialize, Serialize};
use ws_bridge::WsEndpoint;

use crate::temporal::TimeRange;
use crate::types::{SignalView, SourceManifest};

// ---------------------------------------------------------------------------
// WebSocket endpoint — single source of truth for server + client
// ---------------------------------------------------------------------------

pub struct SigIntSocket;

impl WsEndpoint for SigIntSocket {
    const PATH: &'static str = "/ws";
    type ServerMsg = ServerMsg;
    type ClientMsg = ClientMsg;
}

// ---------------------------------------------------------------------------
// Client -> Server
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMsg {
    /// Request signal data for a viewport region
    Query {
        time_range: TimeRange,
        resolution_ns: u64,
        /// Generation counter — server echoes this back in Tile responses
        #[serde(default)]
        generation: u64,
    },
    /// Request the full source hierarchy
    GetManifest,
    /// User edited the source text — recompile
    UpdateSource { text: String },
    /// Keep-alive ping
    Ping,
}

// ---------------------------------------------------------------------------
// Server -> Client
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMsg {
    /// Signal data tile for rendering
    Tile {
        path: String,
        view: SignalView,
        /// Echoed from the Query that produced this tile
        #[serde(default)]
        generation: u64,
    },
    /// Full source tree manifest
    Manifest {
        manifest: SourceManifest,
    },
    /// Parse or compile error
    Error { message: String },
    /// Keep-alive response
    Heartbeat,
}
