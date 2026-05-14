/// Snapshot of DPS per second for sparkline rendering.
#[derive(Debug, Default, Clone)]
pub struct Timeline {
    pub points: Vec<(f64, f64)>, // (elapsed_secs, dps)
}

impl Timeline {
    pub fn push(&mut self, elapsed_secs: f64, dps: f64) {
        self.points.push((elapsed_secs, dps));
        // Keep last 300 seconds
        if self.points.len() > 300 {
            self.points.remove(0);
        }
    }
}
