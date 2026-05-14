use crate::dps_state::DpsState;
use core::{
    module::{Module, ModuleContext},
    types::EntityId,
};
use game::GameEvent;
use std::collections::HashMap;

pub struct DpsMeterModule {
    state:           DpsState,
    names:           HashMap<EntityId, String>,
    classes:         HashMap<EntityId, u32>,
    pending:         Vec<GameEvent>,
    selected_enc:    Option<usize>,
    dps_window_secs: f64,
}

impl DpsMeterModule {
    pub fn new(encounter_timeout_secs: u32) -> Self {
        Self {
            state:           DpsState::new(encounter_timeout_secs),
            names:           HashMap::new(),
            classes:         HashMap::new(),
            pending:         Vec::new(),
            selected_enc:    None,
            dps_window_secs: 3.0,
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

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn update(&mut self, ctx: &ModuleContext) {
        self.dps_window_secs = ctx.config.dps_window_secs as f64;
        self.state.tick();

        for event in self.pending.drain(..) {
            match &event {
                GameEvent::EntityName { id, name, class } => {
                    if !name.is_empty() {
                        self.names.insert(*id, name.clone());
                    }
                    if let Some(c) = class {
                        self.classes.insert(*id, *c);
                    }
                    if let Some(enc) = &mut self.state.active {
                        if let Some(meter) = enc.players.get_mut(id) {
                            if !name.is_empty() { meter.player_name = name.clone(); }
                        }
                    }
                    for enc in &mut self.state.past {
                        if let Some(meter) = enc.players.get_mut(id) {
                            if !name.is_empty() { meter.player_name = name.clone(); }
                        }
                    }
                }
                GameEvent::EntityDespawn { id } => {
                    self.names.remove(id);
                    self.classes.remove(id);
                }
                GameEvent::Combat(_) => {
                    let names = &self.names;
                    self.state.apply_event(&event, |id| {
                        names.get(&id).cloned()
                            .unwrap_or_else(|| format!("Entity {:x}", id.0))
                    });
                }
                _ => {}
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _egui_ctx: &egui::Context) {
        let enc_data = match self.selected_enc {
            None    => self.state.active.as_ref(),
            Some(i) => self.state.past.get(i),
        }.map(|enc| {
            let elapsed    = enc.elapsed().as_secs_f64();
            let start_time = enc.start_time;
            let players: Vec<_> = enc.players_by_damage().into_iter().cloned().collect();
            let max_dmg = players.first().map(|p| p.total_damage).unwrap_or(1);
            (elapsed, start_time, players, max_dmg)
        });

        match enc_data {
            None => {
                ui.centered_and_justified(|ui| {
                    ui.label("No active encounter. Start combat to begin tracking.");
                });
            }
            Some((elapsed, start_time, players, max_dmg)) => {
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
                            let dps = player.current_dps(start_time, self.dps_window_secs);
                            let bar_frac = player.total_damage as f32 / max_dmg as f32;

                            ui.label(format!("{}", rank + 1));

                            ui.vertical(|ui| {
                                ui.label(&player.player_name);
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width().max(80.0), 4.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(rect, 0.0, egui::Color32::DARK_GRAY);
                                let mut bar = rect;
                                bar.set_right(rect.left() + rect.width() * bar_frac);
                                ui.painter().rect_filled(bar, 0.0, egui::Color32::from_rgb(70, 130, 180));
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
