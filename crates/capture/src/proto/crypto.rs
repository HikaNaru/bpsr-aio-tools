// Blue Protocol packets are NOT encrypted — only Zstd-compressed.
// Decompression is handled inline in the frame pipeline (proto/mod.rs).
// This module is kept as a no-op pass-through for structural compatibility.

use bytes::Bytes;

pub fn decrypt(data: Bytes) -> Bytes {
    data
}
