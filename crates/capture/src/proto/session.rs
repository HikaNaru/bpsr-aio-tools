use bytes::Bytes;

/// Placeholder TCP stream reassembly.
/// Currently passes payloads through as single-frame chunks.
/// Replace with proper seq-ordered reassembly once protocol is known.
pub struct SessionTable;

impl SessionTable {
    pub fn new() -> Self {
        Self
    }

    pub fn feed(&mut self, payload: Bytes) -> Vec<Bytes> {
        // TODO: proper TCP stream reassembly
        vec![payload]
    }
}
