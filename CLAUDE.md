# sig-int — Claude Code Context

## Project

sig-int (Signals Intelligence) — a hierarchical signal description language with a
zoomable canvas browser backed by a tile server. Not a WaveDrom viewer — WaveDrom is
an output format. The `SignalSource` trait is the core abstraction.

Rust full-stack: Yew/WASM frontend + Axum backend + shared trait crate.
Canvas-based rendering, tree-sitter parsing, semantic zoom, dark theme.

## Key Documents

- `docs/VISION.md` — Use cases, motivation, language primitives, temporal model, LOD rendering
- `docs/TRAIT_SKETCH.md` — `SignalSource` trait design, `SignalView` enum, LOD types, wire protocol, impl sketches
- `docs/PLAN.md` — Implementation phases, architecture diagram, project structure, dependencies, design decisions

## Architecture Summary

- **Frontend** (Yew/WASM): Canvas renderer, editor (textarea → tree-sitter), viewport state, tile cache, WebSocket client
- **Backend** (Axum): Tile server, SignalSource registry, LOD engine, DSL compiler, VCD/WaveJSON ingest, embedded frontend assets
- **Shared**: `SignalSource` trait, `SignalView` enum (Block/Envelope/Waveform), wire protocol (`ViewRequest`/`ViewResponse`), temporal types
- **No database** — backend is a stateful tile server, not a CRUD app

## Core Abstraction

`SignalSource` trait with `view(query) -> SignalView`:
- `Block` — zoomed way out: labeled rectangle + port events
- `Envelope` — mid zoom: activity bands (min/max/density)
- `Waveform` — zoomed in: individual transitions (WaveDrom-compatible)

LOD thresholds are per-source (a 1.5GHz MIPI link and 100kHz I2C bus coexist with different thresholds).
Backend computes tiles on demand; frontend never holds the full dataset.

## Reference Repos

- **rust-skeleton**: https://github.com/meawoppl/meawoppl-rust-skeleton — Project template (Yew + Axum + Trunk + ws-bridge)
- **WaveDrom**: https://github.com/wavedrom/wavedrom — Original rendering engine
- **WaveJSON spec**: https://github.com/wavedrom/schema/blob/master/WaveJSON.md — Format specification
- **tree-sitter-vcd**: https://github.com/wavedrom/tree-sitter-vcd — VCD grammar for tree-sitter
- **WaveDrom tutorial**: https://wavedrom.com/tutorial.html — Signal types, period, phase, hscale, edges

## Tech Stack

- **Frontend**: Yew 0.21 (CSR) + Trunk + web-sys Canvas2D
- **Backend**: Axum 0.7 + tokio + ws-bridge + rust-embed
- **Shared**: serde + ws-bridge endpoint types
- **Parsing**: tree-sitter on frontend (highlighting), serde_json on backend (Phase 1), custom DSL compiler (Phase 4)
- **Language**: Rust, frontend compiled to wasm32-unknown-unknown
- **No database**: no diesel, no postgres

## Format Notes

- WaveDrom's WaveJSON is the baseline input format; sig-int extends it with additional keys
- Unknown keys in WaveJSON are silently ignored by WaveDrom — this is the extension mechanism
- WaveDrom has no absolute time model — cycles are character indices in the wave string
- sig-int has a hierarchical temporal model: time is a tree of coordinate systems, not a flat array
- "Export as WaveDrom" flattens any visible viewport window to standard WaveJSON
