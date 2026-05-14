use egui::{Color32, Ui};

pub struct StatusInfo<'a> {
    pub interface:      Option<&'a str>,
    pub capturing:      bool,
    pub encounter_secs: Option<f64>,
    pub events_per_sec: f64,
}

pub fn statusbar(ui: &mut Ui, info: &StatusInfo) {
    ui.horizontal(|ui| {
        ui.separator();
        match (info.capturing, info.interface) {
            (true, Some(iface)) => {
                ui.colored_label(Color32::from_rgb(80, 200, 100), "●");
                ui.label(format!("Capturing: {iface}"));
            }
            _ => {
                ui.colored_label(Color32::from_rgb(200, 80, 80), "●");
                ui.label("Not capturing");
            }
        }
        ui.separator();
        if let Some(secs) = info.encounter_secs {
            ui.label(format!(
                "Encounter: {:02}:{:02}",
                secs as u64 / 60,
                secs as u64 % 60
            ));
        } else {
            ui.label("No encounter");
        }
        ui.separator();
        if info.events_per_sec > 0.0 {
            ui.label(format!("{:.0} events/s", info.events_per_sec));
        }
    });
}
