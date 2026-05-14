use crate::{debug_stats, RawPacket};
use anyhow::Result;
use bytes::Bytes;
use crossbeam_channel::Sender;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tracing::{error, info};

pub fn start_capture(
    tx: Sender<RawPacket>,
) -> Result<()> {
    let device = pcap::Device::lookup()?.ok_or_else(|| anyhow::anyhow!("no default device"))?;

    info!("capturing on {}", device.name);

    let bpf = "tcp or udp".to_string();
    info!("BPF filter: {bpf}");

    let mut cap = pcap::Capture::from_device(device)?
        .snaplen(65535)
        .promisc(false)
        .timeout(100)
        .open()?;

    cap.filter(&bpf, true)?;

    std::thread::Builder::new()
        .name("capture".into())
        .spawn(move || loop {
            match cap.next_packet() {
                Ok(packet) => {
                    debug_stats::RAW_PACKETS.fetch_add(1, Ordering::Relaxed);
                    let ts = Duration::from_secs(packet.header.ts.tv_sec as u64)
                        + Duration::from_micros(packet.header.ts.tv_usec as u64);
                    let data = Bytes::copy_from_slice(packet.data);
                    let _ = tx.send(RawPacket { timestamp: ts, data });
                }
                Err(pcap::Error::TimeoutExpired) => {}
                Err(e) => {
                    error!("capture error: {e}");
                    break;
                }
            }
        })?;

    Ok(())
}
