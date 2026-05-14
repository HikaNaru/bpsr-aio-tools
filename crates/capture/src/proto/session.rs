use bytes::Bytes;
use std::collections::{BTreeMap, HashMap};
use tracing::info;

const MAX_BUFFER:       usize = 4 * 1024 * 1024;
const MAX_CACHE_SEGS:   usize = 512;

// BPSR World service UUID and Social service UUID
const UUID_GAME:   u64 = 0x63335342;
const UUID_SOCIAL: u64 = 0x254C89A3;

struct TcpStream {
    buffer:   Vec<u8>,
    cache:    BTreeMap<u32, Vec<u8>>,
    next_seq: Option<u32>,
}

impl TcpStream {
    fn new() -> Self {
        Self { buffer: Vec::new(), cache: BTreeMap::new(), next_seq: None }
    }

    fn feed(&mut self, seq: u32, data: &[u8]) -> Vec<Bytes> {
        if data.is_empty() { return vec![]; }

        if self.next_seq.is_none() {
            self.next_seq = Some(seq);
        }

        // Retransmit check: signed distance negative → already seen
        let dist = seq.wrapping_sub(self.next_seq.unwrap()) as i32;
        if dist < 0 { return vec![]; }

        if self.cache.len() < MAX_CACHE_SEGS {
            self.cache.entry(seq).or_insert_with(|| data.to_vec());
        }

        self.drain()
    }

    fn drain(&mut self) -> Vec<Bytes> {
        let mut frames = Vec::new();
        loop {
            let next = match self.next_seq {
                Some(s) => s,
                None    => break,
            };
            let seg = match self.cache.remove(&next) {
                Some(s) => s,
                None    => break,
            };
            self.next_seq = Some(next.wrapping_add(seg.len() as u32));
            self.buffer.extend_from_slice(&seg);

            while self.buffer.len() >= 4 {
                let size = u32::from_be_bytes(self.buffer[0..4].try_into().unwrap()) as usize;
                if size < 6 || size > MAX_BUFFER {
                    self.buffer.clear();
                    break;
                }
                if self.buffer.len() < size { break; }
                frames.push(Bytes::copy_from_slice(&self.buffer[..size]));
                self.buffer.drain(0..size);
            }
        }
        frames
    }
}

pub struct SessionTable {
    // key: (src_ip_u32, src_port, dst_port)
    streams:      HashMap<(u32, u16, u16), TcpStream>,
    known_server: Option<(u32, u16)>,   // (server_ip, server_port)
    known_prefix: Option<u16>,          // first two octets of server IP
}

impl SessionTable {
    pub fn new() -> Self {
        Self { streams: HashMap::new(), known_server: None, known_prefix: None }
    }

    pub fn feed(
        &mut self,
        src_ip:   [u8; 4],
        src_port: u16,
        dst_port: u16,
        tcp_seq:  u32,
        data:     &[u8],
    ) -> Vec<Bytes> {
        if data.is_empty() { return vec![]; }

        let src_u32 = u32::from_be_bytes(src_ip);

        // Once server is known, filter by subnet (/16 prefix)
        if let Some(pfx) = self.known_prefix {
            if pfx != 0 {
                // pfx == 0 means "accept all" (RFC1918 fallback)
                if u16::from_be_bytes([src_ip[0], src_ip[1]]) != pfx {
                    return vec![];
                }
            }
        }

        let key = (src_u32, src_port, dst_port);
        let stream = self.streams.entry(key).or_insert_with(TcpStream::new);
        let frames = stream.feed(tcp_seq, data);

        // Detect game server from first valid Notify frame
        if self.known_server.is_none() {
            for f in &frames {
                if is_game_notify(f) {
                    self.known_server = Some((src_u32, src_port));
                    // RFC1918: 10.0.0.0/8, 172.16.0.0/12 (only 172.16-172.31), 192.168.0.0/16
                    let is_rfc1918 = src_ip[0] == 10
                        || (src_ip[0] == 172 && src_ip[1] >= 16 && src_ip[1] <= 31)
                        || (src_ip[0] == 192 && src_ip[1] == 168);
                    // 0 = accept all (fallback for RFC1918/unknown private nets)
                    let prefix = if !is_rfc1918 {
                        u16::from_be_bytes([src_ip[0], src_ip[1]])
                    } else {
                        0
                    };
                    self.known_prefix = Some(prefix);
                    info!(
                        "Game server detected: {}.{}.{}.{}:{} subnet={}.{}.*.*",
                        src_ip[0], src_ip[1], src_ip[2], src_ip[3], src_port,
                        src_ip[0], src_ip[1],
                    );
                    break;
                }
            }
        }

        frames
    }
}

// Frame layout: [4-byte size][2-byte type][8-byte uuid][...]
// We check type==2 (Notify, possibly compressed) and uuid == game or social UUID
fn is_game_notify(frame: &[u8]) -> bool {
    if frame.len() < 14 { return false; }
    let msg_type = u16::from_be_bytes([frame[4], frame[5]]) & 0x7FFF;
    if msg_type != 2 { return false; }
    let uuid = u64::from_be_bytes(frame[6..14].try_into().unwrap());
    uuid == UUID_GAME || uuid == UUID_SOCIAL
}
