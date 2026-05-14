use crate::RawPacket;
use anyhow::Result;
use bytes::Bytes;
use crossbeam_channel::Sender;
use std::time::Duration;
use tracing::{error, info};

pub fn list_devices() -> Vec<String> {
    pcap::Device::list()
        .unwrap_or_default()
        .into_iter()
        .map(|d| d.name)
        .collect()
}

pub fn start_capture(interface: Option<String>, tx: Sender<RawPacket>) -> Result<()> {
    let device = match interface {
        Some(name) => pcap::Device::list()?
            .into_iter()
            .find(|d| d.name == name)
            .ok_or_else(|| anyhow::anyhow!("interface '{}' not found", name))?,
        None => pcap::Device::lookup()?.ok_or_else(|| anyhow::anyhow!("no default device"))?,
    };

    info!("capturing on {}", device.name);

    let mut cap = pcap::Capture::from_device(device)?
        .snaplen(65535)
        .promisc(false)
        .timeout(100)
        .open()?;

    cap.filter("tcp or udp", true)?;

    std::thread::Builder::new()
        .name("capture".into())
        .spawn(move || loop {
            match cap.next_packet() {
                Ok(packet) => {
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
