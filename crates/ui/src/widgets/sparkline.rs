use egui::Ui;
use egui_plot::{Line, Plot, PlotPoints};

/// Render a small DPS sparkline. Points: (elapsed_secs, dps).
pub fn sparkline(ui: &mut Ui, id: &str, points: &[(f64, f64)], height: f32) {
    let plot_points: PlotPoints = points
        .iter()
        .map(|&(x, y)| [x, y])
        .collect();

    Plot::new(id)
        .height(height)
        .show_axes([false, false])
        .show_grid(false)
        .allow_zoom(false)
        .allow_drag(false)
        .show(ui, |plot_ui| {
            plot_ui.line(Line::new(plot_points));
        });
}
