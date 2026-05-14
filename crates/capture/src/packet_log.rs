use once_cell::sync::Lazy;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOGGER: Lazy<Mutex<Option<BufWriter<File>>>> = Lazy::new(|| Mutex::new(None));

pub fn init(path: &std::path::Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    let mut guard = LOGGER.lock().unwrap();
    let writer = BufWriter::new(file);
    *guard = Some(writer);
    Ok(())
}

pub fn log_packet(proto: &str, src_port: u16, dst_port: u16, data: &[u8]) {
    let mut guard = LOGGER.lock().unwrap();
    let Some(writer) = guard.as_mut() else { return };

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or(0);

    // Header line: timestamp proto src_port dst_port len
    let _ = writeln!(writer, "# {ts} {proto} {src_port} {dst_port} {}", data.len());

    // Hex dump: 32 bytes per line
    for (i, chunk) in data.chunks(32).enumerate() {
        let offset = i * 32;
        let hex: String = chunk.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
        let ascii: String = chunk
            .iter()
            .map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' })
            .collect();
        let _ = writeln!(writer, "{offset:06x}  {hex:<95}  {ascii}");
    }
    let _ = writeln!(writer);
    let _ = writer.flush();
}

pub fn log_path() -> std::path::PathBuf {
    // Use $XDG_DATA_HOME or $HOME/.local/share without extra dependency
    let base = std::env::var("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            std::path::PathBuf::from(home).join(".local").join("share")
        });
    base.join("bpsr").join("packets.log")
}
