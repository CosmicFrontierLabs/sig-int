use std::collections::HashMap;

use futures::channel::mpsc;
use futures::SinkExt;
use futures::StreamExt;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use shared::protocol::{ClientMsg, ServerMsg, SigIntSocket};
use shared::types::{SignalView, SourceManifest};

use crate::canvas::viewport::Viewport;
use crate::canvas::{TileMap, WaveCanvas};
use crate::editor::Editor;

pub struct App {
    manifest: Option<SourceManifest>,
    tiles: HashMap<String, SignalView>,
    status: String,
    source_text: String,
    /// Channel sender for outgoing WS messages — never borrows anything
    cmd_tx: mpsc::UnboundedSender<ClientMsg>,
    query_generation: u64,
    tiles_generation: u64,
}

pub enum AppMsg {
    Connected,
    ServerMsg(ServerMsg),
    WsError(String),
    ViewportChanged(Viewport),
    SourceChanged(String),
}

impl Component for App {
    type Message = AppMsg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded::<ClientMsg>();

        let link = ctx.link().clone();
        spawn_local(async move {
            match ws_bridge::yew_client::connect::<SigIntSocket>() {
                Ok(conn) => {
                    let (sender, mut receiver) = conn.split();
                    link.send_message(AppMsg::Connected);

                    // Spawn a task that drains the command channel into the WS sender
                    let mut sender = sender;
                    let mut cmd_rx = cmd_rx;
                    spawn_local(async move {
                        while let Some(msg) = cmd_rx.next().await {
                            if sender.send(msg).await.is_err() {
                                break;
                            }
                        }
                    });

                    // Receive loop
                    while let Some(result) = receiver.recv().await {
                        match result {
                            Ok(msg) => link.send_message(AppMsg::ServerMsg(msg)),
                            Err(e) => {
                                link.send_message(AppMsg::WsError(format!("WS: {}", e)));
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    link.send_message(AppMsg::WsError(format!("Connect failed: {}", e)));
                }
            }
        });

        Self {
            manifest: None,
            tiles: HashMap::new(),
            status: "Connecting...".to_string(),
            source_text: DEFAULT_SOURCE.to_string(),
            cmd_tx,
            query_generation: 0,
            tiles_generation: 0,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            AppMsg::Connected => {
                self.status = "Connected".to_string();
                self.send(ClientMsg::GetManifest);
                self.query_generation += 1;
                self.send(ClientMsg::Query {
                    time_range: shared::temporal::TimeRange::new(0, 2_000_000),
                    resolution_ns: 2500,
                    generation: self.query_generation,
                });
                true
            }
            AppMsg::ServerMsg(msg) => {
                match msg {
                    ServerMsg::Manifest { manifest } => {
                        self.status = format!(
                            "Loaded: {} ({} - {})",
                            manifest.name,
                            format_ns(manifest.time_span.start_ns),
                            format_ns(manifest.time_span.end_ns),
                        );
                        let time_span = manifest.time_span.clone();
                        self.manifest = Some(manifest);
                        let duration = time_span.duration_ns();
                        let pad = duration / 10;
                        let padded = shared::temporal::TimeRange::new(
                            time_span.start_ns.saturating_sub(pad),
                            time_span.end_ns + pad,
                        );
                        let res = padded.duration_ns() / 800;
                        self.query_generation += 1;
                        self.send(ClientMsg::Query {
                            time_range: padded,
                            resolution_ns: res.max(1),
                            generation: self.query_generation,
                        });
                    }
                    ServerMsg::Tile {
                        path,
                        view,
                        generation,
                    } => {
                        if generation < self.query_generation {
                            return false;
                        }
                        if generation > self.tiles_generation {
                            self.tiles.clear();
                            self.tiles_generation = generation;
                        }
                        self.tiles.insert(path, view);
                    }
                    ServerMsg::Error { message } => {
                        self.status = format!("Error: {}", message);
                    }
                    ServerMsg::Heartbeat => {}
                }
                true
            }
            AppMsg::WsError(e) => {
                self.status = format!("Error: {}", e);
                true
            }
            AppMsg::ViewportChanged(vp) => {
                self.query_generation += 1;
                self.send(ClientMsg::Query {
                    time_range: vp.time_range.clone(),
                    resolution_ns: vp.resolution_ns(),
                    generation: self.query_generation,
                });
                true
            }
            AppMsg::SourceChanged(text) => {
                self.source_text = text.clone();
                self.send(ClientMsg::UpdateSource { text });
                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let on_viewport_change = ctx.link().callback(AppMsg::ViewportChanged);
        let on_source_change = ctx.link().callback(AppMsg::SourceChanged);

        html! {
            <div class="app-container">
                <div class="header">
                    <h1>{ "sig-int" }</h1>
                    <span class="status">{ &self.status }</span>
                </div>
                <div class="main-panes">
                    <Editor
                        value={AttrValue::from(self.source_text.clone())}
                        on_change={on_source_change}
                    />
                    <WaveCanvas
                        manifest={self.manifest.clone()}
                        tiles={TileMap(self.tiles.clone())}
                        on_viewport_change={on_viewport_change}
                        fit_time_range={self.manifest.as_ref().map(|m| m.time_span.clone())}
                    />
                </div>
                <div class="status-bar">
                    <span class={if self.manifest.is_some() { "status-ok" } else { "" }}>
                        { if self.manifest.is_some() { "●" } else { "○" } }
                    </span>
                    <span>{ &self.status }</span>
                </div>
            </div>
        }
    }
}

impl App {
    fn send(&self, msg: ClientMsg) {
        // Unbounded channel send is synchronous and infallible — no borrow conflicts
        let _ = self.cmd_tx.unbounded_send(msg);
    }
}

fn format_ns(ns: u64) -> String {
    if ns >= 1_000_000 {
        format!("{:.1}ms", ns as f64 / 1_000_000.0)
    } else if ns >= 1_000 {
        format!("{:.1}μs", ns as f64 / 1_000.0)
    } else {
        format!("{}ns", ns)
    }
}

const DEFAULT_SOURCE: &str = r#"{
  "signal": [
    ["I2C Transaction 1",
      { "name": "SCL",     "wave": "1.0.1.0.1.0.1.0.1." },
      { "name": "SDA",     "wave": "10..10.1.0..10..10." },
      { "name": "decoded", "wave": "=...=.......=....=.",
        "data": ["START", "0x48 W", "0x0F", "STOP"] }
    ],
    ["I2C Transaction 2",
      { "name": "SCL",     "wave": "1.0.1.0.1.0.1.0.1." },
      { "name": "SDA",     "wave": "10.1.0.1.0..10.1.0." },
      { "name": "decoded", "wave": "=...=.......=....=.",
        "data": ["START", "0x52 R", "0x7A", "STOP"] }
    ]
  ]
}"#;
