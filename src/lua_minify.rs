//! Lua source code minification: strips comments and optionally obfuscates.
//!
//! Removes all Lua comments (single-line `--` and block `--[[ ... ]]`)
//! while preserving string literals and code structure. This reduces
//! bundle size and makes exported game code less readable when opened
//! in a text editor.

/// Strip all comments from Lua source (both single-line and block comments).
/// Preserves strings and code structure exactly; only removes comment text.
pub fn strip_comments(src: &[u8]) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(src) else {
        return src.to_vec();
    };

    let mut out = Vec::with_capacity(src.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            // Potential comment start
            '-' if chars.peek() == Some(&'-') => {
                chars.next(); // consume second `-`

                // Check if this is a block comment `--[` or `--[[`
                if chars.peek() == Some(&'[') {
                    if let Some(level) = peek_long_bracket_level(&mut chars.clone()) {
                        // Block comment: skip the opening `[...[`
                        chars.next(); // consume `[`
                        for _ in 0..level {
                            chars.next(); // consume `=` signs
                        }
                        chars.next(); // consume final `[`

                        // Skip everything until we find the closing `]....]`
                        while let Some(c) = chars.next() {
                            if c == ']' && is_closing_bracket(&mut chars.clone(), level) {
                                // Consume the closing bracket sequence
                                for _ in 0..level {
                                    chars.next();
                                }
                                chars.next(); // consume final `]`
                                break;
                            }
                        }
                    } else {
                        // Not a block comment, just a regular `--[` comment
                        // Skip to end of line
                        while chars.peek() != Some(&'\n') && chars.peek().is_some() {
                            chars.next();
                        }
                    }
                } else {
                    // Regular short comment `--`
                    // Skip until end of line
                    while chars.peek() != Some(&'\n') && chars.peek().is_some() {
                        chars.next();
                    }
                }
            }

            // Short string literals
            '"' | '\'' => {
                let quote = ch;
                out.push(ch);

                // Copy string content until closing quote
                while let Some(c) = chars.next() {
                    out.push(c);
                    if c == '\\' && chars.peek().is_some() {
                        // Escape sequence: consume next char
                        if let Some(escaped) = chars.next() {
                            out.push(escaped);
                        }
                    } else if c == quote {
                        break;
                    }
                }
            }

            // Long string literals `[[ ... ]]` or `[=[ ... ]=]`
            '[' if peek_long_bracket_level(&mut chars.clone()).is_some() => {
                out.push(ch);
                if let Some(level) = peek_long_bracket_level(&mut chars.clone()) {
                    // Consume the opening `[...[`
                    for _ in 0..level {
                        if let Some(c) = chars.next() {
                            out.push(c);
                        }
                    }
                    if let Some(c) = chars.next() {
                        out.push(c); // closing `[`
                    }

                    // Copy until we find the closing `]......]`
                    while let Some(c) = chars.next() {
                        out.push(c);
                        if c == ']' && is_closing_bracket(&mut chars.clone(), level) {
                            // Consume the closing bracket sequence
                            for _ in 0..level {
                                if let Some(eq) = chars.next() {
                                    out.push(eq);
                                }
                            }
                            if let Some(closing) = chars.next() {
                                out.push(closing); // closing `]`
                            }
                            break;
                        }
                    }
                }
            }

            // Everything else: copy as-is
            _ => {
                out.push(ch as u8);
            }
        }
    }

    out
}

/// Peek ahead to detect long bracket syntax (`[`, `[=`, `[==`, etc.)
/// Returns Some(level) with number of `=` signs, or None if not a long bracket.
fn peek_long_bracket_level(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<usize> {
    let mut temp = chars.clone();
    let mut level = 0;

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
    fn strip_comments_removes_block_comments() {
        let src = b"x = 1 --[[ block comment ]] y = 2\n";
        let result = strip_comments(src);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(!text.contains("block comment"));
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
    fn strip_comments_preserves_long_strings() {
        let src = b"s = [[ x = 1 -- [[ not nested ]] ]] y = 2\n";
        let result = strip_comments(src);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(text.contains("x = 1"));
        assert!(text.contains("not nested"));
    }

    #[test]
    fn strip_comments_handles_multiline_block_comments() {
        let src = b"--[[\nmultiline\ncomment\n]]\nx = 1\n";
        let result = strip_comments(src);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(!text.contains("multiline"));
        assert!(!text.contains("comment"));
        assert!(text.contains("x = 1"));
    }

    #[test]
    fn strip_comments_handles_leveled_brackets() {
        let src = b"x = --[=[ comment ]=] 1\n";
        let result = strip_comments(src);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(!text.contains("comment"));
        assert!(text.contains("x = 1"));
    }

    #[test]
    fn strip_comments_preserves_function_definitions() {
        let src = b"function test() -- comment\n  return 42 -- another\nend\n";
        let result = strip_comments(src);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(text.contains("function test()"));
        assert!(text.contains("return 42"));
        assert!(!text.contains("comment"));
    }
}
