# sig-int: Implementation Plan

> A hierarchical signal description language with a zoomable canvas browser.
> WaveDrom is an output format, not the source of truth.
> Backend-driven tile serving. Multi-timescale. Lazy data. Dark-themed.

See also: `VISION.md` (use cases & motivation), `TRAIT_SKETCH.md` (core trait design).

---

## 1. Architecture

```
┌─────────────────────────────┐       ┌──────────────────────────────────────┐
│  Frontend (Yew / WASM)      │  WS   │  Backend (Axum)                      │
│                             │◄─────►│                                      │
│  ┌───────────┐ ┌──────────┐│       │  ┌────────────┐  ┌────────────────┐  │
│  │ Editor    │ │ Canvas   ││       │  │ Tile Server │  │ SignalSource   │  │
│  │ (DSL/JSON)│ │ Renderer ││       │  │ (WS handler)│  │ Registry       │  │
│  └─────┬─────┘ └────┬─────┘│       │  └──────┬─────┘  │                │  │
│        │ text        │ tiles│       │         │        │ ┌────────────┐ │  │
│        ▼             ▼      │       │         ├───────►│ │InlineSource│ │  │
│  ┌───────────────────────┐  │       │         │        │ │GroupSource │ │  │
│  │ Viewport State        │──┼───────┼────────►│        │ │VcdSource   │ │  │
│  │ (time range, signals, │  │ query │         │        │ │Computed..  │ │  │
│  │  resolution)          │  │       │         │        │ └────────────┘ │  │
│  └───────────────────────┘  │       │         │        └────────────────┘  │
│                             │       │         │                            │
│  ┌───────────────────────┐  │       │  ┌──────┴─────┐  ┌──────────────┐   │
│  │ Tile Cache (LRU)      │  │       │  │ LOD Engine │  │ DSL Compiler │   │
│  └───────────────────────┘  │       │  └────────────┘  └──────────────┘   │
└─────────────────────────────┘       └──────────────────────────────────────┘
```

### Project Structure

```
sig-int/
  Cargo.toml                        # Workspace: frontend, backend, shared
  frontend/
    Cargo.toml                      # Yew + web-sys + gloo
    Trunk.toml
    index.html
    style.css                       # Dark theme CSS variables
    src/
      main.rs                       # Yew entry
      app.rs                        # Split pane layout
      editor/
        mod.rs                      # Editor panel (textarea Phase 1, tree-sitter Phase 3)
        highlighting.rs             # Tree-sitter syntax highlighting (Phase 3)
      canvas/
        mod.rs                      # Canvas Yew component
        renderer.rs                 # Draws SignalView variants to canvas
        viewport.rs                 # Zoom/pan state, temporal range calculation
        layout.rs                   # Vertical layout: row positions, hit testing
        theme.rs                    # Dark theme color palette
      tile_cache.rs                 # Client-side LRU tile cache
      ws_client.rs                  # WebSocket: send ViewRequest, receive ViewResponse
  backend/
    Cargo.toml                      # Axum + tokio + ws-bridge
    src/
      main.rs                       # Axum server, static asset serving
      tile_server.rs                # WebSocket handler: viewport queries -> tiles
      source_registry.rs            # Manages the SignalSource tree
      lod.rs                        # LOD threshold evaluation, downsample logic
      ingest/
        mod.rs                      # Format detection, source construction
        wavejson.rs                 # WaveJSON import -> InlineSource + GroupSource
        vcd.rs                      # VCD import -> VcdSource (Phase 5)
      handlers/
        health.rs
        websocket.rs
  shared/
    Cargo.toml                      # serde + types only (no platform deps)
    src/
      lib.rs
      protocol.rs                   # ViewRequest, ViewResponse, ws-bridge endpoint
      signal_source.rs              # SignalSource trait + SignalView enum
      types.rs                      # SignalState, Transition, Envelope, BlockSummary
      temporal.rs                   # TimeRange, Resolution, temporal addressing
      manifest.rs                   # SourceManifest, SignalInfo, PortInfo
  grammar/                          # (Phase 3+) tree-sitter grammars
```

Key difference from rust-skeleton: no database (diesel/postgres). The backend
is a **stateful tile server**, not a CRUD app. State = the loaded SignalSource tree.

---

## 2. Core Abstractions (shared crate)

### 2.1 The SignalSource Trait

The single most important type in the system. See `TRAIT_SKETCH.md` for full details.

```rust
pub trait SignalSource: Send + Sync {
    fn manifest(&self) -> SourceManifest;
    fn view(&self, query: &ViewQuery) -> SignalView;
    fn children(&self) -> Vec<&dyn SignalSource>;
    fn lod_thresholds(&self) -> LodThresholds;
}
```

Three response types based on zoom resolution:
- `SignalView::Block` — zoomed way out: labeled rectangle with port events
- `SignalView::Envelope` — mid zoom: activity bands (min/max/density per sample)
- `SignalView::Waveform` — zoomed in: individual transitions (WaveDrom-compatible)

### 2.2 Temporal Model

No global clock. Time is a tree of coordinate systems:

```rust
pub struct TimeRange {
    pub start: Duration,
    pub end: Duration,
}

pub struct ViewQuery {
    pub time_range: TimeRange,
    /// Desired time per output sample — the source downsamples to this
    pub resolution: Duration,
}
```

Each SignalSource has:
- A `native_resolution` (finest data it can provide)
- A `time_span` (total duration it covers)
- LOD thresholds that determine which `SignalView` variant to return

Blocks are nested: a block's internal timeline has its own coordinate system,
offset relative to its parent. The backend resolves temporal addresses like
`camera.mipi_frame[0].d0 @ 4.2μs:4.8μs` by walking the tree.

### 2.3 Wire Protocol

```rust
// Frontend -> Backend (over WebSocket)
pub enum ViewRequest {
    /// Viewport changed — send me tiles for this view
    Query {
        signals: Vec<String>,       // path globs: "camera.*", "memory.dma"
        time_range: TimeRange,
        resolution: Duration,       // time per pixel
    },
    /// Get the full source hierarchy (on connect, or after DSL change)
    GetManifest,
    /// User edited the DSL — recompile
    UpdateSource { text: String },
    /// Expand/collapse a block manually
    ToggleBlock { path: String },
}

// Backend -> Frontend (over WebSocket)
pub enum ViewResponse {
    /// Signal data tile for rendering
    Tile {
        path: String,
        view: SignalView,
    },
    /// Full source tree structure
    Manifest(SourceManifest),
    /// Block is still loading data (VCD seek, etc.)
    Loading { path: String, time_range: TimeRange },
    /// Parse/compile error from DSL update
    Error { message: String, span: Option<(usize, usize)> },
}
```

---

## 3. Canvas Rendering Engine

### 3.1 Why Canvas

- Performance at scale (thousands of cycles, 60fps zoom/pan)
- Pixel control for hatched regions, gradient fills, anti-aliased transitions
- Native transform matrix for zoom — no DOM node overhead
- LOD rendering: fundamentally different drawing at each scale

### 3.2 Viewport

```rust
pub struct Viewport {
    /// Time range visible on screen
    pub time_range: TimeRange,
    /// Time per pixel (derived from time_range / canvas_width)
    pub resolution: Duration,
    /// Vertical scroll offset in pixels
    pub v_offset: f64,
    /// Canvas pixel dimensions
    pub canvas_size: (f64, f64),
}
```

Zoom is temporal, not spatial: zooming in means narrowing the `time_range`,
which increases resolution, which may cross LOD thresholds and trigger new
tile requests to the backend.

### 3.3 What Gets Drawn at Each LOD

| Resolution (time/pixel) | Drawing strategy |
|---|---|
| > 100ms/px | **Block view**: Module rectangles, labels, causal edge arrows. Port annotations on block boundaries. No individual signals. |
| 1ms – 100ms/px | **Envelope view**: Activity bands per signal (colored strips showing min/max/density). Transaction markers as dots/diamonds. |
| 1μs – 1ms/px | **State band view**: Signals as colored rectangles (high=green, low=dim, data=colored, x=orange). No transition slopes. |
| 1ns – 1μs/px | **Waveform view**: Full WaveDrom-style rendering — transitions, slopes, data labels, clock arrows. |
| < 1ns/px | **Sub-cycle view**: Rise/fall times, analog-ish rendering, over-sampling detail. |

### 3.4 Rendering Pipeline

```
ViewResponse::Tile arrives
  -> tile_cache.insert(path, view)
  -> if tile intersects current viewport:
       schedule_redraw()

redraw():
  ctx.clear()
  for each visible row in layout:
    match tile_cache.get(row.path, viewport):
      Some(SignalView::Block(b))    -> draw_block(ctx, b, row)
      Some(SignalView::Envelope(e)) -> draw_envelope(ctx, e, row)
      Some(SignalView::Waveform(w)) -> draw_waveform(ctx, w, row)
      None                          -> draw_loading_placeholder(ctx, row)
  draw_causal_edges(ctx)
  draw_cursor(ctx)
```

### 3.5 Zoom / Pan Interaction

- **Mouse wheel**: Zoom time axis (narrow/widen `time_range`), anchored at cursor
- **Ctrl+wheel**: Zoom vertical (scale row heights)
- **Click+drag**: Pan (shift `time_range` and `v_offset`)
- **Pinch**: Trackpad zoom via wheel events with `ctrlKey`
- On zoom: compute new `resolution`, check LOD thresholds, fire `ViewRequest::Query`
  if new tiles are needed
- Debounce rapid zoom (request tiles at most every 50ms during continuous zoom)

### 3.6 Block Interaction

Clicking a collapsed block either:
- **Expands it** (manual toggle → `ViewRequest::ToggleBlock`)
- **Zooms into it** (double-click → set viewport to the block's time range)

Expanded blocks show children inline. The layout engine inserts child rows
beneath the block header with appropriate indentation.

### 3.7 Dark Theme

```css
:root {
  --bg-primary:    #0d1117;
  --bg-secondary:  #161b22;
  --bg-tertiary:   #1c2128;
  --border:        #30363d;
  --text-primary:  #c9d1d9;
  --text-secondary:#8b949e;
  --accent-blue:   #58a6ff;
  --accent-green:  #3fb950;
  --accent-red:    #f85149;
  --accent-orange: #f0883e;
  --accent-purple: #d2a8ff;
  --accent-pink:   #f778ba;
  --font-mono:     'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace;
}
```

Canvas colors mirror these CSS variables so the editor and canvas feel unified.

---

## 4. Backend: Tile Server

### 4.1 Role

The backend is **not** a database-backed API. It is a stateful signal server:
- Holds the loaded SignalSource tree in memory
- Receives viewport queries over WebSocket
- Calls `source.view(query)` to produce tiles
- Sends tiles back to the frontend
- Handles DSL compilation and source tree updates

### 4.2 Query Processing

```
ViewRequest::Query { signals, time_range, resolution }
  -> resolve signal paths (glob matching against source tree)
  -> for each matched source:
       check source.lod_thresholds() vs resolution
       call source.view(ViewQuery { time_range, resolution })
       serialize SignalView
       send ViewResponse::Tile { path, view }
```

For large lazy sources (VCD), `view()` may need to seek/read from disk.
This happens async on a tokio blocking task; the tile server sends
`ViewResponse::Loading` immediately and `ViewResponse::Tile` when ready.

### 4.3 Source Registry

```rust
pub struct SourceRegistry {
    root: Box<dyn SignalSource>,
    /// Path index for fast glob resolution
    path_index: HashMap<String, *const dyn SignalSource>,
}
```

Built on initial load (from DSL compilation or WaveJSON import).
Rebuilt when the user edits the source text.

### 4.4 Embedded Frontend

Following the rust-skeleton pattern: the compiled WASM frontend is embedded
in the backend binary via `rust-embed`. Single binary deployment.
Backend serves the frontend on `/` and the WebSocket on `/ws`.

---

## 5. Parsing & Source Construction

### 5.1 Phase 1: WaveJSON Import (serde)

```
JSON text -> serde_json::from_str::<WaveJsonDoc>()
  -> for each signal: InlineSource
  -> for each group: GroupSource wrapping children
  -> root GroupSource = the whole document
```

This gets us a working demo with basic WaveDrom documents.

### 5.2 Phase 3: Tree-Sitter Integration

Tree-sitter runs on the **frontend** for the editor experience (syntax highlighting,
error markers, incremental re-parse on keystroke). The CST is used for:
1. Syntax highlighting in the editor overlay
2. Error reporting (red underlines on ERROR nodes)
3. Extracting the source text to send to the backend for compilation

The **backend** does the actual compilation (serde or custom parser). This split
means the editor is responsive (tree-sitter is fast) and the backend handles
the heavy semantic work.

```
Frontend:                          Backend:
keystroke                          ViewRequest::UpdateSource { text }
  -> tree-sitter parse (< 1ms)      -> compile(text)
  -> update syntax highlighting      -> rebuild SourceRegistry
  -> debounce 100ms                  -> send new Manifest
  -> send UpdateSource               -> invalidate tile cache
```

### 5.3 Phase 4: sig-int DSL

A text-based DSL (sketched in `VISION.md` §3) compiled on the backend:

```
module camera {
  port exposure: wire
  port vsync: wire

  block mipi_frame @ 1ns {
    duration: 8.3ms
    signal clk_p: clock @ 1.5GHz
    signal d0: wire
    annotation data_rate = "1.2 Gbps"
  }

  edge vsync.rising -> mipi_frame.start
    delay: 1.2ms
}
```

The DSL compiles to a SignalSource tree. WaveJSON import is just one compiler
frontend; the DSL is another. Both produce the same in-memory model.

### 5.4 Phase 5: VCD Import

Uses `tree-sitter-vcd` (maintained by the WaveDrom project) or a Rust VCD parser
crate. Builds a `VcdSource` that lazy-loads sample data on demand.

VCD files can be massive — the ingest step builds an **index** (signal list,
time boundaries, seek offsets) without loading all samples. Samples are loaded
per-tile when `view()` is called.

---

## 6. WaveDrom Compatibility

### 6.1 Input

Standard WaveJSON documents are valid sig-int input. They're imported as a
flat GroupSource with InlineSources for each signal. Character indices become
the time coordinate (1 character = 1 tick at the default timescale).

WaveJSON extensions (extra keys) are read if present, ignored by WaveDrom:
- `"sigint_version"`: format version marker
- `"block": true` on groups: treat as functional encapsulation
- `"timescale"`: physical time units
- `"type"` on signals: semantic classification (clock/data/control)
- `"meta"` on groups: description, port list, annotations

### 6.2 Output

"Export as WaveDrom" flattens the current viewport to a WaveJSON document:
- Visible signals become signal objects with wave strings
- Visible groups become nested arrays
- Time range is cropped to the viewport
- LOD is baked in (whatever the canvas is showing becomes the export)

This means you can use sig-int to design complex multi-timescale systems
and export individual views as standard WaveDrom for documentation.

### 6.3 Extension Principles

1. **Additive only**: Never change the meaning of existing WaveJSON keys
2. **Graceful degradation**: Strip sig-int extensions → valid WaveDrom document
3. **Versioned**: `"sigint_version": 1` marks documents using extensions

---

## 7. Implementation Phases

### Phase 1 — Skeleton + Basic Rendering

**Goal**: Split-pane app, textarea editor, canvas rendering basic waveforms.

1. Project scaffold: Cargo workspace (frontend, backend, shared)
2. Backend: Axum server, embedded frontend, WebSocket endpoint
3. Shared: Wire protocol types, basic SignalSource trait, SignalView enum
4. Frontend: Yew shell, split pane, textarea editor
5. WaveJSON import via serde (backend)
6. Canvas: draw `SignalView::Waveform` — signals 0/1/p/n/x/z/=, transitions, labels
7. Wire it up: editor text → backend → tile → canvas
8. Dark theme CSS + canvas colors
9. Basic zoom/pan (mouse wheel + drag)

### Phase 2 — LOD + Blocks

**Goal**: Semantic zoom, hierarchical groups, multi-resolution rendering.

10. LOD thresholds on GroupSource
11. `SignalView::Block` rendering (collapsed rectangles with labels)
12. `SignalView::Envelope` rendering (activity bands)
13. Automatic collapse/expand based on viewport resolution
14. Manual block toggle (click to override)
15. Layout engine: compute row positions with nested groups
16. Animated expand/collapse transitions
17. Block edge annotations (port signals on collapsed blocks)

### Phase 3 — Tree-Sitter Editor

**Goal**: Syntax highlighting, error recovery, incremental parsing.

18. Integrate `web-tree-sitter` on frontend (tree-sitter-json grammar)
19. Syntax highlighting overlay on textarea
20. Error markers (red underline on parse errors)
21. Incremental reparsing on keystroke
22. Status bar: parse status, error messages with source spans

### Phase 4 — sig-int DSL

**Goal**: Custom language for describing multi-timescale signal hierarchies.

23. DSL grammar design (formalize the sketch from VISION.md §3)
24. Tree-sitter grammar for the DSL (`grammar/grammar.js`)
25. Backend compiler: DSL text → SignalSource tree
26. Causal edges: first-class edge primitives with timing data
27. Edge rendering on canvas (arrows between blocks and signals)
28. Temporal addressing: `module.block[n].signal @ time` paths
29. Named regions / bookmarks

### Phase 5 — Data Import & Scale

**Goal**: Handle real-world captured data at scale.

30. VCD import: tree-sitter-vcd or Rust VCD parser
31. VCD indexing: build seek-offset index without loading all samples
32. Lazy tile loading: `VcdSource.view()` reads only the requested range
33. Tile cache (backend): LRU cache of computed tiles
34. Tile cache (frontend): LRU cache of received tiles
35. Loading placeholders: show "loading" shimmer while tiles arrive
36. Computed signals: `"expr": "a & b"` evaluated on the backend

### Phase 6 — Polish

**Goal**: Production-quality UX.

37. Keyboard shortcuts (Ctrl+/-/0 zoom, Home to fit, Esc to deselect)
38. Cursor / measurement tool (click to mark time, drag to measure interval)
39. WaveDrom export (flatten current view to WaveJSON)
40. PNG/SVG export of current view
41. URL sharing (encode source in URL hash or short-link)
42. Signal search / filter
43. Tooltip on hover (signal value at cursor time)

---

## 8. Key Dependencies

### Frontend (Yew / WASM)

| Crate | Purpose |
|---|---|
| `yew` 0.21 | UI framework (CSR mode) |
| `wasm-bindgen` 0.2 | JS interop |
| `web-sys` 0.3 | Canvas2D, DOM, mouse/wheel events |
| `js-sys` 0.3 | JS type conversions |
| `gloo-timers` 0.3 | Debounce, setTimeout |
| `gloo-events` 0.3 | Event listeners |
| `ws-bridge` 0.1 | Typed WebSocket (yew-client feature) |
| `serde` 1 | Serialization |
| `web-tree-sitter-sg` | Tree-sitter WASM bindings (Phase 3) |

### Backend (Axum)

| Crate | Purpose |
|---|---|
| `axum` 0.7 | HTTP + WebSocket server |
| `tokio` 1 | Async runtime |
| `tower-http` 0.6 | CORS, static file serving, tracing |
| `ws-bridge` 0.1 | Typed WebSocket (server feature) |
| `rust-embed` 8 | Embed compiled frontend in binary |
| `serde` + `serde_json` 1 | JSON parsing, wire protocol |
| `tracing` 0.1 | Logging |
| `clap` 4 | CLI args |
| `anyhow` 1 | Error handling |

### Shared

| Crate | Purpose |
|---|---|
| `serde` 1 | Trait serialization |
| `ws-bridge` 0.1 | WebSocket endpoint definition |

No database. No diesel. No postgres.

---

## 9. Design Decisions

### Backend-driven tiles vs. frontend-only rendering
The multi-timescale problem means data can be enormous (billions of samples
for a real VCD capture). The frontend can't hold it all — the backend serves
slices on demand. Even for small inline documents, the backend computes LOD
so the frontend renderer stays simple (it just draws what it receives).

### SignalSource trait as the core abstraction
Everything is a SignalSource: inline wave strings, hierarchical groups, VCD files,
computed signals. The tile server doesn't care what's behind the trait — it just
calls `view()`. New data sources plug in by implementing one trait.

### LOD is per-source, not global
A 1.5GHz MIPI link and a 100kHz I2C bus coexist in the same view. Each source
declares its own LOD thresholds. The canvas renders Block/Envelope/Waveform
independently per row based on what the backend sent.

### Canvas over SVG
Non-negotiable at this scale. Thousands of signals, billions of potential samples,
60fps zoom/pan. Canvas is O(pixels), SVG is O(DOM nodes).

### Tree-sitter on frontend, compilation on backend
Tree-sitter is fast enough for keystroke-level syntax highlighting on the frontend.
But semantic compilation (resolving temporal addresses, building the source tree,
evaluating expressions) happens on the backend where we have full Rust + disk access.

### WaveDrom as output, not input format
sig-int's source language is richer than WaveJSON. WaveJSON import is supported
(backwards compat) but the sig-int DSL is the primary authoring format. WaveJSON
export gives interop with existing WaveDrom tooling.

---

## 10. Open Questions

- [ ] **DSL syntax**: Text-based (as in VISION.md)? Or structured format (TOML/JSON variant)?
  Leaning text-based — it's more natural for the signal/block/edge primitives.
- [ ] **Generator interface**: How do external data sources (VCD, sigrok, live) plug in?
  Options: compiled-in Rust trait impls, WASM plugin modules, HTTP data endpoints.
- [ ] **Analog signals**: Support float-valued waveforms? The rendering model handles it
  (envelope at low zoom, actual trace at high zoom) but needs `SignalState::Analog(f64)`.
- [ ] **Live streaming**: Can a generator push data in real time? Requires append-only
  SignalSource and incremental tile invalidation.
- [ ] **Collaboration**: Multiple users viewing/annotating the same capture?
  The WebSocket model supports it but needs auth + conflict resolution.
