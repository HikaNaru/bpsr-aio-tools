use bytes::Bytes;
use etherparse::{SlicedPacket, TransportSlice};

/// Strip Ethernet/IP/TCP/UDP headers, return application payload.
pub fn extract_payload(raw: &[u8]) -> Option<Bytes> {
    let sliced = SlicedPacket::from_ethernet(raw).ok()?;
    let payload = match sliced.transport? {
        TransportSlice::Tcp(tcp)   => tcp.payload(),
        TransportSlice::Udp(udp)   => udp.payload(),
        TransportSlice::Icmpv4(v4) => v4.payload(),
        TransportSlice::Icmpv6(v6) => v6.payload(),
        _ => return None,
    };
    if payload.is_empty() {
        return None;
    }
    Some(Bytes::copy_from_slice(payload))
}
