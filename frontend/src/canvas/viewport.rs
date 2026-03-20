use shared::temporal::TimeRange;

/// Width of the signal label column in CSS pixels.
pub const LABEL_WIDTH: f64 = 100.0;

/// Viewport state: what time range and resolution the canvas is showing.
#[derive(Debug, Clone, PartialEq)]
pub struct Viewport {
    /// Visible time range in nanoseconds
    pub time_range: TimeRange,
    /// Vertical scroll offset in pixels
    pub v_offset: f64,
    /// Canvas CSS pixel dimensions (not physical pixels)
    pub canvas_width: f64,
    pub canvas_height: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            time_range: TimeRange::new(0, 2_400_000),
            v_offset: 0.0,
            canvas_width: 800.0,
            canvas_height: 600.0,
        }
    }
}

impl Viewport {
    /// Reset viewport to fit a given time range with padding.
    pub fn fit_time_range(&mut self, range: &TimeRange) {
        let duration = range.duration_ns();
        let pad = duration / 10;
        self.time_range = TimeRange::new(range.start_ns.saturating_sub(pad), range.end_ns + pad);
        self.v_offset = 0.0;
    }

    /// Width of the signal/time area (excluding label column).
    fn signal_area_width(&self) -> f64 {
        (self.canvas_width - LABEL_WIDTH).max(1.0)
    }

    /// Nanoseconds per CSS pixel in the signal area.
    pub fn resolution_ns(&self) -> u64 {
        let res = self.time_range.duration_ns() as f64 / self.signal_area_width();
        (res as u64).max(1)
    }

    /// Zoom in, anchored at the given fractional position within the signal area
    /// (0.0 = left edge of signals, 1.0 = right edge).
    pub fn zoom_in(&mut self, anchor_frac: f64, factor: f64) {
        self.zoom(anchor_frac, 1.0 / factor);
    }

    /// Zoom out, anchored at the given fractional position within the signal area.
    pub fn zoom_out(&mut self, anchor_frac: f64, factor: f64) {
        self.zoom(anchor_frac, factor);
    }

    fn zoom(&mut self, anchor_frac: f64, scale: f64) {
        let duration = self.time_range.duration_ns() as f64;
        let anchor_ns = self.time_range.start_ns as f64 + duration * anchor_frac;

        let new_duration = (duration * scale).clamp(100.0, 10_000_000_000.0);
        let new_start = anchor_ns - new_duration * anchor_frac;

        self.time_range.start_ns = new_start.max(0.0) as u64;
        self.time_range.end_ns = self.time_range.start_ns + new_duration as u64;
    }

    /// Pan by a fractional x offset (fraction of signal area width) and pixel y offset.
    pub fn pan(&mut self, dx_frac: f64, dy: f64) {
        let duration = self.time_range.duration_ns() as f64;
        let dt = (-dx_frac * duration) as i64;

        let new_start = (self.time_range.start_ns as i64 + dt).max(0) as u64;
        let new_end = new_start + self.time_range.duration_ns();

        self.time_range.start_ns = new_start;
        self.time_range.end_ns = new_end;
        self.v_offset += dy;
    }

    /// Convert a nanosecond timestamp to an x pixel coordinate.
    /// Maps time_range.start_ns to LABEL_WIDTH, time_range.end_ns to canvas_width.
    pub fn time_to_x(&self, time_ns: u64) -> f64 {
        if self.time_range.duration_ns() == 0 {
            return LABEL_WIDTH;
        }
        let frac = (time_ns as f64 - self.time_range.start_ns as f64)
            / self.time_range.duration_ns() as f64;
        LABEL_WIDTH + frac * self.signal_area_width()
    }

    /// Convert an x pixel coordinate to a nanosecond timestamp.
    pub fn x_to_time(&self, x: f64) -> u64 {
        let frac = (x - LABEL_WIDTH) / self.signal_area_width();
        (self.time_range.start_ns as f64 + frac * self.time_range.duration_ns() as f64) as u64
    }

    /// Return a new viewport with extra margin on each side (for prefetching tiles).
    pub fn with_margin(&self, margin: f64) -> Self {
        let duration = self.time_range.duration_ns() as f64;
        let extra = duration * margin;
        let mut vp = self.clone();
        vp.time_range = TimeRange::new(
            (self.time_range.start_ns as f64 - extra).max(0.0) as u64,
            self.time_range.end_ns + extra as u64,
        );
        vp
    }
}
