pub mod help;
pub mod list;
pub mod text;

use crate::state::{State, View};

pub fn render(state: &State, rows: usize, cols: usize) {
    match state.view {
        View::Help => help::render(rows, cols),
        View::List => list::render(state, rows, cols),
    }
}
