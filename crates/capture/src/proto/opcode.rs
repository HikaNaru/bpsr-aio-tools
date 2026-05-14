use bytes::Bytes;
use game::GameEvent;

/// Placeholder opcode dispatch.
/// TODO: populate once damage/entity opcodes are known from RE.
pub fn dispatch(_data: &[u8]) -> Option<GameEvent> {
    None
}
