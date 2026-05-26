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
                .then_some(PaneId::Terminal(p.id))
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

#[cfg(test)]
mod tests {
    use super::*;
    use linear_client::types::{Issue, IssueState, Label, LabelConnection};

    fn sample_issue() -> Issue {
        Issue {
            id: "abc".into(),
            identifier: "ENG-1".into(),
            title: "Fix login".into(),
            description: Some("It crashes on Safari.".into()),
            priority: 2.0,
            state: IssueState {
                name: "In Progress".into(),
                state_type: "started".into(),
                color: "#ff0000".into(),
            },
            labels: LabelConnection {
                nodes: vec![
                    Label {
                        name: "bug".into(),
                        color: "#ff0000".into(),
                    },
                    Label {
                        name: "auth".into(),
                        color: "#00ff00".into(),
                    },
                ],
            },
            url: "https://linear.app/x/issue/ENG-1".into(),
            updated_at: "2026-05-22T00:00:00Z".into(),
            created_at: "2026-05-22T00:00:00Z".into(),
        }
    }

    #[test]
    fn render_prompt_substitutes_placeholders() {
        let issue = sample_issue();
        let out = render_prompt(&issue, DEFAULT_PROMPT_TEMPLATE);
        assert!(out.contains("ENG-1"));
        assert!(out.contains("Fix login"));
        assert!(out.contains("It crashes on Safari."));
        assert!(out.contains("- bug"));
        assert!(out.contains("- auth"));
        assert!(out.contains("https://linear.app/x/issue/ENG-1"));
    }

    #[test]
    fn render_prompt_handles_missing_description() {
        let mut issue = sample_issue();
        issue.description = None;
        let out = render_prompt(&issue, DEFAULT_PROMPT_TEMPLATE);
        assert!(out.contains("(no description)"));
    }

    #[test]
    fn render_prompt_handles_empty_labels() {
        let mut issue = sample_issue();
        issue.labels.nodes.clear();
        let out = render_prompt(&issue, DEFAULT_PROMPT_TEMPLATE);
        assert!(out.contains("(no labels)"));
    }

    #[test]
    fn render_prompt_supports_custom_template() {
        let issue = sample_issue();
        let out = render_prompt(&issue, "Work on {identifier}: {title}");
        assert_eq!(out, "Work on ENG-1: Fix login");
    }

    fn pane(id: u32, command: Option<&str>, is_plugin: bool) -> PaneInfo {
        PaneInfo {
            id,
            terminal_command: command.map(str::to_string),
            is_plugin,
            ..PaneInfo::default()
        }
    }

    #[test]
    fn find_claude_pane_matches_substring() {
        let mut panes = BTreeMap::new();
        panes.insert(
            0,
            vec![
                pane(7, Some("bash"), false),
                pane(8, Some("claude --resume"), false),
            ],
        );
        let found = find_claude_pane(&panes, "claude");
        assert_eq!(found, Some(PaneId::Terminal(8)));
    }

    #[test]
    fn find_claude_pane_excludes_plugin_panes() {
        let mut panes = BTreeMap::new();
        panes.insert(0, vec![pane(1, Some("claude"), true)]);
        assert!(find_claude_pane(&panes, "claude").is_none());
    }

    #[test]
    fn find_claude_pane_returns_none_when_no_match() {
        let mut panes = BTreeMap::new();
        panes.insert(0, vec![pane(1, Some("vim"), false), pane(2, None, false)]);
        assert!(find_claude_pane(&panes, "claude").is_none());
    }

    #[test]
    fn find_claude_pane_searches_across_tabs() {
        let mut panes = BTreeMap::new();
        panes.insert(0, vec![pane(1, Some("bash"), false)]);
        panes.insert(1, vec![pane(2, Some("claude code"), false)]);
        assert_eq!(
            find_claude_pane(&panes, "claude"),
            Some(PaneId::Terminal(2))
        );
    }
}
