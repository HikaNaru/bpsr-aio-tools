mod app;

use app::BpsrApp;
use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    if let Some(warning) = capture::platform::check_capabilities() {
        eprintln!("WARNING: {warning}");
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("BPSR AIO Tools")
            .with_inner_size([620.0, 700.0])
            .with_min_inner_size([400.0, 300.0]),
        ..Default::default()
    };

    eframe::run_native(
        "BPSR AIO Tools",
        native_options,
        Box::new(|cc| Ok(Box::new(BpsrApp::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe: {e}"))
}
