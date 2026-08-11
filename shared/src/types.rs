use serde::{Deserialize, Serialize};

use crate::temporal::TimeRange;

// ---------------------------------------------------------------------------
// Signal states and transitions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SignalState {
    Low,
    High,
    HighZ,
    Undefined,
    Data(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    pub time_ns: u64,
    pub state: SignalState,
}

// ---------------------------------------------------------------------------
// Signal view — what the renderer receives
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalWaveform {
    pub name: String,
    pub transitions: Vec<Transition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockSummary {
    pub name: String,
    pub start_ns: u64,
    pub end_ns: u64,
    /// CSS-style color string, e.g. "#58a6ff"
    pub color: String,
    /// Short label displayed inside the block
    pub label: String,
}

/// The data the renderer uses to draw a signal or group.
/// Which variant is returned depends on the viewport's resolution
/// vs. the source's LOD thresholds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SignalView {
    /// Signal waveforms with an optional block overlay.
    /// Layout is ALWAYS based on `signals.len()` rows — never changes with zoom.
    /// `overlay` is drawn on top when present, eclipsing detail as `detail_alpha` drops.
    Waveform {
        signals: Vec<SignalWaveform>,
        /// Block overlay: drawn on top of signal rows when present
        #[serde(skip_serializing_if = "Option::is_none")]
        overlay: Option<BlockSummary>,
        /// How much signal detail to show: 1.0 = full detail, 0.0 = fully eclipsed by overlay
        #[serde(default = "default_detail_alpha")]
        detail_alpha: f64,
    },
}

fn default_detail_alpha() -> f64 {
    1.0
}

// ---------------------------------------------------------------------------
// Source manifest — describes the hierarchy before any data is loaded
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SignalKind {
    Wire,
    Bus { width: u8 },
    Clock,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SignalInfo {
    pub name: String,
    pub path: String,
    pub kind: SignalKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceManifest {
    pub name: String,
    pub children: Vec<ManifestNode>,
    pub time_span: TimeRange,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ManifestNode {
    Signal(SignalInfo),
    Group {
        name: String,
        path: String,
        children: Vec<ManifestNode>,
        time_span: TimeRange,
    },
}
