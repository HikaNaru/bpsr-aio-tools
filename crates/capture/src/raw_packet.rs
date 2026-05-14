use bytes::Bytes;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RawPacket {
    pub timestamp: Duration,
    pub data:      Bytes,
}
