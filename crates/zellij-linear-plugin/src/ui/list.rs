//! Compact list rendering tuned for a ~25–40-column sidebar.

use linear_client::types::Issue;

use crate::state::State;
use crate::ui::text::truncate;

const KEY_HINT: &str = "[c] claude  [r] refresh  [?] help";

pub fn render(state: &State, rows: usize, cols: usize) {
    let header = build_header(state, cols);
    println!("{header}");
    let sep = "─".repeat(cols.max(1));
    println!("{sep}");

    let footer_rows = if state.last_error.is_some() || state.current_status().is_some() {
        3 // hint line + status line + bottom separator
    } else {
        2
    };
    let body_rows = rows.saturating_sub(2 + footer_rows).max(1);

    if !state.permissions_granted {
        println!("Awaiting permissions…");
        return;
    }
    if state.access_token.is_none() {
        println!("Authenticating…");
        if let Some(err) = state.last_error.as_deref() {
            println!("{err}");
        }
        return;
    }
    if let Some(err) = state.config_error.as_deref() {
        println!("Config error:");
        for line in err.lines().take(body_rows.saturating_sub(1)) {
            println!("  {line}");
        }
        return;
    }
    if state.config.is_none() {
        println!("No .linear.toml here.");
        println!("Run in this folder:");
        println!("  zellij-linear init");
        return;
    }
    if !state.initial_load_done {
        println!("Loading issues…");
        return;
    }
    if state.issues.is_empty() {
        println!("No matching issues.");
    } else {
        render_issue_rows(state, cols, body_rows);
    }

    let sep = "─".repeat(cols.max(1));
    println!("{sep}");
    if let Some(msg) = state.current_status() {
        println!("{}", truncate(msg, cols));
    } else if let Some(err) = state.last_error.as_deref() {
        println!("{}", truncate(err, cols));
    }
    println!("{}", truncate(KEY_HINT, cols));
}

fn render_issue_rows(state: &State, cols: usize, body_rows: usize) {
    let offset = viewport_offset(state.selected_idx, state.issues.len(), body_rows);
    let visible = state.issues.iter().enumerate().skip(offset).take(body_rows);
    for (idx, issue) in visible {
        let selected = idx == state.selected_idx;
        let row = format_issue_row(issue, cols, selected);
        println!("{row}");
    }
}

fn format_issue_row(issue: &Issue, cols: usize, selected: bool) -> String {
    let icon = priority_icon(issue.priority);
    let raw = format!("{icon} {:<8} {}", issue.identifier, issue.title);
    let truncated = truncate(&raw, cols);
    if selected {
        format!("\u{1b}[7m{truncated}\u{1b}[0m") // reverse video
    } else {
        truncated.to_string()
    }
}

fn priority_icon(priority: f64) -> &'static str {
    // Linear returns integral priorities 0..=4 as Float for forward
    // compatibility; we truncate via `as u8` — anything outside the known
    // range falls through to the default glyph.
    match priority as u8 {
        1 => "!", // urgent
        2 => "▲", // high
        3 => "·", // normal
        4 => "▽", // low
        _ => " ",
    }
}

fn viewport_offset(selected: usize, total: usize, body_rows: usize) -> usize {
    if total <= body_rows {
        return 0;
    }
    if selected < body_rows / 2 {
        return 0;
    }
    let max_offset = total - body_rows;
    (selected.saturating_sub(body_rows / 2)).min(max_offset)
}

fn build_header(state: &State, cols: usize) -> String {
    let name = state
        .config
        .as_ref()
        .and_then(|c| c.project_name.clone())
        .or_else(|| state.config.as_ref().and_then(|c| c.project_id.clone()))
        .unwrap_or_else(|| "Linear".to_string());
    let count = if state.initial_load_done {
        format!("{}", state.issues.len())
    } else {
        "…".to_string()
    };
    let line = format!("{name}  ({count})");
    truncate(&line, cols).to_string()
}
