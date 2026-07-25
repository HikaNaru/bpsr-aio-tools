use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use tracing::debug;

static UNKNOWN_METHODS: Lazy<Mutex<HashMap<u32, u64>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

static UNKNOWN_SERVICES: Lazy<Mutex<HashMap<u64, u64>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Log an unrecognized method under a known service, keyed by the real
/// `method_id` (not a payload-derived guess — the leading payload bytes are
/// often a changing varint like a timestamp/counter, which made every packet
/// look like a "new" opcode under the old byte-prefix keying).
pub fn log_unknown(service_uuid: u64, method_id: u32, data: &[u8]) {
    let mut map = UNKNOWN_METHODS.lock().unwrap();
    let count = map.entry(method_id).or_insert(0);
    *count += 1;
    if *count == 1 || *count % 100 == 0 {
        let preview = &data[..data.len().min(16)];
        debug!(service_uuid = %format!("{service_uuid:#018x}"), method_id = %format!("{method_id:#010x}"), count = *count, len = data.len(), preview = ?preview, "unknown method");
    }
}

/// Log a Notify under a service_uuid this build doesn't recognize at all.
pub fn log_unknown_service(service_uuid: u64, method_id: u32, len: usize) {
    let mut map = UNKNOWN_SERVICES.lock().unwrap();
    let count = map.entry(service_uuid).or_insert(0);
    *count += 1;
    if *count == 1 || *count % 100 == 0 {
        debug!(service_uuid = %format!("{service_uuid:#018x}"), method_id = %format!("{method_id:#010x}"), count = *count, len, "unknown service");
    }
}

pub fn opcode_stats() -> Vec<(u32, u64)> {
    let map = UNKNOWN_METHODS.lock().unwrap();
    let mut stats: Vec<_> = map.iter().map(|(&k, &v)| (k, v)).collect();
    stats.sort_by(|a, b| b.1.cmp(&a.1));
    stats
}
