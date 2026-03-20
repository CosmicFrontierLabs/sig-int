pub mod renderer;
pub mod viewport;

use std::collections::{HashMap, HashSet};

use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;
use yew::prelude::*;

use shared::temporal::TimeRange;
use shared::types::{SignalView, SourceManifest};

use self::viewport::Viewport;

/// Newtype wrapper so we can derive PartialEq for props
#[derive(Clone)]
pub struct TileMap(pub HashMap<String, SignalView>);

impl PartialEq for TileMap {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}

#[derive(Properties, PartialEq)]
pub struct CanvasProps {
    pub manifest: Option<SourceManifest>,
    pub tiles: TileMap,
    pub on_viewport_change: Callback<Viewport>,
    #[prop_or_default]
    pub fit_time_range: Option<TimeRange>,
}

pub struct WaveCanvas {
    canvas_ref: NodeRef,
    viewport: Viewport,
    render_viewport: Viewport,
    dragging: bool,
    last_mouse_x: f64,
    last_mouse_y: f64,
    fitted: bool,
    needs_fetch: bool,
    /// Collapsed paths — groups or signals that are minimized to a thin line
    collapsed: HashSet<String>,
}

pub enum CanvasMsg {
    Wheel {
        delta_y: f64,
        client_x: f64,
    },
    MouseDown {
        client_x: f64,
        client_y: f64,
    },
    MouseMove {
        client_x: f64,
        client_y: f64,
    },
    MouseUp,
    /// Click at canvas-relative position — check if it hit a label
    Click {
        client_x: f64,
        client_y: f64,
    },
}

const PREFETCH_MARGIN: f64 = 1.0;
const REFETCH_THRESHOLD: f64 = 0.5;

impl Component for WaveCanvas {
    type Message = CanvasMsg;
    type Properties = CanvasProps;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            canvas_ref: NodeRef::default(),
            viewport: Viewport::default(),
            render_viewport: Viewport::default(),
            dragging: false,
            last_mouse_x: 0.0,
            last_mouse_y: 0.0,
            fitted: false,
            needs_fetch: false,
            collapsed: HashSet::new(),
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, _old_props: &Self::Properties) -> bool {
        if !self.fitted {
            if let Some(ref range) = ctx.props().fit_time_range {
                self.viewport.fit_time_range(range);
                self.render_viewport = self.viewport.clone();
                self.fitted = true;
                self.emit_wide_viewport(ctx);
            }
        }
        true
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            CanvasMsg::Wheel { delta_y, client_x } => {
                let canvas: HtmlCanvasElement = self.canvas_ref.cast().unwrap();
                let rect = canvas.get_bounding_client_rect();
                let label_width = viewport::LABEL_WIDTH;
                let time_area_left = rect.left() + label_width;
                let time_area_width = rect.width() - label_width;
                let x_frac = ((client_x - time_area_left) / time_area_width).clamp(0.0, 1.0);

                let clamped = delta_y.abs().min(100.0);
                let factor = 1.0 + clamped * 0.003;

                if delta_y < 0.0 {
                    self.viewport.zoom_in(x_frac, factor);
                } else {
                    self.viewport.zoom_out(x_frac, factor);
                }

                self.emit_wide_viewport(ctx);
                true
            }
            CanvasMsg::MouseDown { client_x, client_y } => {
                self.dragging = true;
                self.last_mouse_x = client_x;
                self.last_mouse_y = client_y;
                false
            }
            CanvasMsg::MouseMove { client_x, client_y } => {
                if self.dragging {
                    let canvas: HtmlCanvasElement = self.canvas_ref.cast().unwrap();
                    let rect = canvas.get_bounding_client_rect();
                    let signal_area_w = rect.width() - viewport::LABEL_WIDTH;
                    let dx_frac = (client_x - self.last_mouse_x) / signal_area_w.max(1.0);
                    let dy = client_y - self.last_mouse_y;

                    self.viewport.pan(dx_frac, dy);
                    self.last_mouse_x = client_x;
                    self.last_mouse_y = client_y;

                    if self.pan_exceeds_threshold() {
                        self.emit_wide_viewport(ctx);
                    }
                    true
                } else {
                    false
                }
            }
            CanvasMsg::MouseUp => {
                if self.dragging {
                    self.dragging = false;
                    if self.needs_fetch {
                        self.emit_wide_viewport(ctx);
                        self.needs_fetch = false;
                    }
                    true
                } else {
                    false
                }
            }
            CanvasMsg::Click { client_x, client_y } => {
                let canvas: HtmlCanvasElement = self.canvas_ref.cast().unwrap();
                let rect = canvas.get_bounding_client_rect();
                let x = client_x - rect.left();
                let y = client_y - rect.top();

                // Only handle clicks in the label area
                if x > viewport::LABEL_WIDTH {
                    return false;
                }

                // Find which row was clicked using the layout
                let hit = renderer::hit_test_label(
                    y,
                    &self.viewport,
                    &ctx.props().tiles.0,
                    &self.collapsed,
                );

                if let Some(path) = hit {
                    if self.collapsed.contains(&path) {
                        self.collapsed.remove(&path);
                    } else {
                        self.collapsed.insert(path);
                    }
                    true
                } else {
                    false
                }
            }
        }
    }

    fn rendered(&mut self, ctx: &Context<Self>, _first_render: bool) {
        let Some(canvas) = self.canvas_ref.cast::<HtmlCanvasElement>() else {
            return;
        };

        let rect = canvas.get_bounding_client_rect();
        let dpr = web_sys::window().unwrap().device_pixel_ratio();
        let w = (rect.width() * dpr) as u32;
        let h = (rect.height() * dpr) as u32;

        if canvas.width() != w || canvas.height() != h {
            canvas.set_width(w);
            canvas.set_height(h);
        }

        self.viewport.canvas_width = rect.width();
        self.viewport.canvas_height = rect.height();

        let ctx2d = canvas
            .get_context("2d")
            .unwrap()
            .unwrap()
            .dyn_into::<web_sys::CanvasRenderingContext2d>()
            .unwrap();

        ctx2d.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0).unwrap();

        renderer::draw(
            &ctx2d,
            &self.viewport,
            &ctx.props().tiles.0,
            ctx.props().manifest.as_ref(),
            &self.collapsed,
        );
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let on_wheel = ctx.link().callback(|e: WheelEvent| {
            e.prevent_default();
            CanvasMsg::Wheel {
                delta_y: e.delta_y(),
                client_x: e.client_x() as f64,
            }
        });

        let on_mousedown = ctx.link().callback(|e: MouseEvent| {
            e.prevent_default();
            CanvasMsg::MouseDown {
                client_x: e.client_x() as f64,
                client_y: e.client_y() as f64,
            }
        });

        let on_mousemove = ctx.link().callback(|e: MouseEvent| CanvasMsg::MouseMove {
            client_x: e.client_x() as f64,
            client_y: e.client_y() as f64,
        });

        let on_mouseup = ctx.link().callback(|_: MouseEvent| CanvasMsg::MouseUp);
        let on_mouseleave = ctx.link().callback(|_: MouseEvent| CanvasMsg::MouseUp);

        let on_click = ctx.link().callback(|e: MouseEvent| CanvasMsg::Click {
            client_x: e.client_x() as f64,
            client_y: e.client_y() as f64,
        });

        html! {
            <div class="canvas-pane">
                <canvas
                    ref={self.canvas_ref.clone()}
                    onwheel={on_wheel}
                    onmousedown={on_mousedown}
                    onmousemove={on_mousemove}
                    onmouseup={on_mouseup}
                    onmouseleave={on_mouseleave}
                    onclick={on_click}
                />
            </div>
        }
    }
}

impl WaveCanvas {
    fn emit_wide_viewport(&mut self, ctx: &Context<Self>) {
        self.render_viewport = self.viewport.clone();
        let wide = self.viewport.with_margin(PREFETCH_MARGIN);
        ctx.props().on_viewport_change.emit(wide);
        self.needs_fetch = false;
    }

    fn pan_exceeds_threshold(&self) -> bool {
        let vp_duration = self.viewport.time_range.duration_ns() as f64;
        let render_center = self.render_viewport.time_range.start_ns as f64
            + self.render_viewport.time_range.duration_ns() as f64 / 2.0;
        let view_center = self.viewport.time_range.start_ns as f64 + vp_duration / 2.0;
        let drift = (view_center - render_center).abs();
        drift > vp_duration * REFETCH_THRESHOLD
    }
}
