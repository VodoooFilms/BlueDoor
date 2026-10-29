//! Bounded, local measurements. Never stores document text, names or paths.
use std::sync::Mutex;
use std::time::Instant;
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Sample {
    stage: &'static str,
    elapsed_ms: u64,
    peak_rss_kb: Option<u64>,
}
static SAMPLES: Mutex<Vec<Sample>> = Mutex::new(Vec::new());
pub(crate) struct Timer {
    stage: &'static str,
    started: Instant,
}
impl Timer {
    pub(crate) fn new(stage: &'static str) -> Self {
        Self {
            stage,
            started: Instant::now(),
        }
    }
}
impl Drop for Timer {
    fn drop(&mut self) {
        // VmHWM is a process high-water mark, not an isolated stage allocation.
        let peak_rss_kb = std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|s| {
                s.lines().find_map(|l| {
                    l.strip_prefix("VmHWM:")
                        .and_then(|v| v.split_whitespace().next()?.parse().ok())
                })
            });
        if let Ok(mut samples) = SAMPLES.lock() {
            if samples.len() >= 50 {
                samples.remove(0);
            }
            samples.push(Sample {
                stage: self.stage,
                elapsed_ms: self.started.elapsed().as_millis() as u64,
                peak_rss_kb,
            });
        }
    }
}
#[tauri::command]
pub(crate) fn get_local_performance() -> Vec<Sample> {
    SAMPLES.lock().map(|s| s.clone()).unwrap_or_default()
}
