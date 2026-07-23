use core::module::{Module, ModuleContext};
use game::event::{ChatChannel, GameEvent};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const MAX_MESSAGES: usize = 500;
const WARNING_TTL: Duration = Duration::from_secs(10);
const COUNTDOWN_WARN_SECS: f64 = 5.0;

pub struct ChatMessage {
    pub received_at: Instant,
    pub channel:     ChatChannel,
    pub sender_name: String,
    pub text:        String,
}

struct RaidWarning {
    text: String,
    at:   Instant,
}

struct Countdown {
    ends_at: Instant,
    label:   String,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum ChannelTab {
    All,
    World,
    Scene,
    Team,
    Union,
    Private,
    Group,
}

impl ChannelTab {
    fn label(self) -> &'static str {
        match self {
            Self::All     => "All",
            Self::World   => "World",
            Self::Scene   => "Scene",
            Self::Team    => "Team",
            Self::Union   => "Union",
            Self::Private => "Private",
            Self::Group   => "Group",
        }
    }

    fn matches(self, ch: &ChatChannel) -> bool {
        match self {
            Self::All     => true,
            Self::World   => matches!(ch, ChatChannel::World),
            Self::Scene   => matches!(ch, ChatChannel::Scene),
            Self::Team    => matches!(ch, ChatChannel::Team),
            Self::Union   => matches!(ch, ChatChannel::Union),
            Self::Private => matches!(ch, ChatChannel::Private),
            Self::Group   => matches!(ch, ChatChannel::Group),
        }
    }
}

pub struct ChatModule {
    messages:        VecDeque<ChatMessage>,
    pending:         Vec<GameEvent>,
    active_tab:      ChannelTab,
    keyword:         String,
    warnings:        Vec<RaidWarning>,
    countdown:       Option<Countdown>,
    scroll_to_bottom: bool,
}

impl ChatModule {
    pub fn new() -> Self {
        Self {
            messages:         VecDeque::new(),
            pending:          Vec::new(),
            active_tab:       ChannelTab::All,
            keyword:          String::new(),
            warnings:         Vec::new(),
            countdown:        None,
            scroll_to_bottom: false,
        }
    }

    pub fn push_event(&mut self, event: GameEvent) {
        self.pending.push(event);
    }

    fn ingest(&mut self, channel: ChatChannel, sender_name: String, text: String) {
        // Raid warning: /rw <text>
        if let Some(warn_text) = text.strip_prefix("/rw ") {
            let warn_text = warn_text.to_string();
            core::notification::notify("Raid Warning", &warn_text, true);
            self.warnings.push(RaidWarning { text: warn_text, at: Instant::now() });
        }

        // Countdown: /ct <secs> [label]  or  /countdown <secs> [label]
        let ct_body = text.strip_prefix("/ct ").or_else(|| text.strip_prefix("/countdown "));
        if let Some(rest) = ct_body {
            let mut parts = rest.splitn(2, ' ');
            if let Some(secs_str) = parts.next() {
                if let Ok(secs) = secs_str.parse::<u64>() {
                    let label = parts.next().unwrap_or("Countdown").to_string();
                    let msg = format!("Countdown: {} — {}", label, format_dur(secs as f64));
                    core::notification::notify("Countdown started", &msg, false);
                    self.countdown = Some(Countdown {
                        ends_at: Instant::now() + Duration::from_secs(secs),
                        label,
                    });
                }
            }
        }

        self.messages.push_back(ChatMessage {
            received_at: Instant::now(),
            channel,
            sender_name,
            text,
        });
        if self.messages.len() > MAX_MESSAGES {
            self.messages.pop_front();
        }
        self.scroll_to_bottom = true;
    }
}

impl Default for ChatModule {
    fn default() -> Self { Self::new() }
}

impl Module for ChatModule {
    fn id(&self)   -> &'static str { "chat" }
    fn name(&self) -> &str         { "Chat" }
    fn icon(&self) -> &str         { egui_phosphor::regular::CHAT_CIRCLE }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn update(&mut self, _ctx: &ModuleContext) {
        // Drain pending events
        let chats: Vec<_> = self.pending.drain(..)
            .filter_map(|e| {
                if let GameEvent::Chat { channel, sender_name, text, .. } = e {
                    Some((channel, sender_name, text))
                } else {
                    None
                }
            })
            .collect();
        for (channel, sender_name, text) in chats {
            self.ingest(channel, sender_name, text);
        }

        // Expire warnings
        self.warnings.retain(|w| w.at.elapsed() < WARNING_TTL);

        // Expire finished countdown
        if let Some(cd) = &self.countdown {
            if Instant::now() >= cd.ends_at {
                self.countdown = None;
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _egui_ctx: &egui::Context) {
        // ── Active countdown banner ──────────────────────────────────────────
        if let Some(cd) = &self.countdown {
            let remaining = cd.ends_at.saturating_duration_since(Instant::now()).as_secs_f64();
            let color = if remaining <= COUNTDOWN_WARN_SECS {
                egui::Color32::from_rgb(220, 60, 60)
            } else {
                egui::Color32::from_rgb(60, 180, 120)
            };
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{} {} — {}", egui_phosphor::regular::TIMER, cd.label, format_dur(remaining)))
                        .strong()
                        .size(18.0)
                        .color(color),
                );
            });
            ui.separator();
        }

        // ── Active raid warnings ─────────────────────────────────────────────
        if !self.warnings.is_empty() {
            for w in &self.warnings {
                ui.label(
                    egui::RichText::new(format!("{} {}", egui_phosphor::regular::WARNING, w.text))
                        .strong()
                        .color(egui::Color32::from_rgb(240, 160, 30)),
                );
            }
            ui.separator();
        }

        // ── Channel tabs ────────────────────────────────────────────────────
        let tabs = [
            ChannelTab::All,
            ChannelTab::World,
            ChannelTab::Scene,
            ChannelTab::Team,
            ChannelTab::Union,
            ChannelTab::Private,
            ChannelTab::Group,
        ];
        ui.horizontal(|ui| {
            for tab in tabs {
                if ui.selectable_label(self.active_tab == tab, tab.label()).clicked() {
                    self.active_tab = tab;
                    self.scroll_to_bottom = true;
                }
            }
        });

        // ── Keyword filter ───────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label("Filter:");
            ui.text_edit_singleline(&mut self.keyword);
            if !self.keyword.is_empty() && ui.small_button(egui_phosphor::regular::X).clicked() {
                self.keyword.clear();
            }
        });
        ui.separator();

        // ── Message list ─────────────────────────────────────────────────────
        let kw = self.keyword.to_lowercase();
        let scroll_to_bottom = self.scroll_to_bottom;
        self.scroll_to_bottom = false;

        egui::ScrollArea::vertical()
            .id_salt("chat_scroll")
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for msg in &self.messages {
                    if !self.active_tab.matches(&msg.channel) { continue; }
                    if !kw.is_empty()
                        && !msg.text.to_lowercase().contains(&kw)
                        && !msg.sender_name.to_lowercase().contains(&kw)
                    {
                        continue;
                    }

                    ui.horizontal_wrapped(|ui| {
                        // Channel badge
                        let (ch_label, ch_color) = channel_style(&msg.channel);
                        ui.label(
                            egui::RichText::new(ch_label)
                                .small()
                                .color(ch_color)
                                .monospace(),
                        );

                        // Sender — click to copy name
                        let sender_resp = ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!("[{}]", msg.sender_name))
                                    .small()
                                    .color(egui::Color32::from_rgb(150, 190, 240)),
                            )
                            .sense(egui::Sense::click()),
                        );
                        if sender_resp.clicked() {
                            ui.output_mut(|o| o.copied_text = msg.sender_name.clone());
                        }
                        sender_resp.on_hover_text("Click to copy name");

                        ui.label(egui::RichText::new(&msg.text).small());
                    });
                }

                if scroll_to_bottom {
                    ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                }
            });
    }
}

fn channel_style(ch: &ChatChannel) -> (&'static str, egui::Color32) {
    match ch {
        ChatChannel::World   => ("[W]", egui::Color32::from_rgb(180, 220, 255)),
        ChatChannel::Scene   => ("[S]", egui::Color32::from_rgb(160, 240, 160)),
        ChatChannel::Team    => ("[T]", egui::Color32::from_rgb(255, 220, 100)),
        ChatChannel::Union   => ("[G]", egui::Color32::from_rgb(200, 160, 255)),
        ChatChannel::Private => ("[P]", egui::Color32::from_rgb(255, 180, 180)),
        ChatChannel::Group   => ("[Gr]", egui::Color32::from_rgb(200, 200, 200)),
        ChatChannel::Other(_)=> ("[?]", egui::Color32::GRAY),
    }
}

fn format_dur(secs: f64) -> String {
    let s = secs as u64;
    if s >= 60 {
        format!("{:02}:{:02}", s / 60, s % 60)
    } else {
        format!("{}s", s)
    }
}
