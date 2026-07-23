use core::{module::{Module, ModuleContext}};
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

// ── Protocol types ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct WsEnvelope {
    #[serde(rename = "type")]
    msg_type: String,
    // full_update / boss_update share same boss field names
    bosses: Option<Vec<BossPayload>>,
    // single-boss update
    id: Option<String>,
    name: Option<String>,
    region: Option<String>,
    status: Option<String>,
    last_death_ts: Option<i64>,
    respawn_secs: Option<u64>,
}

#[derive(Deserialize)]
struct BossPayload {
    id: String,
    name: String,
    region: String,
    status: String,
    last_death_ts: Option<i64>,
    respawn_secs: u64,
}

// ── Domain types ──────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
pub enum BossStatus { Alive, Dead, Unknown }

impl BossStatus {
    fn from_str(s: &str) -> Self {
        match s { "alive" => Self::Alive, "dead" => Self::Dead, _ => Self::Unknown }
    }
    fn label(&self) -> &'static str {
        match self { Self::Alive => "Alive", Self::Dead => "Dead", Self::Unknown => "?" }
    }
    fn color(&self) -> egui::Color32 {
        match self {
            Self::Alive   => egui::Color32::from_rgb(80, 200, 80),
            Self::Dead    => egui::Color32::from_rgb(220, 80, 80),
            Self::Unknown => egui::Color32::GRAY,
        }
    }
}

#[derive(Clone)]
pub struct BossEntry {
    pub id:            String,
    pub name:          String,
    pub region:        String,
    pub status:        BossStatus,
    pub last_death_ts: Option<i64>,
    pub respawn_secs:  u64,
}

impl From<BossPayload> for BossEntry {
    fn from(p: BossPayload) -> Self {
        Self {
            id:            p.id,
            name:          p.name,
            region:        p.region,
            status:        BossStatus::from_str(&p.status),
            last_death_ts: p.last_death_ts,
            respawn_secs:  p.respawn_secs,
        }
    }
}

// ── Thread message ────────────────────────────────────────────────────────────

enum WsMsg {
    Connected,
    Disconnected,
    Entries(Vec<BossEntry>),
}

// ── Module ────────────────────────────────────────────────────────────────────

pub struct BPTimerModule {
    bosses:          HashMap<String, BossEntry>,
    connected:       bool,
    current_ws_url:  String,
    report_url:      String,
    region_filter:   String,
    collapsed:       bool,

    msg_rx:     Option<Receiver<WsMsg>>,
    should_run: Arc<AtomicBool>,
}

impl BPTimerModule {
    pub fn new() -> Self {
        Self {
            bosses:         HashMap::new(),
            connected:      false,
            current_ws_url: String::new(),
            report_url:     String::new(),
            region_filter:  String::new(),
            collapsed:      false,
            msg_rx:         None,
            should_run:     Arc::new(AtomicBool::new(false)),
        }
    }

    fn start_thread(&mut self, ws_url: String) {
        self.should_run.store(false, Ordering::Relaxed);

        let flag  = Arc::new(AtomicBool::new(true));
        self.should_run = flag.clone();
        self.current_ws_url = ws_url.clone();

        let (tx, rx) = crossbeam_channel::unbounded::<WsMsg>();
        self.msg_rx = Some(rx);

        std::thread::Builder::new()
            .name("bptimer-ws".into())
            .spawn(move || ws_worker(ws_url, tx, flag))
            .ok();
    }

    fn stop_thread(&mut self) {
        self.should_run.store(false, Ordering::Relaxed);
        self.current_ws_url.clear();
        self.msg_rx = None;
        self.connected = false;
    }

    fn drain_messages(&mut self) {
        let Some(rx) = &self.msg_rx else { return };
        loop {
            match rx.try_recv() {
                Ok(WsMsg::Connected)    => { self.connected = true; }
                Ok(WsMsg::Disconnected) => { self.connected = false; }
                Ok(WsMsg::Entries(v))   => {
                    for e in v { self.bosses.insert(e.id.clone(), e); }
                }
                Err(TryRecvError::Empty)        => break,
                Err(TryRecvError::Disconnected) => {
                    self.connected = false;
                    self.msg_rx    = None;
                    break;
                }
            }
        }
    }
}

impl Default for BPTimerModule {
    fn default() -> Self { Self::new() }
}

impl Module for BPTimerModule {
    fn id(&self)   -> &'static str { "bptimer" }
    fn name(&self) -> &str         { "BPTimer" }
    fn icon(&self) -> &str         { egui_phosphor::regular::TIMER }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn update(&mut self, ctx: &ModuleContext) {
        let enabled = ctx.config.bptimer_enabled;
        let ws_url  = &ctx.config.bptimer_ws_url;

        if enabled && !ws_url.is_empty() {
            if *ws_url != self.current_ws_url {
                self.start_thread(ws_url.clone());
            }
        } else if !enabled && !self.current_ws_url.is_empty() {
            self.stop_thread();
        }

        self.report_url   = ctx.config.bptimer_report_url.clone();
        self.region_filter = ctx.config.bptimer_region.clone();

        self.drain_messages();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        // ── Header ────────────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            let arrow = if self.collapsed { egui_phosphor::regular::CARET_RIGHT } else { egui_phosphor::regular::CARET_DOWN };
            if ui.small_button(arrow).clicked() {
                self.collapsed = !self.collapsed;
            }
            ui.strong("BPTimer — Field Boss Spawns");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (dot_color, dot_label) = if !self.current_ws_url.is_empty() {
                    if self.connected {
                        (egui::Color32::from_rgb(80, 220, 80), "Connected")
                    } else {
                        (egui::Color32::from_rgb(220, 180, 40), "Reconnecting…")
                    }
                } else {
                    (egui::Color32::GRAY, "Disabled")
                };
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(10.0, 10.0), egui::Sense::hover()
                );
                ui.painter().circle_filled(rect.center(), 5.0, dot_color);
                ui.label(egui::RichText::new(dot_label).small().color(dot_color));
            });
        });

        if self.collapsed { return; }
        ui.separator();

        if self.current_ws_url.is_empty() {
            ui.label(
                egui::RichText::new("BPTimer disabled. Set WebSocket URL in Settings.")
                    .color(egui::Color32::GRAY)
            );
            return;
        }

        if self.bosses.is_empty() {
            ui.label(
                egui::RichText::new(if self.connected {
                    "Waiting for data…"
                } else {
                    "Connecting…"
                }).color(egui::Color32::GRAY)
            );
            return;
        }

        // ── Boss table ────────────────────────────────────────────────────────
        let now_ts  = chrono::Utc::now().timestamp();
        let region  = self.region_filter.clone();
        let rep_url = self.report_url.clone();

        let mut entries: Vec<&BossEntry> = self.bosses.values()
            .filter(|e| region.is_empty() || e.region.to_lowercase().contains(&region.to_lowercase()))
            .collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        // Track which boss to report (avoid borrow in closure)
        let mut report_id: Option<String> = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("bptimer_table")
                .num_columns(4)
                .striped(true)
                .min_col_width(60.0)
                .show(ui, |ui| {
                    ui.strong("Boss");
                    ui.strong("Region");
                    ui.strong("Status");
                    ui.strong("Spawn Timer");
                    ui.end_row();

                    for entry in entries {
                        let (timer_str, timer_color) = spawn_time_label(entry, now_ts);

                        let name_resp = ui.label(&entry.name);
                        ui.label(
                            egui::RichText::new(&entry.region)
                                .small()
                                .color(egui::Color32::from_rgb(160, 160, 180))
                        );
                        ui.label(
                            egui::RichText::new(entry.status.label())
                                .color(entry.status.color())
                                .strong()
                        );
                        ui.label(egui::RichText::new(&timer_str).color(timer_color));

                        // Right-click context menu on name cell
                        name_resp.context_menu(|ui| {
                            ui.label(egui::RichText::new(&entry.name).strong());
                            ui.separator();
                            if ui.button("Report Dead Now").clicked() {
                                report_id = Some(entry.id.clone());
                                ui.close_menu();
                            }
                        });

                        ui.end_row();
                    }
                });
        });

        // Fire report outside borrow
        if let Some(id) = report_id {
            let url = rep_url.clone();
            std::thread::spawn(move || {
                if let Err(e) = post_report(&url, &id) {
                    tracing::warn!("bptimer report failed: {e}");
                }
            });
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn spawn_time_label(entry: &BossEntry, now_ts: i64) -> (String, egui::Color32) {
    match &entry.status {
        BossStatus::Alive => {
            ("Alive".to_string(), egui::Color32::from_rgb(80, 200, 80))
        }
        BossStatus::Dead => {
            if let Some(death_ts) = entry.last_death_ts {
                let respawn_ts = death_ts + entry.respawn_secs as i64;
                let remaining  = respawn_ts - now_ts;
                if remaining > 0 {
                    let m = remaining / 60;
                    let s = remaining % 60;
                    (format!("{}m {:02}s", m, s), egui::Color32::from_rgb(230, 100, 80))
                } else {
                    let overdue = (-remaining) as u64;
                    let m = overdue / 60;
                    let s = overdue % 60;
                    (
                        format!("+{}m {:02}s OVERDUE", m, s),
                        egui::Color32::from_rgb(255, 210, 40),
                    )
                }
            } else {
                ("Dead (time unknown)".to_string(), egui::Color32::from_rgb(200, 100, 80))
            }
        }
        BossStatus::Unknown => ("?".to_string(), egui::Color32::GRAY),
    }
}

fn post_report(url: &str, boss_id: &str) -> anyhow::Result<()> {
    if url.is_empty() { return Ok(()); }
    let body = serde_json::json!({
        "boss_id":   boss_id,
        "killed_at": chrono::Utc::now().timestamp(),
        "reporter":  "BPSR-AIO-Tools",
    });
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(url)
        .json(&body)
        .timeout(Duration::from_secs(10))
        .send()?;
    if !resp.status().is_success() {
        tracing::warn!("bptimer report returned {}", resp.status());
    }
    Ok(())
}

// ── WebSocket worker thread ───────────────────────────────────────────────────

fn ws_worker(ws_url: String, tx: Sender<WsMsg>, should_run: Arc<AtomicBool>) {
    while should_run.load(Ordering::Relaxed) {
        tracing::debug!("bptimer: connecting to {ws_url}");
        match tungstenite::connect(&ws_url) {
            Ok((mut socket, _)) => {
                tracing::info!("bptimer: connected");
                tx.send(WsMsg::Connected).ok();
                loop {
                    if !should_run.load(Ordering::Relaxed) { break; }
                    match socket.read() {
                        Ok(tungstenite::Message::Text(text)) => {
                            if let Some(entries) = parse_payload(&text) {
                                tx.send(WsMsg::Entries(entries)).ok();
                            }
                        }
                        Ok(tungstenite::Message::Ping(data)) => {
                            let _ = socket.write(tungstenite::Message::Pong(data));
                            let _ = socket.flush();
                        }
                        Ok(tungstenite::Message::Close(_)) => break,
                        Err(e) => {
                            tracing::debug!("bptimer ws read error: {e}");
                            break;
                        }
                        _ => {}
                    }
                }
                let _ = socket.close(None);
                tracing::info!("bptimer: disconnected");
            }
            Err(e) => {
                tracing::warn!("bptimer ws connect failed: {e}");
            }
        }

        if should_run.load(Ordering::Relaxed) {
            tx.send(WsMsg::Disconnected).ok();
            std::thread::sleep(Duration::from_secs(5));
        }
    }
}

fn parse_payload(text: &str) -> Option<Vec<BossEntry>> {
    let env: WsEnvelope = serde_json::from_str(text).ok()?;
    match env.msg_type.as_str() {
        "full_update" | "spawn_update" => {
            let entries = env.bosses?.into_iter().map(BossEntry::from).collect();
            Some(entries)
        }
        "boss_update" => {
            let entry = BossEntry {
                id:            env.id?,
                name:          env.name.unwrap_or_default(),
                region:        env.region.unwrap_or_default(),
                status:        BossStatus::from_str(env.status.as_deref().unwrap_or("")),
                last_death_ts: env.last_death_ts,
                respawn_secs:  env.respawn_secs.unwrap_or(3600),
            };
            Some(vec![entry])
        }
        _ => None,
    }
}
