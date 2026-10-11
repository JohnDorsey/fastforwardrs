//! Telling what kind of click a response had.

use egui::Response;

/// Whether `response` was clicked twice quickly, or more than twice. (egui counts a third click soon
/// after two, or a double click soon after another click somewhere, as something else than a double
/// click, and either of those is as good as a double click here.)
pub fn is_a_double_click(response: &Response) -> bool {
    response.double_clicked() || response.triple_clicked()
}
