//! Send-to-Claude bridge — find the Claude pane in the current session
//! and paste a rendered prompt into it.

use std::collections::BTreeMap;

use linear_client::types::Issue;
use zellij_tile::prelude::{
    copy_to_clipboard, focus_pane_with_id, write_chars_to_pane_id, PaneId, PaneInfo,
};

pub const DEFAULT_PROMPT_TEMPLATE: &str = "I'm working on Linear issue {identifier}: {title}\n\
\n\
URL: {url}\n\
\n\
## Description\n\
{description}\n\
\n\
## Labels\n\
{labels_as_bullets}\n\
\n\
Please propose an approach before writing code.";

pub fn render_prompt(issue: &Issue, template: &str) -> String {
    let description = issue.description.as_deref().unwrap_or("(no description)");
    let labels = if issue.labels.nodes.is_empty() {
        "(no labels)".to_string()
    } else {
        issue
            .labels
            .nodes
            .iter()
            .map(|l| format!("- {}", l.name))
            .collect::<Vec<_>>()
            .join("\n")
    };
    template
        .replace("{identifier}", &issue.identifier)
        .replace("{title}", &issue.title)
        .replace("{description}", description)
        .replace("{labels_as_bullets}", &labels)
        .replace("{url}", &issue.url)
}

pub fn find_claude_pane(
    panes: &BTreeMap<usize, Vec<PaneInfo>>,
    target_substring: &str,
) -> Option<PaneId> {
    panes
        .values()
        .flatten()
        .filter(|p| !p.is_plugin)
        .find_map(|p| {
            let cmd = p.terminal_command.as_deref()?;
            cmd.contains(target_substring)
                .then(|| PaneId::Terminal(p.id))
        })
}

pub enum SendOutcome {
    Sent,
    Copied,
}

/// Paste `prompt` into the Claude pane if one is found, otherwise copy
/// it to the clipboard.
pub fn send_or_copy(
    panes: &BTreeMap<usize, Vec<PaneInfo>>,
    target_substring: &str,
    prompt: String,
    auto_submit: bool,
) -> SendOutcome {
    match find_claude_pane(panes, target_substring) {
        Some(pane_id) => {
            // Float-on-hidden=false, in-place-on-hidden=true: surface the
            // pane in its tiled position if it's currently suppressed.
            focus_pane_with_id(pane_id, false, true);
            let mut to_send = prompt;
            if auto_submit && !to_send.ends_with('\n') {
                to_send.push('\n');
            }
            write_chars_to_pane_id(&to_send, pane_id);
            SendOutcome::Sent
        }
        None => {
            // Clipboard fallback omits the auto-submit newline so paste
            // doesn't carry an extra blank line.
            copy_to_clipboard(prompt);
            SendOutcome::Copied
        }
    }
}
