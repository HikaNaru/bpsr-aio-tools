pub mod transport;
pub mod session;
pub mod framing;
pub mod crypto;
pub mod opcode;
pub mod packets;
pub mod service_ids;
pub mod method_ids;

use crate::{debug_stats, RawPacket};
use crossbeam_channel::{Receiver, Sender};
use game::GameEvent;
use std::sync::atomic::Ordering;
use tracing::{debug, warn};

pub fn run_parser(raw_rx: Receiver<RawPacket>, event_tx: Sender<GameEvent>) {
    std::thread::Builder::new()
        .name("parser".into())
        .spawn(move || {
            let mut sessions = session::SessionTable::new();

            for raw in raw_rx {
                let Some(extracted) = transport::extract_payload(&raw.data) else {
                    continue;
                };

                debug_stats::PAYLOADS_EXTRACTED.fetch_add(1, Ordering::Relaxed);
                let frames = sessions.feed(extracted.src_ip, extracted.src_port, extracted.dst_port, extracted.tcp_seq, &extracted.data);
                debug_stats::FRAMES_PROCESSED.fetch_add(frames.len() as u64, Ordering::Relaxed);

                for frame in frames {
                    let events = process_frame_buffer(&frame);
                    debug_stats::EVENTS_DISPATCHED.fetch_add(events.len() as u64, Ordering::Relaxed);
                    for event in events {
                        debug_stats::record_event_str(format_event(&event));
                        let _ = event_tx.send(event);
                    }
                }
            }
            warn!("parser thread exiting");
        })
        .expect("parser thread");
}

/// Process a buffer that may contain one or more frames (Notify or FrameDown).
/// FrameDown packets wrap inner frames that may be Zstd-compressed.
fn process_frame_buffer(data: &[u8]) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let mut pos = 0;

    while pos < data.len() {
        if pos + 6 > data.len() {
            break;
        }

        // 4-byte big-endian frame size (includes these 4 bytes)
        let size = match data[pos..pos + 4].try_into().ok().map(u32::from_be_bytes) {
            Some(s) => s as usize,
            None => break,
        };

        if size < 6 || size > 10 * 1024 * 1024 {
            debug!("frame: bad size {size}");
            break;
        }
        if pos + size > data.len() {
            break; // incomplete frame
        }

        let frame = &data[pos..pos + size];
        pos += size;

        let ptype      = u16::from_be_bytes([frame[4], frame[5]]);
        let compressed = (ptype & 0x8000) != 0;
        let msg_type   = ptype & 0x7FFF;
        let payload    = &frame[6..];

        match msg_type {
            2 => {
                // Notify: [8 service_uuid][4 stub_id][4 method_id][payload...]
                if payload.len() < 16 {
                    continue;
                }
                let service_uuid = u64::from_be_bytes(payload[0..8].try_into().unwrap());
                // stub_id at [8..12] — skip
                let method_id = u32::from_be_bytes(payload[12..16].try_into().unwrap());
                let msg_data = &payload[16..];

                let decoded = if compressed {
                    match zstd::decode_all(msg_data) {
                        Ok(d) => d,
                        Err(e) => {
                            debug!("Notify zstd decompress failed: {e}");
                            continue;
                        }
                    }
                } else {
                    msg_data.to_vec()
                };

                events.extend(opcode::dispatch(service_uuid, method_id, &decoded));
            }
            6 => {
                // FrameDown: [4 seq_id][nested frames...]
                if payload.len() < 4 {
                    continue;
                }
                let nested = &payload[4..]; // skip 4-byte server sequence id

                let inner = if compressed {
                    match zstd::decode_all(nested) {
                        Ok(d) => d,
                        Err(e) => {
                            debug!("FrameDown zstd decompress failed: {e}");
                            continue;
                        }
                    }
                } else {
                    nested.to_vec()
                };

                // Process inner frames recursively
                events.extend(process_frame_buffer(&inner));
            }
            _ => {}
        }
    }

    events
}

fn format_event(event: &GameEvent) -> String {
    match event {
        GameEvent::EntityName { id, name, class, .. } => {
            format!("[EntityName] id={:#x} name={:?} class={:?}", id.0, name, class)
        }
        GameEvent::Combat(c) => {
            format!(
                "[Combat] src={:#x} → tgt={:#x}  dmg={}  crit={}",
                c.source_id.0, c.target_id.0, c.damage, c.is_crit
            )
        }
        GameEvent::Heal(c) => {
            format!(
                "[Heal] src={:#x} → tgt={:#x}  heal={}  crit={}",
                c.source_id.0, c.target_id.0, c.damage, c.is_crit
            )
        }
        GameEvent::LocalPlayer { id } => {
            format!("[LocalPlayer] id={:#x}", id.0)
        }
        GameEvent::ZoneChange { zone_id, zone_name } => {
            format!("[ZoneChange] id={zone_id} name={zone_name:?}")
        }
        GameEvent::EntityDespawn { id } => {
            format!("[EntityDespawn] id={:#x}", id.0)
        }
        GameEvent::PlayerInventory { id, modules } => {
            format!("[PlayerInventory] id={:#x} modules={}", id.0, modules.len())
        }
        GameEvent::ShieldList { id, shields } => {
            format!("[ShieldList] id={:#x} shields={}", id.0, shields.len())
        }
        GameEvent::EquipData { id, slots } => {
            format!("[EquipData] id={:#x} slots={}", id.0, slots.len())
        }
        GameEvent::SkillLoadout { id, skills } => {
            format!("[SkillLoadout] id={:#x} skills={}", id.0, skills.len())
        }
        GameEvent::EntityStats { id, stats } => {
            format!(
                "[EntityStats] id={:#x} lv={:?} hp={:?}/{:?} atk={:?} gs={:?}",
                id.0, stats.level, stats.hp, stats.max_hp, stats.attack, stats.ability_score
            )
        }
        GameEvent::DungeonState { state } => {
            format!("[DungeonState] state={state:?}")
        }
        GameEvent::DungeonPhaseSignal { targets, phase_id } => {
            format!("[DungeonPhaseSignal] phase_id={phase_id:?} targets={}", targets.len())
        }
        GameEvent::Chat { channel, sender_name, text, .. } => {
            format!("[Chat] ch={channel:?} from={sender_name:?} text={text:?}")
        }
        GameEvent::MatchmakingAlert { kind } => {
            format!("[MatchmakingAlert] kind={kind:?}")
        }
        GameEvent::ThreatUpdate { target_id, entity_id, threat } => {
            format!("[ThreatUpdate] target={target_id:?} entity={entity_id:?} threat={threat}")
        }
        GameEvent::Unknown { opcode, .. } => {
            format!("[Unknown] opcode={opcode:#010x}")
        }
    }
}
