/// Check capture permissions. Returns an error message if missing.
pub fn check_capabilities() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        if std::env::var("USER").map(|u| u == "root").unwrap_or(false) {
            return None;
        }
        // Check CAP_NET_RAW via /proc/self/status
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("CapEff:") {
                    if let Some(hex) = line.split_whitespace().nth(1) {
                        if let Ok(cap) = u64::from_str_radix(hex, 16) {
                            // CAP_NET_RAW = bit 13
                            if cap & (1 << 13) != 0 {
                                return None;
                            }
                        }
                    }
                }
            }
        }
        return Some(
            "Missing CAP_NET_RAW. Run: sudo setcap cap_net_raw+eip ./target/release/bpsr-aio-tools"
                .to_string(),
        );
    }

    #[cfg(target_os = "windows")]
    {
        let npcap_path = "C:\\Windows\\System32\\Npcap\\wpcap.dll";
        if !std::path::Path::new(npcap_path).exists() {
            return Some("Npcap not found. Install from https://npcap.com".to_string());
        }
        None
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    None
}
