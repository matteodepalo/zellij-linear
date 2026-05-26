//! Floating-pane detail view — full issue with comments.
//!
//! Zellij's `Ctrl+s` scroll mode does not capture plugin output
//! ([#4996](https://github.com/zellij-org/zellij/issues/4996)), so we
//! window the body ourselves. The render pass publishes the current
//! max-scroll back into [`State`] so the key/mouse handlers can clamp
//! their moves without re-running the line wrap.

use linear_client::types::IssueDetail;

use crate::state::State;
use crate::ui::text::{truncate, wrap};

const KEY_HINT: &str = "[j/k] scroll [c/C] claude [y/Y] copy [o] open [q] close";

pub fn render(state: &mut State, rows: usize, cols: usize) {
    let body_rows = rows.saturating_sub(3).max(1); // title + sep + footer
    let sep = "─".repeat(cols.max(1));

    let title_line = build_title(state, cols);
    println!("{title_line}");
    println!("{sep}");

    if !state.permissions_granted {
        state.detail_max_scroll = 0;
        state.detail_scroll = 0;
        println!("Awaiting permissions…");
        for _ in 1..body_rows {
            println!();
        }
        println!("{sep}");
        println!("{}", truncate(KEY_HINT, cols));
        return;
    }
    if state.access_token.is_none() {
        state.detail_max_scroll = 0;
        state.detail_scroll = 0;
        println!("Authenticating…");
        if let Some(err) = state.last_error.as_deref() {
            println!("{}", truncate(err, cols));
        }
        for _ in 2..body_rows {
            println!();
        }
        println!("{sep}");
        println!("{}", truncate(KEY_HINT, cols));
        return;
    }

    if let Some(issue) = state.detail_issue.as_ref() {
        let all = build_body_lines(issue, cols);
        let max_scroll = all.len().saturating_sub(body_rows);
        state.detail_max_scroll = max_scroll;
        // Re-clamp here too in case the pane shrank since the last keypress.
        if state.detail_scroll > max_scroll {
            state.detail_scroll = max_scroll;
        }
        let start = state.detail_scroll;
        let mut printed = 0;
        for line in all.iter().skip(start).take(body_rows) {
            println!("{line}");
            printed += 1;
        }
        for _ in printed..body_rows {
            println!();
        }
    } else if let Some(err) = state.last_error.as_deref() {
        state.detail_max_scroll = 0;
        state.detail_scroll = 0;
        println!("Error: {}", truncate(err, cols.saturating_sub(7)));
        for _ in 1..body_rows {
            println!();
        }
    } else {
        state.detail_max_scroll = 0;
        state.detail_scroll = 0;
        println!("Loading issue…");
        for _ in 1..body_rows {
            println!();
        }
    }

    println!("{sep}");
    println!("{}", truncate(KEY_HINT, cols));
}

fn build_title(state: &State, cols: usize) -> String {
    let raw = match state.detail_issue.as_ref() {
        Some(issue) => format!("{}  {}", issue.identifier, issue.title),
        None => state
            .detail_issue_id
            .clone()
            .unwrap_or_else(|| "Issue".to_string()),
    };
    // Reverse video so the title visually separates from the body.
    format!("\u{1b}[7m{}\u{1b}[0m", truncate(&raw, cols))
}

fn build_body_lines(issue: &IssueDetail, cols: usize) -> Vec<String> {
    let mut out = Vec::new();

    // Metadata: state · priority · labels.
    let mut meta_parts: Vec<String> = Vec::new();
    meta_parts.push(issue.state.name.clone());
    let prio = priority_label(issue.priority);
    if !prio.is_empty() {
        meta_parts.push(prio.to_string());
    }
    if !issue.labels.nodes.is_empty() {
        let labels = issue
            .labels
            .nodes
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        meta_parts.push(labels);
    }
    out.extend(wrap(&meta_parts.join("  ·  "), cols));
    out.push(String::new());

    // URL.
    out.extend(wrap(&issue.url, cols));
    out.push(String::new());

    // Description.
    out.push(section_header("Description", cols));
    let desc = issue
        .description
        .as_deref()
        .unwrap_or("(no description)")
        .replace("\r\n", "\n");
    out.extend(wrap(&desc, cols));

    // Comments.
    if !issue.comments.nodes.is_empty() {
        out.push(String::new());
        out.push(section_header(
            &format!("Comments ({})", issue.comments.nodes.len()),
            cols,
        ));
        for (i, c) in issue.comments.nodes.iter().enumerate() {
            if i > 0 {
                out.push(String::new());
            }
            let author = c
                .user
                .as_ref()
                .map(|u| u.name.as_str())
                .unwrap_or("(unknown)");
            let header = format!("@{author}  {}", c.created_at);
            out.extend(wrap(&header, cols));
            let body = c.body.replace("\r\n", "\n");
            out.extend(wrap(&body, cols));
        }
    }
    out
}

fn section_header(label: &str, cols: usize) -> String {
    let prefix = format!("─── {label} ");
    let needed = cols.saturating_sub(prefix.chars().count());
    format!("{prefix}{}", "─".repeat(needed))
}

fn priority_label(priority: f64) -> &'static str {
    match priority as u8 {
        1 => "Urgent",
        2 => "High",
        3 => "Medium",
        4 => "Low",
        _ => "",
    }
}
