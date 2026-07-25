//! Custom binary "blob" delta-diff format used by `SyncDungeonDirtyData`
//! (WorldNtf method 0x18). Not protobuf — ported from BPSR-ZDPS's
//! `BlobReader`/`BlobType`/`Blobs/DungeonDirtyData.cs` etc. Field numbers
//! match the corresponding `DungeonSyncData` protobuf message (same schema,
//! just diff-encoded), confirmed against the reference source.
//!
//! Container shape: `i32 tag(-2)`, `i32 size`(-3 = empty), then repeated
//! `i32 field_index` + field payload, until a non-positive index (-3 = end)
//! is hit. An unrecognized field index means we don't know its length, so —
//! matching the reference's own behavior — we jump straight to the end of
//! the container (`offset + size`) and stop, meaning only fields ordered
//! before the first unhandled one are recovered. We only care about
//! FlowInfo (2) and Target (4), both low field numbers.

pub struct BlobReader<'a> {
    buf: &'a [u8],
    pos: usize,
    stream_safe: bool,
}

impl<'a> BlobReader<'a> {
    pub fn new(buf: &'a [u8], stream_safe: bool) -> Self {
        Self { buf, pos: 0, stream_safe }
    }

    fn advance(&mut self, n: usize) {
        self.pos += if self.stream_safe { n + 4 } else { n };
    }

    pub fn read_i32(&mut self) -> Option<i32> {
        let bytes = self.buf.get(self.pos..self.pos + 4)?;
        let v = i32::from_le_bytes(bytes.try_into().ok()?);
        self.advance(4);
        Some(v)
    }

    pub fn read_string(&mut self) -> Option<String> {
        let len = self.read_i32()? as usize;
        let bytes = self.buf.get(self.pos..self.pos + len)?;
        let s = String::from_utf8_lossy(bytes).into_owned();
        self.advance(len);
        Some(s)
    }

    /// Runs a `BlobType`-shaped container: tag(-2), size, repeated
    /// (index, field) pairs until a non-positive index. `parse_field`
    /// returns `true` if it consumed the field; on `false` (unrecognized),
    /// parsing jumps to the container's end and stops, matching reference.
    pub fn read_container(&mut self, mut parse_field: impl FnMut(i32, &mut BlobReader<'a>) -> bool) {
        let Some(tag) = self.read_i32() else { return };
        if tag != -2 { return; }
        let Some(size) = self.read_i32() else { return };
        if size == -3 || size < 0 { return; }

        let start = self.pos;
        loop {
            let Some(index) = self.read_i32() else { return };
            if index <= 0 { return; }
            if !parse_field(index, self) {
                self.pos = start + size as usize;
                return;
            }
        }
    }

    /// `Dictionary<i32, DungeonTargetData>` diff read: add/remove/update
    /// counts, matching `BlobReader.ReadHashMap<T,X>`. Only "add" and
    /// "update" entries carry a value; "remove" entries are key-only.
    pub fn read_target_data_map(&mut self) -> Vec<(i32, DungeonTargetData)> {
        let mut out = Vec::new();
        let Some(mut add) = self.read_i32() else { return out };
        if add == -4 { return out; } // early exit, empty
        let (remove, update) = if add == -1 {
            let Some(real_add) = self.read_i32() else { return out };
            add = real_add;
            (0, 0)
        } else {
            let Some(r) = self.read_i32() else { return out };
            let Some(u) = self.read_i32() else { return out };
            (r, u)
        };

        for _ in 0..add {
            let Some(key) = self.read_i32() else { return out };
            let val = DungeonTargetData::read(self);
            out.push((key, val));
        }
        for _ in 0..remove {
            if self.read_i32().is_none() { return out; }
        }
        for _ in 0..update {
            let Some(key) = self.read_i32() else { return out };
            let val = DungeonTargetData::read(self);
            out.push((key, val));
        }
        out
    }
}

#[derive(Default, Clone, Copy)]
pub struct DungeonTargetData {
    pub target_id: i32,
    pub nums:      i32,
    pub complete:  i32,
}

impl DungeonTargetData {
    fn read(blob: &mut BlobReader) -> Self {
        let mut out = Self::default();
        blob.read_container(|index, blob| {
            match index {
                1 => { out.target_id = blob.read_i32().unwrap_or(0); true }
                2 => { out.nums      = blob.read_i32().unwrap_or(0); true }
                3 => { out.complete  = blob.read_i32().unwrap_or(0); true }
                _ => false,
            }
        });
        out
    }
}

#[derive(Default)]
pub struct DungeonDirtyData {
    pub state:   Option<i32>,
    pub targets: Vec<(i32, DungeonTargetData)>,
}

impl DungeonDirtyData {
    /// Parses the top-level dirty container, extracting only FlowInfo.State
    /// (field 2 of DungeonFlowInfo, itself field 2 of DungeonSyncData) and
    /// Target.TargetData (field 1 of DungeonTarget, itself field 4 of
    /// DungeonSyncData). Everything else (Damage, PlayerList, etc.) is
    /// unhandled — reading bails at the first such field per read_container,
    /// which is fine since FlowInfo/Target have lower field numbers.
    pub fn parse(buf: &[u8], stream_safe: bool) -> Self {
        let mut blob = BlobReader::new(buf, stream_safe);
        let mut out = Self::default();
        blob.read_container(|index, blob| {
            match index {
                // SceneUuid — scalar (not container-framed), just consume.
                1 => blob.read_i32().is_some(),
                // FlowInfo — message; we only want .State (its field 1).
                2 => {
                    let mut state = None;
                    blob.read_container(|fi, blob| {
                        if fi == 1 { state = blob.read_i32(); true } else { false }
                    });
                    out.state = state;
                    true
                }
                // Title — message we don't care about; skip via the generic
                // container framing (tag+size lets us jump past it intact
                // regardless of its internal fields).
                3 => { blob.read_container(|_, _| false); true }
                // Target — message; want .TargetData (its field 1).
                4 => {
                    let mut targets = Vec::new();
                    blob.read_container(|fi, blob| {
                        if fi == 1 { targets = blob.read_target_data_map(); true } else { false }
                    });
                    out.targets = targets;
                    true
                }
                _ => false,
            }
        });
        out
    }
}
