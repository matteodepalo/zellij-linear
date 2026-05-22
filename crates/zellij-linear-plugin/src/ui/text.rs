//! Display-width aware truncation. Uses `unicode-width` so CJK
//! characters and emoji count as 2 cells.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const ELLIPSIS: char = '…';
const ELLIPSIS_WIDTH: usize = 1;

/// Truncate `s` so it fits in at most `cols` display columns, appending
/// `…` when content was dropped. Returns an empty string when `cols == 0`.
pub fn truncate(s: &str, cols: usize) -> String {
    if cols == 0 {
        return String::new();
    }
    if s.width() <= cols {
        return s.to_string();
    }
    let budget = cols.saturating_sub(ELLIPSIS_WIDTH);
    let mut width = 0usize;
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let w = UnicodeWidthChar::width(c).unwrap_or(0);
        if width + w > budget {
            break;
        }
        out.push(c);
        width += w;
    }
    out.push(ELLIPSIS);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn short_input_returned_verbatim() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn long_input_gets_ellipsis() {
        let out = truncate("abcdefghij", 5);
        assert_eq!(out, "abcd…");
        assert_eq!(out.width(), 5);
    }

    #[test]
    fn empty_cols_returns_empty() {
        assert_eq!(truncate("anything", 0), "");
    }

    #[test]
    fn cjk_counts_as_two_columns() {
        // 漢字 is 4 columns; should fit in width 4 but not in 3.
        assert_eq!(truncate("漢字", 4), "漢字");
        let truncated = truncate("漢字漢字", 4);
        assert!(
            truncated.width() <= 4,
            "got `{truncated}` ({} cols)",
            truncated.width()
        );
        assert!(truncated.ends_with('…'));
    }
}
