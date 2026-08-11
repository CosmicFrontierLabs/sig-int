# sig-int: Vision & Use Cases

> sig-int is not a WaveDrom viewer. It's a **hierarchical signal description language**
> with a zoomable canvas browser backed by a tile server.
> WaveDrom is an output format. The `SignalSource` trait is the core abstraction.
>
> Think Google Maps for signals: zoom out to see the system, zoom in to see the bits.
>
> See also: `PLAN.md` (implementation phases), `TRAIT_SKETCH.md` (core trait design).

---

## 1. The Core Problem

Real systems span many orders of magnitude in time:

| Layer | Example | Timescale | Data volume |
|---|---|---|---|
| System events | Camera frame capture | 250ms | trivial |
| Protocol transactions | I2C register write | 100μs | small |
| Byte-level | SPI byte clock | 100ns | moderate |
| Bit-level | MIPI CSI-2 lane | <1ns | enormous |

No flat signal array can represent all of these at once. You'd need ~10^9 samples
to cover one camera frame at MIPI bit resolution. That's unloadable and unrenderable.

**What you actually want:**
- At the system level: "camera captured a frame, then MIPI transmitted it"
- Zoom into the MIPI block: "here are the packet boundaries and lane assignments"
- Zoom into a packet: "here are the individual bit transitions on D-PHY lane 0"

Each zoom level has **different data, different timescales, and different detail** —
not just the same data drawn smaller.

---

## 2. Use Cases

### Use Case 1: Camera System Overview

```
System (1 second window)
├── Camera
│   ├── exposure  ████░░░░████░░░░  (250ms high, 250ms low)
│   ├── vsync     │       │         (edge markers)
│   └── [MIPI TX] ░░░████░░░░████░  (block: "1.2Gbps, 1080p")
├── ISP
│   ├── busy      ░░░░███░░░░░███░
│   └── [I2C CFG] ·  ▪  ·  ▪  ·    (block: "sensor config writes")
└── Memory
    ├── [DMA WR]  ░░░░▓▓▓░░░░░▓▓▓  (block: "480MB/s burst")
    └── [DMA RD]  ░░░░░░▓░░░░░░▓░
```

**Zoomed out**: blocks with summary annotations (data rate, burst size, etc.)
**Zoomed into MIPI TX**:

```
MIPI TX (500μs window)
├── clk_p     ┌┐┌┐┌┐┌┐┌┐┌┐┌┐┌┐
├── d0_p      ──┐┌──┐┌┐┌──┐┌──┐
├── d1_p      ┐┌──┐┌──┐┌┐┌──┐┌─
├── [packet]  │  HDR  │   PAYLOAD   │  CRC  │
└── byte_cnt  x 0  1  2  3 ...  1920  1921 x
```

**Key insight**: The MIPI TX data doesn't exist at the system level. It's generated/loaded
on demand when you zoom into that block's time range.

### Use Case 2: I2C Between Devices

```
I2C Bus (10ms window)
├── SCL       ┌┐┌┐┌┐┌┐┌┐┌┐  (100kHz)
├── SDA       ─┐┌─┐┌┐┌─┐┌─
└── [decoded] │S│ 0x48 │A│ 0x0F │A│ 0x3C │A│P│
```

But zoomed out to 1 second:

```
System (1s window)
├── ISP.config_bus  ▪▪  ▪▪▪  ▪▪    (each dot = one I2C transaction)
│                   ^   ^    ^
│                   "write exposure"
│                   "write gain"
│                   "read status"
```

Transactions become **point events** at the system scale. The block annotation
tells you what each transaction *means*, not what bits were on the wire.

### Use Case 3: Cross-Module Causal Chains

```
System (1s)
  Camera.vsync ────────│──────────│────
                       ↓ (1.2ms)
  MIPI.frame_start ────│──────────│────
                       ↓ (8.3ms)
  ISP.done ────────────│──────────│────
                       ↓ (0.1ms)
  DMA.burst ──────────▓▓──────────▓▓──
                       ↓ (0.5ms)
  Display.vsync ───────│──────────│────
```

The arrows between blocks show **causal edges** with measured latencies.
These edges are first-class primitives, not WaveDrom-style visual annotations.
They represent "this signal's edge caused that block to start."

### Use Case 4: Protocol Analyzer View

You have a logic analyzer capture (VCD, sigrok, etc.) with millions of samples.
sig-int doesn't load it all — it builds a **level-of-detail pyramid**:

| Zoom level | What's shown | Data loaded |
|---|---|---|
| Full capture (10s) | Activity density heatmap per channel | Summary metadata only |
| 100ms window | Protocol transaction boundaries | Transaction index |
| 1ms window | Byte-level decode | Byte-level samples |
| 1μs window | Individual transitions | Raw samples for visible range |

---

## 3. Language Primitives

sig-int needs a description language where the primitives map to the zoom model.

### 3.1 Signals

The leaf-level data. Compatible with WaveDrom wave strings, but with explicit timescale.

```
signal clk: clock @ 100MHz
signal sda: wire
signal addr: bus[8] @ 100MHz
```

### 3.2 Blocks

A named, bounded region of signals with its own timescale. This is the **functional
encapsulation** unit. At a distance, a block is a single labeled rectangle.

```
block mipi_frame {
  timescale: 1ns
  duration: 8.3ms

  signal clk_p:  clock @ 1.5GHz
  signal d0:     wire
  signal d1:     wire

  // Block-level annotations visible when collapsed
  annotation data_rate = "1.2 Gbps"
  annotation pixel_count = "1920x1080"
}
```

### 3.3 Modules

A collection of signals and sub-blocks that represents a device or functional unit.
Modules define the **interface signals** visible at the edges when collapsed.

```
module camera {
  // Interface signals — visible even when module is collapsed
  port exposure: wire          // outward-facing signal
  port vsync: wire
  port mipi: block mipi_frame  // a block is also a port

  // Internal signals — only visible when zoomed in
  internal pixel_clk: clock @ 48MHz
  internal adc_data: bus[12]
}
```

### 3.4 Edges (Causal Connections)

First-class causal relationships between signal events across modules and timescales.

```
edge camera.vsync.rising -> mipi_frame.start
  delay: 1.2ms
  label: "frame readout begin"

edge mipi_frame.end -> isp.process.start
  delay: 50μs
  label: "ISP pipeline kick"
```

Edges are **not visual annotations** — they carry timing data and participate in
the temporal model. When you zoom out, edges between collapsed blocks become
the primary information: "camera triggers MIPI which triggers ISP."

### 3.5 Timelines

A timeline binds modules together at a shared temporal reference and defines
what's visible at each zoom level.

```
timeline system_overview {
  timescale: 1ms
  duration: 2s

  place camera
  place isp
  place memory
  place display

  // At this timescale, blocks appear as rectangles
  // Edges appear as arrows between them
}
```

### 3.6 Generators (Lazy Data)

For real captured data (VCD, logic analyzer), a generator produces signal data
on demand for a requested time range and resolution.

```
generator mipi_capture {
  source: "capture_2024_03.vcd"
  timescale: auto  // derived from VCD

  // sig-int calls this when the user zooms into the relevant time range
  // Only the visible window is loaded into memory
}
```

---

## 4. The Temporal Model

### 4.1 No Single Global Clock

There is **no flat sample array**. Instead, time is a tree:

```
System (root timeline)
├── camera.exposure     @ ms resolution
├── camera.mipi_frame   @ ns resolution (lazy, bounded)
│   ├── mipi.clk_p      @ sub-ns resolution (lazy, bounded)
│   └── mipi.d0         @ sub-ns resolution (lazy, bounded)
├── isp.busy            @ μs resolution
└── memory.dma_burst    @ μs resolution
```

Each node has:
- A **timescale** (units per sample)
- A **time offset** relative to its parent (when does this block start in the parent's time?)
- A **duration** (how long does this block last?)
- **Data**: either inline wave strings or a lazy generator

### 4.2 Temporal Addressing

Every point in the system has an address:

```
camera.mipi_frame[0].d0 @ 4.2μs
│       │          │  │     │
│       │          │  │     └─ time offset within the block instance
│       │          │  └─ signal within the block
│       │          └─ first instance of this block (it repeats per frame)
│       └─ block name
└─ module name
```

A **region** is a signal set + time range:

```
camera.mipi_frame[0].{d0,d1} @ 4.2μs:4.8μs
```

This is how external references (docs, test harnesses, other diagrams) point
into the hierarchy.

### 4.3 Block Instances

Blocks can repeat. A MIPI frame block occurs once per camera frame — potentially
30 times per second. Each instance has:
- An index: `mipi_frame[0]`, `mipi_frame[1]`, ...
- A start time in the parent's coordinate system
- Its own internal timeline

At the system zoom level, these appear as repeated rectangles:
```
camera.mipi: ░░▓▓▓░░░▓▓▓░░░▓▓▓░░░
              [0]    [1]    [2]
```

Zoom into `[1]` and you enter the block's internal timescale.

---

## 5. Rendering Model: Level-of-Detail

### 5.1 What Gets Drawn at Each Scale

The canvas renderer doesn't just scale the same data. It draws **fundamentally
different content** based on the viewport's temporal resolution:

| Viewport resolution | Rendering strategy |
|---|---|
| > 100ms/pixel | Module blocks with labels and edge arrows only |
| 1ms–100ms/pixel | Signal activity envelopes (min/max bands), transaction markers |
| 1μs–1ms/pixel | Signal state bands (high/low/data colored strips, no transitions) |
| 1ns–1μs/pixel | Full WaveDrom-style waveforms with transitions and data labels |
| < 1ns/pixel | Sub-cycle detail, rise/fall times, analog-ish rendering |

### 5.2 Data Loading Strategy

```
User zooms into region
  -> viewport.temporal_range = (t_start, t_end)
  -> viewport.resolution = (t_end - t_start) / canvas_width_px
  -> for each visible signal:
       if signal.data is inline:
         render directly (small data, always in memory)
       if signal.data is generator:
         request = (t_start, t_end, resolution)
         if cache has this range at sufficient resolution:
           render from cache
         else:
           load from generator (async)
           show "loading" placeholder
           render when data arrives
```

Cache is a **tile-based pyramid** (similar to map tiles):
- Tile = fixed time range at a fixed resolution
- Higher zoom levels = more tiles, finer resolution
- Tiles outside the viewport are evictable

### 5.3 Block Edge Signals

When a block is collapsed, its **port signals** are still visible as annotations
on the block boundary:

```
┌─────────── MIPI TX ──────────────┐
│  ▸ clk: 1.5GHz                  │  ← port annotations
│  ▸ data_rate: 1.2Gbps           │
│  ▸ d0: ────┐┌──┐┌┐──            │  ← miniature signal preview
╞══════════════════════════════════╡
│  ↓ frame_start    ↑ frame_end   │  ← edge connection points
└──────────────────────────────────┘
```

The edge connection points on the block boundary are how edges from other
modules visually attach. This is the "index into the edges of signal groups"
the user described.

---

## 6. Relationship to WaveDrom

WaveDrom is an **output format**, not the source of truth:

```
sig-int DSL / WaveJSON import
  → SignalSource tree (in backend memory)
  → tile server (serves LOD data over WebSocket)
  → canvas renderer (zoomable browser)
  → WaveDrom JSON export (flatten any visible window)
```

At any zoom level, you can "export as WaveDrom" and get a flat WaveJSON document
for that particular view. But the sig-int source document carries the hierarchy,
timescales, causal edges, and lazy data references that WaveDrom cannot represent.

Standard WaveDrom documents can be **imported** as a single flat block with
character-index timescale — full backwards compatibility.

---

## 7. The SignalSource Trait

The core abstraction is a Rust trait that every data source implements.
The trait's `view()` method returns different data depending on the requested
level of detail — this is what makes semantic zoom work.

See `TRAIT_SKETCH.md` for the complete design including:
- `SignalSource` trait definition
- `SignalView` enum: `Block`, `Envelope`, `Waveform`
- LOD thresholds (per-source, not global)
- Implementations: `InlineSource`, `GroupSource`, `VcdSource`, `ComputedSource`
- Wire protocol: `ViewRequest` / `ViewResponse` over WebSocket
- Tile caching strategy

Key property: **LOD is per-source**. A 1.5GHz MIPI link in the same diagram
as a 100kHz I2C bus — each has its own thresholds. The tile server calls
`source.view(query)` and the source decides what to return based on the
requested resolution vs. its own native resolution.

---

## 8. Backend Architecture

The backend is a **stateful tile server**, not a CRUD API:

```
Frontend viewport change
  → ViewRequest::Query { signals, time_range, resolution }
  → Backend resolves signal paths (glob matching)
  → For each matched SignalSource: call view(query)
  → SignalView serialized as ViewResponse::Tile
  → Sent to frontend over WebSocket
  → Frontend caches tile + draws to canvas
```

The backend also handles:
- DSL compilation (source text → SignalSource tree)
- WaveJSON import (JSON → InlineSource + GroupSource)
- VCD indexing and lazy sample loading
- Computed signal evaluation

This architecture means the frontend never holds the full dataset.
A 10GB VCD file lives on the backend; the frontend only ever has
the tiles visible in the current viewport (~kilobytes).

---

## 9. Open Design Questions

- [ ] **DSL syntax**: Text-based (as sketched in §3)? Or structured format?
  Leaning text-based — more natural for signal/block/edge declarations.
- [ ] **Generator interface**: How do external data sources plug in?
  Options: compiled-in trait impls, WASM plugin modules, HTTP endpoints.
- [ ] **Analog signals**: Support float-valued waveforms?
  Rendering model handles it (envelope at low zoom, trace at high zoom)
  but needs `SignalState::Analog(f64)`.
- [ ] **Live streaming**: Can a source push data in real time?
  Needs append-only SignalSource + incremental tile invalidation.
- [ ] **Collaboration**: Multiple users on the same capture?
  WebSocket model supports it but needs auth + conflict resolution.
