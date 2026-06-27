pub fn notify(summary: &str, body: &str, urgent: bool) {
    let urgency = if urgent { "critical" } else { "normal" };
    let _ = std::process::Command::new("notify-send")
        .args(["--urgency", urgency, summary, body])
        .spawn();
}
