use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const DEDUP_TTL: Duration = Duration::from_secs(30);
const DEDUP_MAX: usize    = 5;

static RECENT_HASHES: Mutex<Option<VecDeque<(u64, Instant)>>> = Mutex::new(None);

pub struct PlayerSummary {
    pub name:            String,
    pub class_name:      &'static str,
    pub spec:            Option<String>,
    pub total_damage:    u64,
    pub dps:             f64,
    pub crit_pct:        f64,
    pub crit_damage_pct: Option<f64>,
    pub luck_pct:        Option<f64>,
    pub damage_taken:    u64,
    pub total_healing:   u64,
    pub heal_ps:         f64,
    pub ability_score:   Option<u32>,
    pub season_strength: Option<u32>,
}

pub struct DiscordReport {
    pub scene_name:    String,
    pub duration_secs: f64,
    pub players:       Vec<PlayerSummary>,
    pub total_damage:  u64,
    // Used for dedup key alongside scene_name
    pub started_at_secs: i64,
}

pub fn send_report_async(webhook_url: String, report: DiscordReport) {
    std::thread::spawn(move || {
        if let Err(e) = send_blocking(&webhook_url, &report) {
            tracing::warn!("discord webhook failed: {e}");
        }
    });
}

fn is_duplicate(report: &DiscordReport) -> bool {
    let key = hash_report(report);
    let mut guard = RECENT_HASHES.lock().unwrap();
    let deque = guard.get_or_insert_with(VecDeque::new);

    // Expire old entries
    deque.retain(|(_, at)| at.elapsed() < DEDUP_TTL);

    if deque.iter().any(|(h, _)| *h == key) {
        return true;
    }
    deque.push_back((key, Instant::now()));
    if deque.len() > DEDUP_MAX {
        deque.pop_front();
    }
    false
}

fn hash_report(report: &DiscordReport) -> u64 {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;
    let mut h = DefaultHasher::new();
    report.scene_name.hash(&mut h);
    report.started_at_secs.hash(&mut h);
    report.players.len().hash(&mut h);
    h.finish()
}

fn send_blocking(url: &str, report: &DiscordReport) -> anyhow::Result<()> {
    if url.is_empty() { return Ok(()); }
    if is_duplicate(report) {
        tracing::debug!("discord: skipping duplicate report");
        return Ok(());
    }

    let body = build_body(report);
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(url)
        .json(&body)
        .timeout(Duration::from_secs(10))
        .send()?;

    if !resp.status().is_success() {
        tracing::warn!("discord webhook returned {}", resp.status());
    }
    Ok(())
}

fn build_body(report: &DiscordReport) -> serde_json::Value {
    let scene = if report.scene_name.is_empty() { "Unknown Zone" } else { &report.scene_name };
    let dur   = format_duration(report.duration_secs);

    let mut lines = Vec::new();
    for (i, p) in report.players.iter().enumerate() {
        // Class + spec tag
        let class_spec = if let Some(s) = &p.spec {
            format!("{} · {}", p.class_name, s)
        } else {
            p.class_name.to_string()
        };

        // AS / SS suffix
        let mut as_ss = String::new();
        if let Some(gs) = p.ability_score {
            as_ss.push_str(&format!(" AS {gs}"));
        }
        if let Some(ss) = p.season_strength.filter(|&v| v > 0) {
            as_ss.push_str(&format!(" SS {ss}"));
        }

        // Combat stat parts
        let mut stat_parts = vec![
            format!("{} DMG", fmt_damage(p.total_damage)),
            format!("{} DPS", fmt_damage(p.dps as u64)),
            format!("{:.1}% Crit", p.crit_pct),
        ];
        if let Some(cd) = p.crit_damage_pct {
            stat_parts.push(format!("{:.1}% CritDmg", cd));
        }
        if let Some(lk) = p.luck_pct {
            stat_parts.push(format!("{:.1}% Luck", lk));
        }

        lines.push(format!(
            "{}. **{}** `{}`{} — {}",
            i + 1, p.name, class_spec, as_ss, stat_parts.join("  ")
        ));

        // Line 2: taken + heal (only if non-zero)
        if p.damage_taken > 0 || p.total_healing > 0 {
            lines.push(format!(
                "   ↓ {} taken  ♥ {} heal ({}/s)",
                fmt_damage(p.damage_taken),
                fmt_damage(p.total_healing),
                fmt_damage(p.heal_ps as u64),
            ));
        }
    }
    let description = lines.join("\n");
    let title = format!("{scene} — {dur}");

    serde_json::json!({
        "username": "BPSR AIO Tools",
        "embeds": [{
            "title": title,
            "description": description,
            "color": 3_066_993,   // teal
            "footer": { "text": format!("Total: {} DMG", fmt_damage(report.total_damage)) }
        }]
    })
}

fn format_duration(secs: f64) -> String {
    let s = secs as u64;
    if s >= 60 {
        format!("{}m {:02}s", s / 60, s % 60)
    } else {
        format!("{}s", s)
    }
}

fn fmt_damage(dmg: u64) -> String {
    if dmg >= 1_000_000 {
        format!("{:.2}M", dmg as f64 / 1_000_000.0)
    } else if dmg >= 1_000 {
        format!("{:.1}K", dmg as f64 / 1_000.0)
    } else {
        format!("{dmg}")
    }
}
