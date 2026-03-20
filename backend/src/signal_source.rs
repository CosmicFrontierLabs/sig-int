use shared::temporal::{TimeRange, ViewQuery};
use shared::types::*;

// ---------------------------------------------------------------------------
// SignalSource trait — the core abstraction
// ---------------------------------------------------------------------------

pub trait SignalSource: Send + Sync {
    fn manifest(&self) -> ManifestNode;
    fn view(&self, query: &ViewQuery) -> SignalView;
    fn children(&self) -> Vec<&dyn SignalSource>;
    /// Resolution threshold (ns/px) above which this source returns Block instead of Waveform
    fn block_threshold_ns(&self) -> u64;
}

// ---------------------------------------------------------------------------
// InlineSource — a single signal with explicit transitions
// ---------------------------------------------------------------------------

pub struct InlineSource {
    pub name: String,
    pub path: String,
    pub kind: SignalKind,
    pub transitions: Vec<Transition>,
}

impl SignalSource for InlineSource {
    fn manifest(&self) -> ManifestNode {
        ManifestNode::Signal(SignalInfo {
            name: self.name.clone(),
            path: self.path.clone(),
            kind: self.kind.clone(),
        })
    }

    fn view(&self, query: &ViewQuery) -> SignalView {
        // Filter transitions to the query's time range
        let filtered: Vec<Transition> = self
            .transitions
            .iter()
            .filter(|t| t.time_ns < query.time_range.end_ns)
            .cloned()
            .collect();

        SignalView::Waveform {
            signals: vec![SignalWaveform {
                name: self.name.clone(),
                transitions: filtered,
            }],
            overlay: None,
            detail_alpha: 1.0,
        }
    }

    fn children(&self) -> Vec<&dyn SignalSource> {
        vec![]
    }

    fn block_threshold_ns(&self) -> u64 {
        0 // individual signals never collapse to blocks
    }
}

// ---------------------------------------------------------------------------
// GroupSource — hierarchical group that collapses to a block when zoomed out
// ---------------------------------------------------------------------------

pub struct GroupSource {
    pub name: String,
    pub path: String,
    pub children: Vec<Box<dyn SignalSource>>,
    pub time_span: TimeRange,
    pub block_threshold: u64,
    /// Color for the collapsed block
    pub block_color: String,
    /// Label for the collapsed block
    pub block_label: String,
}

impl SignalSource for GroupSource {
    fn manifest(&self) -> ManifestNode {
        ManifestNode::Group {
            name: self.name.clone(),
            path: self.path.clone(),
            children: self.children.iter().map(|c| c.manifest()).collect(),
            time_span: self.time_span.clone(),
        }
    }

    fn view(&self, query: &ViewQuery) -> SignalView {
        // GroupSource.view() returns ONLY its own leaf signals (not children's).
        // The query system walks the tree and emits a separate tile per group.
        // This means each group gets its own overlay on the renderer.

        // Collect only direct InlineSource children's signals
        let mut own_signals = Vec::new();
        for child in &self.children {
            // Only collect leaf signals — child groups are handled separately
            if child.block_threshold_ns() == 0 {
                // It's an InlineSource (leaf)
                match child.view(query) {
                    SignalView::Waveform { signals, .. } => {
                        own_signals.extend(signals);
                    }
                }
            }
        }

        let threshold = self.block_threshold as f64;
        let res = query.resolution_ns as f64;
        let zone_lo = threshold / 3.0;
        let zone_hi = threshold * 3.0;

        let detail_alpha = if res <= zone_lo {
            1.0
        } else if res >= zone_hi {
            0.0
        } else {
            let alpha = 1.0 - (res.ln() - zone_lo.ln()) / (zone_hi.ln() - zone_lo.ln());
            alpha.clamp(0.0, 1.0)
        };

        let block_summary = BlockSummary {
            name: self.name.clone(),
            start_ns: self.time_span.start_ns,
            end_ns: self.time_span.end_ns,
            color: self.block_color.clone(),
            label: self.block_label.clone(),
        };

        SignalView::Waveform {
            signals: own_signals,
            overlay: Some(block_summary),
            detail_alpha,
        }
    }

    fn children(&self) -> Vec<&dyn SignalSource> {
        self.children.iter().map(|c| c.as_ref()).collect()
    }

    fn block_threshold_ns(&self) -> u64 {
        self.block_threshold
    }
}

// ---------------------------------------------------------------------------
// SourceRegistry — holds the loaded source tree
// ---------------------------------------------------------------------------

pub struct SourceRegistry {
    root: Box<dyn SignalSource>,
}

impl SourceRegistry {
    pub fn new(root: Box<dyn SignalSource>) -> Self {
        Self { root }
    }

    pub fn manifest(&self) -> SourceManifest {
        let root_manifest = self.root.manifest();
        let (children, time_span) = match root_manifest {
            ManifestNode::Group {
                children,
                time_span,
                ..
            } => (children, time_span),
            signal @ ManifestNode::Signal(_) => (
                vec![signal],
                TimeRange::new(0, 1_000_000_000), // default 1s
            ),
        };

        SourceManifest {
            name: "sig-int".to_string(),
            children,
            time_span,
        }
    }

    pub fn query(&self, query: &ViewQuery) -> Vec<(String, SignalView)> {
        let mut results = Vec::new();
        self.collect_views(&*self.root, query, &mut results);
        results
    }

    fn collect_views(
        &self,
        source: &dyn SignalSource,
        query: &ViewQuery,
        results: &mut Vec<(String, SignalView)>,
    ) {
        // Emit this source's own view (with its own overlay)
        let view = source.view(query);
        let path = match source.manifest() {
            ManifestNode::Signal(s) => s.path,
            ManifestNode::Group { path, .. } => path,
        };

        // Only emit if this group has signals (skip empty intermediate groups)
        let SignalView::Waveform { ref signals, .. } = view;
        if !signals.is_empty() {
            results.push((path, view));
        }

        // Recurse into child groups
        for child in source.children() {
            if child.block_threshold_ns() > 0 {
                // It's a group — recurse
                self.collect_views(child, query, results);
            }
        }
    }

    pub fn replace(&mut self, root: Box<dyn SignalSource>) {
        self.root = root;
    }
}
