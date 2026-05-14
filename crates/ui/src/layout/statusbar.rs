use egui::Ui;

pub struct StatusInfo<'a> {
    pub interface:      Option<&'a str>,
    pub capturing:      bool,
    pub encounter_secs: Option<f64>,
    pub events_per_sec: f64,
}

pub fn statusbar(ui: &mut Ui, info: &StatusInfo) {
    ui.horizontal(|ui| {
        if info.events_per_sec > 0.0 {
            ui.separator();
            ui.label(format!("{:.0} events/s", info.events_per_sec));
        }
    });
}
