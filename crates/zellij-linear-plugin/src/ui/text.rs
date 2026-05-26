//! Display-width aware truncation + word-wrapping. Uses `unicode-width`
//! so CJK characters and emoji count as 2 cells.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const ELLIPSIS: char = '…';
const ELLIPSIS_WIDTH: usize = 1;

/// Word-wrap `text` to lines of at most `cols` display columns. Hard
/// line breaks in the input are preserved. Long words that don't fit
/// in `cols` are broken on character boundaries rather than overflowing.
pub fn wrap(text: &str, cols: usize) -> Vec<String> {
    if cols == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut line = String::new();
        let mut line_width = 0usize;
        for word in paragraph.split(' ') {
            let word_width = word.width();
            // Standalone word longer than the line — char-break it.
            if word_width > cols {
                if !line.is_empty() {
                    out.push(std::mem::take(&mut line));
                    line_width = 0;
                }
                let mut chunk = String::new();
                let mut chunk_width = 0usize;
                for c in word.chars() {
                    let w = UnicodeWidthChar::width(c).unwrap_or(0);
                    if chunk_width + w > cols {
                        out.push(std::mem::take(&mut chunk));
                        chunk_width = 0;
                    }
                    chunk.push(c);
                    chunk_width += w;
                }
                if !chunk.is_empty() {
                    line = chunk;
                    line_width = chunk_width;
                }
                continue;
            }
            // Need a space-separator if the line already has content.
            let sep_width = if line.is_empty() { 0 } else { 1 };
            if line_width + sep_width + word_width > cols {
                out.push(std::mem::take(&mut line));
                line.push_str(word);
                line_width = word_width;
            } else {
                if !line.is_empty() {
                    line.push(' ');
                    line_width += 1;
                }
                line.push_str(word);
                line_width += word_width;
            }
        }
        out.push(line);
    }
    out
}

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

    #[test]
    fn wrap_short_text_fits_in_one_line() {
        assert_eq!(wrap("hello world", 20), vec!["hello world"]);
    }

    #[test]
    fn wrap_breaks_on_word_boundary() {
        let out = wrap("one two three four five", 10);
        assert!(out.iter().all(|l| l.width() <= 10), "out = {out:?}");
        assert_eq!(out.join(" "), "one two three four five");
    }

    #[test]
    fn wrap_preserves_blank_lines() {
        let out = wrap("para one\n\npara two", 20);
        assert_eq!(out, vec!["para one", "", "para two"]);
    }

    #[test]
    fn wrap_breaks_overlong_word_on_chars() {
        // 18 chars, no spaces. Wraps at every 5.
        let out = wrap("abcdefghijklmnopqr", 5);
        assert_eq!(out, vec!["abcde", "fghij", "klmno", "pqr"]);
    }

    #[test]
    fn wrap_handles_cjk() {
        // Each 漢 is 2 cols; 4 of them at cols=4 should wrap to 2 lines.
        let out = wrap("漢字漢字漢字漢字", 4);
        assert!(out.iter().all(|l| l.width() <= 4));
    }
}
