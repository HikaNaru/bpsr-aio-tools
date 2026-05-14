use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use tracing::debug;

static UNKNOWN_OPCODES: Lazy<Mutex<HashMap<u32, u64>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Log unrecognized packet for reverse-engineering analysis.
pub fn log_unknown(data: &[u8]) {
    if data.len() < 4 {
        return;
    }
    let opcode = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let mut map = UNKNOWN_OPCODES.lock().unwrap();
    let count = map.entry(opcode).or_insert(0);
    *count += 1;
    if *count == 1 || *count % 100 == 0 {
        let preview = &data[..data.len().min(16)];
        debug!(opcode = %format!("{:#010x}", opcode), count = *count, len = data.len(), preview = ?preview, "unknown opcode");
    }
}

pub fn opcode_stats() -> Vec<(u32, u64)> {
    let map = UNKNOWN_OPCODES.lock().unwrap();
    let mut stats: Vec<_> = map.iter().map(|(&k, &v)| (k, v)).collect();
    stats.sort_by(|a, b| b.1.cmp(&a.1));
    stats
}
