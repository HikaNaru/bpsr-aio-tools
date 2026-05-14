use bytes::Bytes;

/// Placeholder frame boundary detection.
/// TODO: implement once packet format is known (likely 4-byte LE length prefix).
pub fn split_frames(data: Bytes) -> Vec<Bytes> {
    vec![data]
}
