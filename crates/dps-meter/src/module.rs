use crate::dps_state::DpsState;
use core::module::{Module, ModuleContext};
use game::GameEvent;

pub struct DpsMeterModule {
    state:        DpsState,
    pending:      Vec<GameEvent>,
    selected_enc: Option<usize>, // index into past encounters (None = active)
}

impl DpsMeterModule {
    pub fn new(encounter_timeout_secs: u32) -> Self {
        Self {
            state:        DpsState::new(encounter_timeout_secs),
            pending:      Vec::new(),
            selected_enc: None,
        }
    }

    pub fn push_event(&mut self, event: GameEvent) {
        self.pending.push(event);
    }
}

impl Module for DpsMeterModule {
    fn id(&self)   -> &'static str { "dps-meter" }
    fn name(&self) -> &str         { "DPS Meter" }
    fn icon(&self) -> &str         { "📊" }

    fn update(&mut self, ctx: &ModuleContext) {
        self.state.tick();
        // Events are fed via push_event from app.rs before update() is called
    }

    fn ui(&mut self, ui: &mut egui::Ui, _egui_ctx: &egui::Context) {
        // Collect display data first to avoid borrow conflicts with closures
        let enc_data = match self.selected_enc {
            None    => self.state.active.as_ref(),
            Some(i) => self.state.past.get(i),
        }.map(|enc| {
            let elapsed = enc.elapsed().as_secs_f64();
            let players: Vec<_> = enc.players_by_damage().into_iter().cloned().collect();
            let max_dmg = players.first().map(|p| p.total_damage).unwrap_or(1);
            (elapsed, players, max_dmg)
        });

        match enc_data {
            None => {
                ui.centered_and_justified(|ui| {
                    ui.label("No active encounter. Start combat to begin tracking.");
                });
            }
            Some((elapsed, players, max_dmg)) => {
                let mut do_reset = false;

                ui.horizontal(|ui| {
                    ui.strong("Encounter");
                    ui.label(format!(
                        "{:02}:{:02}",
                        elapsed as u64 / 60,
                        elapsed as u64 % 60
                    ));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Reset").clicked() {
                            do_reset = true;
                        }
                    });
                });
                if do_reset {
                    self.state.reset();
                    return;
                }
                ui.separator();

                egui::Grid::new("dps_table")
                    .num_columns(5)
                    .striped(true)
                    .min_col_width(60.0)
                    .show(ui, |ui| {
                        ui.strong("#");
                        ui.strong("Player");
                        ui.strong("Damage");
                        ui.strong("DPS");
                        ui.strong("Crit%");
                        ui.end_row();

                        for (rank, player) in players.iter().enumerate() {
                            let dps = if elapsed > 0.1 {
                                player.total_damage as f64 / elapsed
                            } else {
                                0.0
                            };
                            let bar_frac = player.total_damage as f32 / max_dmg as f32;

                            ui.label(format!("{}", rank + 1));

                            // Name + damage bar
                            ui.vertical(|ui| {
                                ui.label(&player.player_name);
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width().max(80.0), 4.0),
                                    egui::Sense::hover(),
                                );
                                let fill = egui::Color32::from_rgb(70, 130, 180);
                                ui.painter().rect_filled(rect, 0.0, egui::Color32::DARK_GRAY);
                                let mut bar = rect;
                                bar.set_right(rect.left() + rect.width() * bar_frac);
                                ui.painter().rect_filled(bar, 0.0, fill);
                            });

                            ui.label(fmt_damage(player.total_damage));
                            ui.label(format!("{:.0}", dps));
                            ui.label(format!("{:.1}%", player.crit_rate() * 100.0));
                            ui.end_row();
                        }
                    });
            }
        }
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
