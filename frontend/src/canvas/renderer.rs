use std::collections::{HashMap, HashSet};

use web_sys::CanvasRenderingContext2d;

use shared::types::*;

use super::viewport::Viewport;

// ---------------------------------------------------------------------------
// Theme constants
// ---------------------------------------------------------------------------
const BG_PRIMARY: &str = "#0d1117";
const BG_ALT: &str = "#161b22";
const GRID_COLOR: &str = "#21262d";
const TEXT_PRIMARY: &str = "#c9d1d9";
const TEXT_DIM: &str = "#8b949e";
const SIGNAL_HIGH: &str = "#3fb950";
const SIGNAL_LOW: &str = "#3fb950";
const SIGNAL_Z: &str = "#8b949e";
const SIGNAL_X: &str = "#f0883e";
const LABEL_BG: &str = "#1c2128";

const ROW_HEIGHT: f64 = 36.0;
const COLLAPSED_HEIGHT: f64 = 6.0;
const SIGNAL_HEIGHT: f64 = 22.0;
const HEADER_HEIGHT: f64 = 28.0;
const GROUP_LABEL_INDENT: f64 = 8.0;
const SIGNAL_LABEL_INDENT: f64 = 20.0;

use super::viewport::LABEL_WIDTH;

const DATA_COLORS: [&str; 5] = ["#58a6ff", "#3fb950", "#d2a8ff", "#f0883e", "#f778ba"];

// ---------------------------------------------------------------------------
// Layout: compute row positions accounting for collapse state
// ---------------------------------------------------------------------------

struct LayoutRow {
    y: f64,
    height: f64,
    kind: RowKind,
}

enum RowKind {
    GroupHeader {
        path: String,
        name: String,
        color: String,
        collapsed: bool,
    },
    Signal {
        path: String,
        name: String,
        signal_idx: usize,
        collapsed: bool,
    },
}

/// Build layout rows from tiles + collapse state.
/// Returns the rows and the total height consumed.
fn build_layout(
    tiles: &HashMap<String, SignalView>,
    collapsed: &HashSet<String>,
) -> Vec<LayoutRow> {
    let mut rows = Vec::new();
    let mut y = HEADER_HEIGHT;

    for (path, view) in tiles {
        let SignalView::Waveform {
            signals, overlay, ..
        } = view;

        // Group header row
        let group_collapsed = collapsed.contains(path);
        let group_color = overlay
            .as_ref()
            .map(|o| o.color.clone())
            .unwrap_or_else(|| "#30363d".to_string());
        let group_name = overlay
            .as_ref()
            .map(|o| o.name.clone())
            .unwrap_or_else(|| path.clone());

        rows.push(LayoutRow {
            y,
            height: if group_collapsed {
                COLLAPSED_HEIGHT
            } else {
                ROW_HEIGHT * 0.6
            },
            kind: RowKind::GroupHeader {
                path: path.clone(),
                name: group_name,
                color: group_color,
                collapsed: group_collapsed,
            },
        });

        if group_collapsed {
            y += COLLAPSED_HEIGHT;
            continue;
        }
        y += ROW_HEIGHT * 0.6;

        // Signal rows
        for (i, signal) in signals.iter().enumerate() {
            let sig_path = format!("{}.{}", path, signal.name);
            let sig_collapsed = collapsed.contains(&sig_path);

            let h = if sig_collapsed {
                COLLAPSED_HEIGHT
            } else {
                ROW_HEIGHT
            };
            rows.push(LayoutRow {
                y,
                height: h,
                kind: RowKind::Signal {
                    path: sig_path,
                    name: signal.name.clone(),
                    signal_idx: i,
                    collapsed: sig_collapsed,
                },
            });
            y += h;
        }
    }

    rows
}

// ---------------------------------------------------------------------------
// Hit test: which label path was clicked at a given y?
// ---------------------------------------------------------------------------

pub fn hit_test_label(
    click_y: f64,
    _viewport: &Viewport,
    tiles: &HashMap<String, SignalView>,
    collapsed: &HashSet<String>,
) -> Option<String> {
    let rows = build_layout(tiles, collapsed);
    for row in &rows {
        if click_y >= row.y && click_y < row.y + row.height {
            return Some(match &row.kind {
                RowKind::GroupHeader { path, .. } => path.clone(),
                RowKind::Signal { path, .. } => path.clone(),
            });
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Main draw entry point
// ---------------------------------------------------------------------------

pub fn draw(
    ctx: &CanvasRenderingContext2d,
    viewport: &Viewport,
    tiles: &HashMap<String, SignalView>,
    manifest: Option<&SourceManifest>,
    collapsed: &HashSet<String>,
) {
    let w = viewport.canvas_width;
    let h = viewport.canvas_height;

    ctx.set_fill_style_str(BG_PRIMARY);
    ctx.fill_rect(0.0, 0.0, w, h);

    draw_time_grid(ctx, viewport);

    let Some(_manifest) = manifest else {
        ctx.set_fill_style_str(TEXT_DIM);
        ctx.set_font("14px 'JetBrains Mono', monospace");
        ctx.set_text_align("center");
        let _ = ctx.fill_text("Connecting...", w / 2.0, h / 2.0);
        return;
    };

    let rows = build_layout(tiles, collapsed);

    if rows.is_empty() {
        ctx.set_fill_style_str(TEXT_DIM);
        ctx.set_font("13px 'JetBrains Mono', monospace");
        ctx.set_text_align("center");
        let tile_info = if tiles.is_empty() {
            "Waiting for data...".to_string()
        } else {
            format!("Tiles: {} (not drawn — check viewport)", tiles.len())
        };
        let _ = ctx.fill_text(&tile_info, w / 2.0, h / 2.0);
    }

    // Collect group spans for overlay drawing
    struct GroupSpan {
        y: f64,
        height: f64,
        path: String,
    }
    let mut group_spans: Vec<GroupSpan> = Vec::new();
    let mut current_group_path: Option<String> = None;
    let mut current_group_y = 0.0;

    for row in &rows {
        match &row.kind {
            RowKind::GroupHeader {
                path,
                name,
                color,
                collapsed: is_collapsed,
            } => {
                // Close previous group span
                if let Some(prev_path) = current_group_path.take() {
                    let end_y = row.y;
                    group_spans.push(GroupSpan {
                        y: current_group_y,
                        height: end_y - current_group_y,
                        path: prev_path,
                    });
                }
                current_group_path = Some(path.clone());
                current_group_y = row.y;

                if row.y + row.height < 0.0 || row.y > h {
                    continue;
                }

                if *is_collapsed {
                    // Thin colored line for collapsed group
                    draw_collapsed_indicator(
                        ctx,
                        row.y,
                        COLLAPSED_HEIGHT,
                        color,
                        name,
                        GROUP_LABEL_INDENT,
                    );
                } else {
                    // Group header label
                    draw_group_label(ctx, row.y, row.height, color, name);
                }
            }
            RowKind::Signal {
                path: _,
                name,
                signal_idx,
                collapsed: is_collapsed,
            } => {
                if row.y + row.height < 0.0 || row.y > h {
                    continue;
                }

                if *is_collapsed {
                    draw_collapsed_indicator(
                        ctx,
                        row.y,
                        COLLAPSED_HEIGHT,
                        TEXT_DIM,
                        name,
                        SIGNAL_LABEL_INDENT,
                    );
                } else {
                    // Find the tile and signal data
                    // We need to find which tile this signal belongs to
                    if let Some(group_path) = &current_group_path {
                        if let Some(SignalView::Waveform {
                            signals,
                            detail_alpha,
                            ..
                        }) = tiles.get(group_path.as_str())
                        {
                            if let Some(signal) = signals.get(*signal_idx) {
                                draw_signal_row(ctx, viewport, signal, row.y, *detail_alpha);
                                // Draw signal label in the gutter
                                draw_signal_label(ctx, row.y, ROW_HEIGHT, name, *detail_alpha);
                            }
                        }
                    }
                }
            }
        }
    }

    // Close last group span
    if let Some(prev_path) = current_group_path.take() {
        let end_y = rows
            .last()
            .map(|r| r.y + r.height)
            .unwrap_or(current_group_y);
        group_spans.push(GroupSpan {
            y: current_group_y,
            height: end_y - current_group_y,
            path: prev_path,
        });
    }

    // Draw group overlays and borders
    for span in &group_spans {
        if let Some(SignalView::Waveform {
            overlay,
            detail_alpha,
            ..
        }) = tiles.get(&span.path)
        {
            if let Some(block) = overlay {
                let overlay_alpha = 1.0 - detail_alpha;
                if overlay_alpha > 0.001 {
                    draw_block_overlay(ctx, viewport, block, span.y, span.height, overlay_alpha);
                }
                draw_group_border(ctx, viewport, block, span.y, span.height);
            }
        }
    }

    // Label column background (draw over any signal overflow)
    ctx.set_fill_style_str(BG_PRIMARY);
    ctx.set_global_alpha(0.85);
    ctx.fill_rect(0.0, HEADER_HEIGHT, LABEL_WIDTH - 2.0, h - HEADER_HEIGHT);
    ctx.set_global_alpha(1.0);

    // Re-draw labels on top of the background
    for row in &rows {
        if row.y + row.height < 0.0 || row.y > h {
            continue;
        }
        match &row.kind {
            RowKind::GroupHeader {
                name,
                color,
                collapsed: is_collapsed,
                ..
            } => {
                if *is_collapsed {
                    draw_collapsed_indicator(
                        ctx,
                        row.y,
                        COLLAPSED_HEIGHT,
                        color,
                        name,
                        GROUP_LABEL_INDENT,
                    );
                } else {
                    draw_group_label(ctx, row.y, row.height, color, name);
                }
            }
            RowKind::Signal {
                name,
                collapsed: is_collapsed,
                signal_idx,
                ..
            } => {
                if *is_collapsed {
                    draw_collapsed_indicator(
                        ctx,
                        row.y,
                        COLLAPSED_HEIGHT,
                        TEXT_DIM,
                        name,
                        SIGNAL_LABEL_INDENT,
                    );
                } else {
                    if let Some(group_path) = find_group_for_row(row, &rows) {
                        if let Some(SignalView::Waveform {
                            signals,
                            detail_alpha,
                            ..
                        }) = tiles.get(&group_path)
                        {
                            if let Some(_signal) = signals.get(*signal_idx) {
                                draw_signal_label(ctx, row.y, ROW_HEIGHT, name, *detail_alpha);
                            }
                        }
                    }
                }
            }
        }
    }

    draw_status_overlay(ctx, viewport);
}

/// Find the group path that contains a signal row
fn find_group_for_row(target: &LayoutRow, rows: &[LayoutRow]) -> Option<String> {
    let mut last_group: Option<String> = None;
    for row in rows {
        if std::ptr::eq(row, target) {
            return last_group;
        }
        if let RowKind::GroupHeader { path, .. } = &row.kind {
            last_group = Some(path.clone());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Label drawing helpers
// ---------------------------------------------------------------------------

fn draw_group_label(ctx: &CanvasRenderingContext2d, y: f64, height: f64, color: &str, name: &str) {
    // Colored accent bar on the left
    ctx.set_fill_style_str(color);
    ctx.set_global_alpha(0.6);
    ctx.fill_rect(2.0, y + 2.0, 3.0, height - 4.0);
    ctx.set_global_alpha(1.0);

    // Group name
    ctx.set_fill_style_str(color);
    ctx.set_font("11px 'JetBrains Mono', monospace");
    ctx.set_text_align("left");
    ctx.set_text_baseline("middle");
    let _ = ctx.fill_text(name, GROUP_LABEL_INDENT, y + height / 2.0);
}

fn draw_signal_label(
    ctx: &CanvasRenderingContext2d,
    y: f64,
    height: f64,
    name: &str,
    detail_alpha: f64,
) {
    ctx.save();
    ctx.set_global_alpha(detail_alpha.max(0.3)); // always somewhat visible
    ctx.set_fill_style_str(TEXT_DIM);
    ctx.set_font("11px 'JetBrains Mono', monospace");
    ctx.set_text_align("left");
    ctx.set_text_baseline("middle");
    let _ = ctx.fill_text(name, SIGNAL_LABEL_INDENT, y + height / 2.0);
    ctx.restore();
}

fn draw_collapsed_indicator(
    ctx: &CanvasRenderingContext2d,
    y: f64,
    height: f64,
    color: &str,
    name: &str,
    indent: f64,
) {
    // Thin colored line
    ctx.save();
    ctx.set_global_alpha(0.5);
    ctx.set_stroke_style_str(color);
    ctx.set_line_width(1.0);
    ctx.begin_path();
    ctx.move_to(indent, y + height / 2.0);
    ctx.line_to(LABEL_WIDTH - 4.0, y + height / 2.0);
    ctx.stroke();

    // Tiny label
    ctx.set_global_alpha(0.4);
    ctx.set_fill_style_str(color);
    ctx.set_font("8px 'JetBrains Mono', monospace");
    ctx.set_text_align("left");
    ctx.set_text_baseline("middle");
    let _ = ctx.fill_text(name, indent + 2.0, y + height / 2.0 - 0.5);
    ctx.restore();
}

// ---------------------------------------------------------------------------
// Time grid
// ---------------------------------------------------------------------------

fn draw_time_grid(ctx: &CanvasRenderingContext2d, vp: &Viewport) {
    let duration = vp.time_range.duration_ns() as f64;
    if duration <= 0.0 {
        return;
    }

    let signal_area = vp.canvas_width - LABEL_WIDTH;
    let target_px_spacing = 120.0;
    let target_ns = (duration / signal_area) * target_px_spacing;
    let interval = nice_interval(target_ns);

    let start = (vp.time_range.start_ns as f64 / interval).floor() as u64 * interval as u64;

    ctx.set_stroke_style_str(GRID_COLOR);
    ctx.set_line_width(1.0);
    ctx.set_fill_style_str(TEXT_DIM);
    ctx.set_font("10px 'JetBrains Mono', monospace");
    ctx.set_text_align("center");

    let mut t = start;
    while t <= vp.time_range.end_ns {
        let x = vp.time_to_x(t);
        if x >= LABEL_WIDTH && x <= vp.canvas_width {
            ctx.begin_path();
            ctx.move_to(x, HEADER_HEIGHT);
            ctx.line_to(x, vp.canvas_height);
            ctx.stroke();

            let label = format_time(t);
            let _ = ctx.fill_text(&label, x, HEADER_HEIGHT - 6.0);
        }
        t += interval as u64;
    }
}

fn nice_interval(target_ns: f64) -> f64 {
    let exponent = target_ns.log10().floor();
    let base = 10.0_f64.powf(exponent);
    let normalized = target_ns / base;
    let nice = if normalized <= 1.5 {
        1.0
    } else if normalized <= 3.5 {
        2.0
    } else if normalized <= 7.5 {
        5.0
    } else {
        10.0
    };
    nice * base
}

fn format_time(ns: u64) -> String {
    if ns >= 1_000_000_000 {
        format!("{:.1}s", ns as f64 / 1_000_000_000.0)
    } else if ns >= 1_000_000 {
        format!("{:.1}ms", ns as f64 / 1_000_000.0)
    } else if ns >= 1_000 {
        format!("{:.1}μs", ns as f64 / 1_000.0)
    } else {
        format!("{}ns", ns)
    }
}

// ---------------------------------------------------------------------------
// Signal waveform row (draws only the waveform, not the label)
// ---------------------------------------------------------------------------

fn draw_signal_row(
    ctx: &CanvasRenderingContext2d,
    vp: &Viewport,
    signal: &SignalWaveform,
    y: f64,
    alpha: f64,
) {
    ctx.save();
    ctx.set_global_alpha(alpha);

    let sig_top = y + (ROW_HEIGHT - SIGNAL_HEIGHT) / 2.0;
    let sig_bottom = sig_top + SIGNAL_HEIGHT;
    let sig_mid = sig_top + SIGNAL_HEIGHT / 2.0;

    if signal.transitions.is_empty() {
        ctx.restore();
        return;
    }

    ctx.set_line_width(1.5);
    let signal_x_start = LABEL_WIDTH;

    for i in 0..signal.transitions.len() {
        let t = &signal.transitions[i];
        let x = vp.time_to_x(t.time_ns).max(signal_x_start);

        let next_x = if i + 1 < signal.transitions.len() {
            vp.time_to_x(signal.transitions[i + 1].time_ns)
        } else {
            vp.canvas_width
        }
        .min(vp.canvas_width);

        if next_x < signal_x_start || x > vp.canvas_width {
            continue;
        }

        let draw_x = x.max(signal_x_start);
        let draw_next_x = next_x;

        if draw_next_x <= draw_x {
            continue;
        }

        let prev_state = if i > 0 {
            Some(&signal.transitions[i - 1].state)
        } else {
            None
        };

        match &t.state {
            SignalState::High => {
                ctx.set_stroke_style_str(SIGNAL_HIGH);
                ctx.begin_path();
                ctx.move_to(draw_x, sig_top);
                ctx.line_to(draw_next_x, sig_top);
                ctx.stroke();
                if let Some(prev) = prev_state {
                    if !matches!(prev, SignalState::High) {
                        ctx.begin_path();
                        ctx.move_to(draw_x, sig_bottom);
                        ctx.line_to(draw_x, sig_top);
                        ctx.stroke();
                    }
                }
            }
            SignalState::Low => {
                ctx.set_stroke_style_str(SIGNAL_LOW);
                ctx.begin_path();
                ctx.move_to(draw_x, sig_bottom);
                ctx.line_to(draw_next_x, sig_bottom);
                ctx.stroke();
                if let Some(prev) = prev_state {
                    if !matches!(prev, SignalState::Low) {
                        ctx.begin_path();
                        ctx.move_to(draw_x, sig_top);
                        ctx.line_to(draw_x, sig_bottom);
                        ctx.stroke();
                    }
                }
            }
            SignalState::HighZ => {
                ctx.set_stroke_style_str(SIGNAL_Z);
                ctx.set_line_dash(&js_sys::Array::of2(&4.0.into(), &3.0.into()))
                    .unwrap();
                ctx.begin_path();
                ctx.move_to(draw_x, sig_mid);
                ctx.line_to(draw_next_x, sig_mid);
                ctx.stroke();
                ctx.set_line_dash(&js_sys::Array::new()).unwrap();
            }
            SignalState::Undefined => {
                ctx.set_fill_style_str(SIGNAL_X);
                ctx.set_global_alpha(0.2);
                ctx.fill_rect(draw_x, sig_top, draw_next_x - draw_x, SIGNAL_HEIGHT);
                ctx.set_global_alpha(alpha);
                ctx.set_stroke_style_str(SIGNAL_X);
                ctx.begin_path();
                ctx.move_to(draw_x, sig_top);
                ctx.line_to(draw_next_x, sig_top);
                ctx.move_to(draw_x, sig_bottom);
                ctx.line_to(draw_next_x, sig_bottom);
                ctx.stroke();
            }
            SignalState::Data(label) => {
                let hash = label
                    .bytes()
                    .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
                let color = DATA_COLORS[hash as usize % DATA_COLORS.len()];

                ctx.set_fill_style_str(color);
                ctx.set_global_alpha(alpha * 0.15);
                ctx.fill_rect(draw_x, sig_top, draw_next_x - draw_x, SIGNAL_HEIGHT);
                ctx.set_global_alpha(alpha);

                ctx.set_stroke_style_str(color);
                ctx.set_line_width(1.0);
                ctx.begin_path();
                let slope = 4.0_f64.min((draw_next_x - draw_x) / 4.0);
                ctx.move_to(draw_x, sig_mid);
                ctx.line_to(draw_x + slope, sig_top);
                ctx.line_to(draw_next_x - slope, sig_top);
                ctx.line_to(draw_next_x, sig_mid);
                ctx.line_to(draw_next_x - slope, sig_bottom);
                ctx.line_to(draw_x + slope, sig_bottom);
                ctx.close_path();
                ctx.stroke();

                let text_width = draw_next_x - draw_x;
                if text_width > 20.0 {
                    ctx.set_fill_style_str(TEXT_PRIMARY);
                    ctx.set_font("10px 'JetBrains Mono', monospace");
                    ctx.set_text_align("center");
                    let _ = ctx.fill_text(label, (draw_x + draw_next_x) / 2.0, sig_mid + 3.5);
                }

                ctx.set_line_width(1.5);
            }
        }
    }

    ctx.restore();
}

// ---------------------------------------------------------------------------
// Block overlay — eclipses signal detail as you zoom out
// ---------------------------------------------------------------------------

fn draw_block_overlay(
    ctx: &CanvasRenderingContext2d,
    vp: &Viewport,
    block: &BlockSummary,
    y: f64,
    height: f64,
    alpha: f64,
) {
    let raw_x_start = vp.time_to_x(block.start_ns);
    let raw_x_end = vp.time_to_x(block.end_ns);

    if raw_x_end < LABEL_WIDTH || raw_x_start > vp.canvas_width {
        return;
    }

    let x_start = raw_x_start.max(LABEL_WIDTH);
    let x_end = raw_x_end
        .min(vp.canvas_width)
        .max(x_start + 60.0)
        .min(vp.canvas_width);

    let block_top = y + 2.0;
    let block_height = height - 4.0;

    ctx.save();

    ctx.set_fill_style_str(&block.color);
    ctx.set_global_alpha(alpha * 0.35);
    ctx.fill_rect(x_start, block_top, x_end - x_start, block_height);

    ctx.set_global_alpha(alpha);
    ctx.set_stroke_style_str(&block.color);
    ctx.set_line_width(2.0);
    ctx.begin_path();
    ctx.rect(x_start, block_top, x_end - x_start, block_height);
    ctx.stroke();

    if alpha > 0.3 {
        let text_width = x_end - x_start;
        if text_width > 40.0 {
            ctx.set_fill_style_str(TEXT_PRIMARY);
            ctx.set_global_alpha(alpha);
            ctx.set_font("13px 'JetBrains Mono', monospace");
            ctx.set_text_align("center");
            ctx.set_text_baseline("middle");

            ctx.save();
            ctx.begin_path();
            ctx.rect(x_start, block_top, x_end - x_start, block_height);
            ctx.clip();
            let _ = ctx.fill_text(&block.label, (x_start + x_end) / 2.0, y + height / 2.0);
            ctx.restore();
        }
    }

    ctx.restore();
}

// ---------------------------------------------------------------------------
// Group border — always visible subtle outline
// ---------------------------------------------------------------------------

fn draw_group_border(
    ctx: &CanvasRenderingContext2d,
    vp: &Viewport,
    block: &BlockSummary,
    y: f64,
    height: f64,
) {
    let raw_x_start = vp.time_to_x(block.start_ns);
    let raw_x_end = vp.time_to_x(block.end_ns);

    if raw_x_end < LABEL_WIDTH || raw_x_start > vp.canvas_width {
        return;
    }

    let x_start = raw_x_start.max(LABEL_WIDTH);
    let x_end = raw_x_end
        .min(vp.canvas_width)
        .max(x_start + 60.0)
        .min(vp.canvas_width);

    ctx.save();
    ctx.set_global_alpha(0.25);
    ctx.set_stroke_style_str(&block.color);
    ctx.set_line_width(1.0);
    ctx.begin_path();
    ctx.rect(x_start, y + 1.0, x_end - x_start, height - 2.0);
    ctx.stroke();
    ctx.restore();
}

// ---------------------------------------------------------------------------
// Status overlay
// ---------------------------------------------------------------------------

fn draw_status_overlay(ctx: &CanvasRenderingContext2d, vp: &Viewport) {
    let res = vp.resolution_ns();
    let label = format!(
        "{}/px | {} - {}",
        format_time(res),
        format_time(vp.time_range.start_ns),
        format_time(vp.time_range.end_ns),
    );

    ctx.set_fill_style_str(LABEL_BG);
    ctx.set_global_alpha(0.85);
    ctx.fill_rect(
        vp.canvas_width - 300.0,
        vp.canvas_height - 24.0,
        300.0,
        24.0,
    );
    ctx.set_global_alpha(1.0);
    ctx.set_fill_style_str(TEXT_DIM);
    ctx.set_font("10px 'JetBrains Mono', monospace");
    ctx.set_text_align("right");
    let _ = ctx.fill_text(&label, vp.canvas_width - 8.0, vp.canvas_height - 8.0);
}
