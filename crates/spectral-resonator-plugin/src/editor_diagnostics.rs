//! GUI-only timing. Optional probe output is written on editor close, never
//! from the audio callback, and is enabled only by the local test environment.
use std::collections::VecDeque;

#[derive(Default)]
pub struct FrameStats {
    last: Option<f64>,
    intervals: VecDeque<f64>,
    ui_ms: VecDeque<f64>,
    analysis_ms: VecDeque<f64>,
}
impl FrameStats {
    pub fn begin(&mut self, now: f64) {
        if let Some(last) = self.last.replace(now) {
            let dt = now - last;
            if dt > 0.0001 {
                bounded_push(&mut self.intervals, dt);
            }
        }
    }
    pub fn finish(&mut self, ui_ms: f64, analysis_ms: f64) {
        bounded_push(&mut self.ui_ms, ui_ms);
        bounded_push(&mut self.analysis_ms, analysis_ms);
    }
    pub fn fps(&self) -> f64 {
        let mut seconds = 0.0;
        let mut count = 0;
        for dt in self.intervals.iter().rev() {
            seconds += dt;
            count += 1;
            if seconds >= 1.0 {
                break;
            }
        }
        if seconds > 0.0 {
            count as f64 / seconds
        } else {
            0.0
        }
    }
    pub fn export(&self, cap: u32) {
        let Ok(path) = std::env::var("SPECTRAL_UI_PROFILE") else {
            return;
        };
        if self.intervals.len() < 5 {
            return;
        }
        let mut dt: Vec<_> = self.intervals.iter().copied().collect();
        dt.sort_by(f64::total_cmp);
        let mean = |values: &VecDeque<f64>| values.iter().sum::<f64>() / values.len().max(1) as f64;
        let line = format!(
            "{{\"cap\":{cap},\"frames\":{},\"fps\":{:.3},\"p50_ms\":{:.3},\"p95_ms\":{:.3},\"max_ms\":{:.3},\"ui_mean_ms\":{:.3},\"analysis_mean_ms\":{:.3}}}\n",
            dt.len(),
            dt.len() as f64 / dt.iter().sum::<f64>(),
            dt[dt.len() / 2] * 1000.0,
            dt[((dt.len() - 1) as f64 * 0.95).round() as usize] * 1000.0,
            dt[dt.len() - 1] * 1000.0,
            mean(&self.ui_ms),
            mean(&self.analysis_ms)
        );
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            use std::io::Write;
            let _ = file.write_all(line.as_bytes());
        }
    }
}
fn bounded_push(values: &mut VecDeque<f64>, value: f64) {
    if values.len() == 512 {
        values.pop_front();
    }
    values.push_back(value);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn measures_actual_intervals_not_the_configured_cap() {
        let mut stats = FrameStats::default();
        for i in 0..200 {
            stats.begin(i as f64 / 47.0);
        }
        assert!((stats.fps() - 47.0).abs() < 0.001);
    }
}
