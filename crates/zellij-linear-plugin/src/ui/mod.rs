pub mod detail;
pub mod help;
pub mod list;
pub mod text;

use crate::state::{PluginMode, State, View};

pub fn render(state: &mut State, rows: usize, cols: usize) {
    // Detail mode is a separate plugin instance — it ignores `view` and
    // always renders the issue detail (or its loading/error state).
    if matches!(state.mode, PluginMode::Detail) {
        detail::render(state, rows, cols);
        return;
    }
    match state.view {
        View::Help => help::render(rows, cols),
        View::List => list::render(state, rows, cols),
    }
}
