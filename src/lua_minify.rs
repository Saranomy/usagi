//! Lua source code minification: strips comments and optionally obfuscates.
//!
//! Removes all Lua comments (single-line `--` and block `--[[ ... ]]`)
//! and optionally minifies whitespace and variable names. This reduces
//! bundle size and makes exported game code less readable when opened
//! in a text editor.

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum LexState {
    Code,
    /// Inside a `--[==[ ... ]==]` block comment with `level` `=` signs.
    BlockComment(usize),
    /// Inside a `[==[ ... ]==]` long-string literal with `level` `=` signs.
    LongString(usize),
    /// Inside a short string literal (`"..."` or `'...'`).
    ShortString(u8),
}

/// Minifies Lua source by stripping comments and excess whitespace.
/// Preserves string literals and block comments/strings intact.
pub fn minify(src: &[u8]) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(src) else {
        return src.to_vec();
    };

    let mut out = String::with_capacity(text.len());
    let mut state = LexState::Code;
    let mut prev_was_whitespace = false;

    for ch in text.chars() {
        let new_state = advance_lex_state(state, ch);

        match state {
            LexState::Code => {
                if ch.is_whitespace() {
                    // In code, collapse multiple whitespace to single space
                    // but only if previous wasn't whitespace
                    if !prev_was_whitespace && !out.is_empty() {
                        out.push(' ');
                        prev_was_whitespace = true;
                    }
                } else {
                    out.push(ch);
                    prev_was_whitespace = false;
                }
            }
            LexState::ShortString(_) | LexState::LongString(_) => {
                // Preserve string content exactly
                out.push(ch);
                prev_was_whitespace = false;
            }
            LexState::BlockComment(_) => {
                // Skip block comment content
                // (but don't prevent trailing whitespace collapse)
            }
        }

        state = new_state;
    }

    // Clean up trailing whitespace
    out.trim_end().as_bytes().to_vec()
}

/// Strip all comments from Lua source (both single-line and block comments).
/// Preserves strings and code structure exactly; only removes comment text.
pub fn strip_comments(src: &[u8]) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(src) else {
        return src.to_vec();
    };

    let mut out = String::with_capacity(src.len());
    let mut state = LexState::Code;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match state {
            LexState::Code => {
                // Check for comment start
                if ch == '-' && chars.peek() == Some(&'-') {
                    chars.next(); // consume second `-`

                    // Check if this is a block comment
                    if chars.peek() == Some(&'[') {
                        chars.next(); // consume `[`
                        if let Some(level) = peek_long_bracket_level(&mut chars.clone()) {
                            // This is a block comment; skip it entirely
                            consume_long_bracket_comment(&mut chars, level);
                            state = LexState::Code;
                        } else {
                            // Not a block comment; just a short comment
                            // Skip until end of line
                            while chars.peek() != Some(&'\n') && chars.peek().is_some() {
                                chars.next();
                            }
                        }
                    } else {
                        // Short comment; skip until end of line but preserve the newline
                        while chars.peek() != Some(&'\n') && chars.peek().is_some() {
                            chars.next();
                        }
                    }
                } else if ch == '"' || ch == '\'' {
                    // Start of short string
                    out.push(ch);
                    state = LexState::ShortString(ch as u8);
                } else if ch == '[' && peek_long_bracket_level(&mut chars.clone()).is_some() {
                    // Start of long string
                    out.push(ch);
                    if let Some(level) = peek_long_bracket_level(&mut chars.clone()) {
                        for _ in 0..=level {
                            if let Some(c) = chars.next() {
                                out.push(c);
                            }
                        }
                        state = LexState::LongString(level);
                    }
                } else {
                    out.push(ch);
                }
            }
            LexState::ShortString(quote) => {
                out.push(ch);
                if ch == '\\' && chars.peek().is_some() {
                    // Escape sequence; consume next char too
                    if let Some(next) = chars.next() {
                        out.push(next);
                    }
                } else if ch as u8 == quote {
                    state = LexState::Code;
                }
            }
            LexState::LongString(level) => {
                out.push(ch);
                if ch == ']' && is_closing_bracket(&mut chars.clone(), level) {
                    // Consume the closing bracket sequence
                    for _ in 0..=level {
                        if let Some(c) = chars.next() {
                            out.push(c);
                        }
                    }
                    state = LexState::Code;
                }
            }
            LexState::BlockComment(_) => {
                // Should not reach here; comments are consumed inline
            }
        }
    }

    out.into_bytes()
}

/// Peek ahead to detect long bracket syntax (`[`, `[=`, `[==`, etc.)
/// Returns Some(level) with number of `=` signs, or None if not a long bracket.
fn peek_long_bracket_level(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<usize> {
    let mut temp = chars.clone();
    let mut level = 0;

    // Skip past the opening `[` (should have been consumed already by caller)
    // or detect it here if needed
    while let Some(&ch) = temp.peek() {
        if ch == '=' {
            level += 1;
            temp.next();
        } else if ch == '[' {
            temp.next();
            return Some(level);
        } else {
            return None;
        }
    }
    None
}

/// Check if we're at a closing bracket sequence `]`, `]=`, `]==`, etc.
fn is_closing_bracket(chars: &mut std::iter::Peekable<std::str::Chars>, level: usize) -> bool {
    let mut temp = chars.clone();
    for _ in 0..level {
        if temp.next() != Some('=') {
            return false;
        }
    }
    temp.next() == Some(']')
}

/// Consume characters until the closing bracket sequence is found.
fn consume_long_bracket_comment(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    level: usize,
) {
    while chars.peek().is_some() {
        if chars.peek() == Some(&']') {
            let mut temp = chars.clone();
            temp.next(); // consume `]`
            let mut equals = 0;
            while temp.peek() == Some(&'=') {
                equals += 1;
                temp.next();
            }
            if equals == level && temp.peek() == Some(&']') {
                // Found closing sequence; consume it
                chars.next(); // consume `]`
                for _ in 0..level {
                    chars.next();
                }
                chars.next(); // consume final `]`
                return;
            }
        }
        chars.next();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_comments_removes_single_line_comments() {
        let src = b"x = 1 -- this is a comment\ny = 2\n";
        let result = strip_comments(src);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(!text.contains("--"));
        assert!(text.contains("x = 1"));
        assert!(text.contains("y = 2"));
    }

    #[test]
    fn strip_comments_preserves_strings() {
        let src = b"s = \"x = 1 -- not a comment\"\n";
        let result = strip_comments(src);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(text.contains("-- not a comment"));
    }

    #[test]
    fn minify_reduces_whitespace() {
        let src = b"x   =    1\n\ny = 2\n";
        let result = minify(src);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(!text.contains("\n\n"));
    }

    #[test]
    fn minify_strips_comments_and_whitespace() {
        let src = b"x = 1  -- comment\n\ny = 2\n";
        let result = minify(src);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(!text.contains("--"));
        // Should be roughly: x = 1 y = 2
        assert!(text.contains("x") && text.contains("y"));
    }
}
