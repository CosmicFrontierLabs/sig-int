# sig-int: Signal Source Trait Design

## The Core Abstraction

Everything that produces signal data implements one trait. The zoom level
determines *which* trait method gets called and *what* comes back.

---

## Sketch

```rust
use std::ops::Range;
use std::time::Duration;

/// A time range query at a specific resolution
pub struct ViewQuery {
    /// Absolute time range in the parent's coordinate system
    pub time_range: Range<Duration>,
    /// Desired resolution: time per pixel (or per sample point)
    /// The implementation should return ~1 sample per this interval
    pub resolution: Duration,
}

/// What a signal source returns depends on the detail level.
/// This is not one-size-fits-all — the *shape* of the response changes.
pub enum SignalView {
    /// Zoomed way out: just metadata about the block
    /// "This is a MIPI TX, it's active here, 1.2Gbps"
    Block(BlockSummary),

    /// Mid zoom: activity envelope without individual transitions
    /// "Signal is mostly high here, toggling fast here, idle here"
    Envelope(EnvelopeData),

    /// Zoomed in: actual signal states per sample point
    /// "0, 1, 1, 0, x, z, =" — WaveDrom-compatible
    Waveform(WaveformData),
}

/// The trait every signal source implements
pub trait SignalSource: Send + Sync {
    /// Metadata: what signals does this source provide?
    fn manifest(&self) -> SourceManifest;

    /// Query: give me renderable data for this viewport
    fn view(&self, query: &ViewQuery) -> SignalView;

    /// What children does this source have? (for hierarchy traversal)
    fn children(&self) -> Vec<Box<dyn SignalSource>>;

    /// At what resolution thresholds does the detail level change?
    fn lod_thresholds(&self) -> LodThresholds;
}
```

## SourceManifest

Describes what a source provides before any data is loaded:

```rust
pub struct SourceManifest {
    /// Human-readable name
    pub name: String,
    /// Signal names and types at this level
    pub signals: Vec<SignalInfo>,
    /// Port signals visible even when collapsed
    pub ports: Vec<PortInfo>,
    /// Total time span this source covers
    pub time_span: Range<Duration>,
    /// Native timescale (finest resolution this source has)
    pub native_resolution: Duration,
    /// Static annotations (data rate, description, etc.)
    pub annotations: Vec<Annotation>,
}

pub struct SignalInfo {
    pub name: String,
    pub kind: SignalKind,
}

pub enum SignalKind {
    Wire,              // single bit
    Bus { width: u8 }, // multi-bit
    Clock { freq: f64 },
    Analog { range: (f64, f64) },
}

pub struct PortInfo {
    pub signal: SignalInfo,
    pub direction: PortDirection,
    /// Which edges on this port trigger causal connections?
    pub edge_triggers: Vec<EdgeTrigger>,
}

pub enum PortDirection { In, Out, InOut }

pub struct EdgeTrigger {
    pub edge: Edge,         // Rising, Falling, Either
    pub target: String,     // path to the signal/block this triggers
    pub typical_delay: Option<Duration>,
}
```

## LOD Thresholds

Each source declares when to switch representation:

```rust
pub struct LodThresholds {
    /// Above this resolution (coarser), render as Block
    pub block_threshold: Duration,
    /// Above this resolution, render as Envelope
    /// Below this, render as Waveform
    pub envelope_threshold: Duration,
}
```

Example for a MIPI frame block:
```rust
LodThresholds {
    // Coarser than 1μs/px → just show the block rectangle
    block_threshold: Duration::from_micros(1),
    // Coarser than 1ns/px → show envelope (activity bands)
    // Finer than 1ns/px → show full waveform
    envelope_threshold: Duration::from_nanos(1),
}
```

## The Three Response Types

### BlockSummary — "I'm a rectangle with a label"

```rust
pub struct BlockSummary {
    pub name: String,
    pub time_span: Range<Duration>,
    pub annotations: Vec<Annotation>,
    /// Mini sparkline of activity density (for the block fill)
    pub activity: Vec<f32>,
    /// Port edge events visible on the block boundary
    pub port_events: Vec<PortEvent>,
}

pub struct PortEvent {
    pub port: String,
    pub time: Duration,
    pub edge: Edge,
}
```

This is all the renderer needs to draw a collapsed block at the system level,
including the edge connection points where causal arrows attach.

### EnvelopeData — "Here's the shape, not the details"

```rust
pub struct EnvelopeData {
    pub signals: Vec<SignalEnvelope>,
    pub time_range: Range<Duration>,
    pub sample_interval: Duration,
}

pub struct SignalEnvelope {
    pub name: String,
    /// Per sample point: (min_state, max_state, transition_density)
    /// For a wire: min/max are 0 or 1, density is toggles per interval
    /// For a bus: min/max are value range, density is change rate
    pub samples: Vec<EnvelopeSample>,
}

pub struct EnvelopeSample {
    pub min: SignalState,
    pub max: SignalState,
    /// 0.0 = static, 1.0 = toggling every cycle
    pub density: f32,
}
```

Renderer draws this as colored bands (thick when active, thin when idle)
without individual transitions. Think oscilloscope persistence mode.

### WaveformData — "Here are the actual transitions"

```rust
pub struct WaveformData {
    pub signals: Vec<SignalWaveform>,
    pub time_range: Range<Duration>,
}

pub struct SignalWaveform {
    pub name: String,
    pub transitions: Vec<Transition>,
}

pub struct Transition {
    pub time: Duration,
    pub state: SignalState,
}

pub enum SignalState {
    Low,
    High,
    HighZ,
    Undefined,
    Data(DataValue),
    Clock(ClockPhase),
    WeakHigh,
    WeakLow,
}

pub enum DataValue {
    /// Named value like "A0", "0x4F"
    Named(String),
    /// Numeric with bus width
    Numeric { value: u64, width: u8 },
}
```

This is what gets rendered as classic WaveDrom-style waveforms.
It's also what can be **exported** as a WaveDrom JSON document.

---

## Implementations

The trait is the plug-in point. Different implementations for different data sources:

### InlineSource — WaveDrom-style wave strings

```rust
/// A flat signal defined by a wave string. Always returns Waveform.
/// This is what you write by hand in the editor.
struct InlineSource {
    name: String,
    wave: String,       // "p..1..0..x"
    data: Vec<String>,  // ["A0", "A1"]
    period: f64,
    phase: f64,
    timescale: Duration,
}

impl SignalSource for InlineSource {
    fn view(&self, query: &ViewQuery) -> SignalView {
        // Always has full data, just slice to the query range
        // If query resolution is much coarser than native, return Envelope
        if query.resolution > self.timescale * 100 {
            SignalView::Envelope(self.compute_envelope(query))
        } else {
            SignalView::Waveform(self.slice(query))
        }
    }
    fn children(&self) -> Vec<Box<dyn SignalSource>> { vec![] }
}
```

### GroupSource — Hierarchical block with children

```rust
/// A module/block containing child signal sources.
/// Returns Block when zoomed out, delegates to children when zoomed in.
struct GroupSource {
    manifest: SourceManifest,
    children: Vec<Box<dyn SignalSource>>,
    thresholds: LodThresholds,
    summary_cache: Option<BlockSummary>,
}

impl SignalSource for GroupSource {
    fn view(&self, query: &ViewQuery) -> SignalView {
        if query.resolution > self.thresholds.block_threshold {
            // Zoomed out: return pre-computed block summary
            SignalView::Block(self.summary())
        } else {
            // Zoomed in: caller should traverse children directly
            // (or we could aggregate children's views here)
            SignalView::Waveform(self.aggregate_children(query))
        }
    }
    fn children(&self) -> Vec<Box<dyn SignalSource>> { ... }
}
```

### VcdSource — Lazy-loaded from a VCD file

```rust
/// Reads a VCD file on demand. Builds an LOD pyramid on first load.
struct VcdSource {
    path: PathBuf,
    index: VcdIndex,  // pre-built: signal list, time range, block boundaries
    cache: TileCache, // LRU cache of loaded time ranges at various resolutions
}

impl SignalSource for VcdSource {
    fn view(&self, query: &ViewQuery) -> SignalView {
        // Load only the requested range from the VCD file
        // Downsample to match query resolution
        // Cache the result as tiles
    }
}
```

### ComputedSource — Derived from expressions

```rust
/// A signal computed from other signals: "a & b", "!reset", etc.
struct ComputedSource {
    name: String,
    expr: BoolExpr,
    inputs: Vec<Box<dyn SignalSource>>,
}

impl SignalSource for ComputedSource {
    fn view(&self, query: &ViewQuery) -> SignalView {
        // Query inputs, evaluate expression per sample point
        let input_views: Vec<_> = self.inputs.iter()
            .map(|s| s.view(query))
            .collect();
        self.evaluate(input_views, query)
    }
}
```

---

## Wire Protocol (Frontend ↔ Backend)

The frontend sends viewport changes, the backend responds with `SignalView` data:

```rust
// Client → Server
pub enum ViewRequest {
    /// Viewport changed (pan/zoom)
    Query {
        /// Which signals (path globs)
        signals: Vec<String>,
        /// Time range and resolution
        query: ViewQuery,
    },
    /// Expand a collapsed block
    Expand { path: String },
    /// Collapse a block
    Collapse { path: String },
}

// Server → Client
pub enum ViewResponse {
    /// Tile of signal data for the requested viewport
    Tile {
        path: String,
        view: SignalView,
    },
    /// Hierarchy changed (block expanded/collapsed)
    LayoutUpdate {
        tree: SourceManifest,
    },
    /// Data still loading, here's a placeholder
    Loading {
        path: String,
        time_range: Range<Duration>,
    },
}
```

These go over the WebSocket from the rust-skeleton's ws-bridge.
Tiles are cached on both sides. The backend pre-computes coarse LOD
levels on ingest; fine LOD levels are computed on demand.

---

## Key Properties

1. **The trait doesn't know about rendering.** It returns typed data, not pixels or SVG.
   The canvas renderer decides how to draw each `SignalView` variant.

2. **LOD is per-source, not global.** A 100MHz SPI bus and a 1.5GHz MIPI link
   in the same diagram have different thresholds. The system handles this naturally.

3. **Children are lazy.** `children()` returns the manifest, not the data.
   Data is only loaded when `view()` is called for a specific time range.

4. **Block summaries are cheap.** Pre-computed on ingest or on first query.
   The system-level view (all blocks collapsed) loads instantly regardless
   of how much underlying data exists.

5. **WaveDrom is just one SignalSource impl.** Import a WaveJSON file →
   `InlineSource` for each signal, `GroupSource` for each group. Done.
