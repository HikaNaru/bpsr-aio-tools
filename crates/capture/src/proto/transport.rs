use bytes::Bytes;
use etherparse::{NetSlice, SlicedPacket, TransportSlice};

pub struct ExtractedPayload {
    pub data:     Bytes,
    pub src_port: u16,
    pub dst_port: u16,
    pub src_ip:   [u8; 4],
    pub tcp_seq:  u32,
    pub proto:    &'static str,
}

pub fn extract_payload(raw: &[u8]) -> Option<ExtractedPayload> {
    let sliced = SlicedPacket::from_ethernet(raw).ok()?;

    let src_ip = match sliced.net.as_ref()? {
        NetSlice::Ipv4(ipv4) => ipv4.header().source(),
        _ => return None,
    };

    let (src_port, dst_port, tcp_seq, proto, payload) = match sliced.transport? {
        TransportSlice::Tcp(tcp) => (
            tcp.source_port(),
            tcp.destination_port(),
            tcp.sequence_number(),
            "TCP",
            tcp.payload(),
        ),
        TransportSlice::Udp(udp) => (
            udp.source_port(),
            udp.destination_port(),
            0u32,
            "UDP",
            udp.payload(),
        ),
        _ => return None,
    };

    if payload.is_empty() {
        return None;
    }

    Some(ExtractedPayload {
        data: Bytes::copy_from_slice(payload),
        src_port,
        dst_port,
        src_ip,
        tcp_seq,
        proto,
    })
}
