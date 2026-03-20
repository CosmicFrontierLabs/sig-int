use serde::{Deserialize, Serialize};

/// A time range in nanoseconds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeRange {
    pub start_ns: u64,
    pub end_ns: u64,
}

impl TimeRange {
    pub fn new(start_ns: u64, end_ns: u64) -> Self {
        Self { start_ns, end_ns }
    }

    pub fn duration_ns(&self) -> u64 {
        self.end_ns.saturating_sub(self.start_ns)
    }

    pub fn contains(&self, t: u64) -> bool {
        t >= self.start_ns && t < self.end_ns
    }

    pub fn overlaps(&self, other: &TimeRange) -> bool {
        self.start_ns < other.end_ns && other.start_ns < self.end_ns
    }
}

/// A query for signal data at a specific viewport resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewQuery {
    pub time_range: TimeRange,
    /// Desired nanoseconds per output sample point.
    /// The source should downsample to approximately this resolution.
    pub resolution_ns: u64,
}
