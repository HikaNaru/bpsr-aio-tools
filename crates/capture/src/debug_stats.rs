use once_cell::sync::Lazy;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub static RAW_PACKETS:        AtomicU64 = AtomicU64::new(0);
pub static PAYLOADS_EXTRACTED: AtomicU64 = AtomicU64::new(0);
pub static FRAMES_PROCESSED:   AtomicU64 = AtomicU64::new(0);
pub static EVENTS_DISPATCHED:  AtomicU64 = AtomicU64::new(0);
pub static UNKNOWN_DISPATCHES: AtomicU64 = AtomicU64::new(0);

pub fn load_all() -> PipelineStats {
    PipelineStats {
        raw_packets:        RAW_PACKETS.load(Ordering::Relaxed),
        payloads_extracted: PAYLOADS_EXTRACTED.load(Ordering::Relaxed),
        frames_processed:   FRAMES_PROCESSED.load(Ordering::Relaxed),
        events_dispatched:  EVENTS_DISPATCHED.load(Ordering::Relaxed),
        unknown_dispatches: UNKNOWN_DISPATCHES.load(Ordering::Relaxed),
    }
}

pub struct PipelineStats {
    pub raw_packets:        u64,
    pub payloads_extracted: u64,
    pub frames_processed:   u64,
    pub events_dispatched:  u64,
    pub unknown_dispatches: u64,
}

const MAX_EVENT_ENTRIES: usize = 500;

pub static RECENT_EVENTS: Lazy<Mutex<VecDeque<String>>> =
    Lazy::new(|| Mutex::new(VecDeque::new()));

pub fn record_event_str(text: String) {
    let mut q = RECENT_EVENTS.lock().unwrap();
    if q.len() >= MAX_EVENT_ENTRIES {
        q.pop_front();
    }
    q.push_back(text);
}
