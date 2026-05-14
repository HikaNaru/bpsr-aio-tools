pub mod transport;
pub mod session;
pub mod framing;
pub mod crypto;
pub mod opcode;
pub mod packets;

use crate::RawPacket;
use crossbeam_channel::{Receiver, Sender};
use game::GameEvent;
use tracing::warn;

pub fn run_parser(raw_rx: Receiver<RawPacket>, event_tx: Sender<GameEvent>) {
    std::thread::Builder::new()
        .name("parser".into())
        .spawn(move || {
            let mut sessions = session::SessionTable::new();

            for raw in raw_rx {
                let Some(payload) = transport::extract_payload(&raw.data) else {
                    continue;
                };
                let frames = sessions.feed(payload);
                for frame in frames {
                    let decrypted = crypto::decrypt(frame);
                    match opcode::dispatch(&decrypted) {
                        Some(event) => {
                            let _ = event_tx.send(event);
                        }
                        None => {
                            packets::unknown::log_unknown(&decrypted);
                        }
                    }
                }
            }
            warn!("parser thread exiting");
        })
        .expect("parser thread");
}
