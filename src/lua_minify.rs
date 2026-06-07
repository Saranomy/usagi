//! Lua source code minification: strips comments and obfuscates variable names.
//!
//! Removes all Lua comments (single-line `--` and block `--[[ ... ]]`)
//! and obfuscates local variable names to reduce bundle size and make
//! exported game code less readable when opened in a text editor.

use std::collections::HashMap;

/// Strip all comments from Lua source (both single-line and block comments),
/// and obfuscate local variable names.
/// Preserves strings and code structure; only removes comment text and renames locals.
pub fn strip_comments(src: &[u8]) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(src) else {
        return src.to_vec();
    };

    // First pass: strip comments
    let no_comments = strip_comments_only(text);

    // Second pass: obfuscate variable names
    let obfuscated = obfuscate_variables(&no_comments);

    // Third pass: collapse whitespace (but preserve newlines for debugging)
    collapse_excess_whitespace(&obfuscated).into_bytes()
}

/// Strip comments only, preserving all other content and whitespace.
fn strip_comments_only(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
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
                    // Skip until end of line (but preserve newline)
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
                out.push(ch);
            }
        }
    }

    out
}

/// Obfuscate local variable names using a simple counter-based scheme.
/// Renames `local foo` -> `local a`, `local bar` -> `local b`, etc.
/// Preserves global variables, strings, and the engine API (usagi, gfx, input, etc).
fn obfuscate_variables(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut var_map: HashMap<String, String> = HashMap::new();
    let mut var_counter = 0;

    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        // Handle strings: don't obfuscate anything inside them
        if ch == '"' || ch == '\'' {
            let quote = ch;
            out.push(ch);
            while let Some(c) = chars.next() {
                out.push(c);
                if c == '\\' && chars.peek().is_some() {
                    if let Some(escaped) = chars.next() {
                        out.push(escaped);
                    }
                } else if c == quote {
                    break;
                }
            }
            continue;
        }

        // Handle long strings
        if ch == '[' {
            let mut temp = chars.clone();
            if let Some(level) = peek_long_bracket_level(&mut temp) {
                out.push(ch);
                for _ in 0..level {
                    if let Some(c) = chars.next() {
                        out.push(c);
                    }
                }
                if let Some(c) = chars.next() {
                    out.push(c);
                }

                while let Some(c) = chars.next() {
                    out.push(c);
                    if c == ']' && is_closing_bracket(&mut chars.clone(), level) {
                        for _ in 0..level {
                            if let Some(eq) = chars.next() {
                                out.push(eq);
                            }
                        }
                        if let Some(closing) = chars.next() {
                            out.push(closing);
                        }
                        break;
                    }
                }
                continue;
            }
        }

        // Check for `local` keyword
        if ch == 'l' {
            let mut temp = chars.clone();
            let mut word = String::from(ch);

            // Try to read the full word
            while let Some(&c) = temp.peek() {
                if c.is_alphanumeric() || c == '_' {
                    word.push(c);
                    temp.next();
                } else {
                    break;
                }
            }

            if word == "local" && (temp.peek().is_none() || !temp.peek().unwrap().is_alphanumeric()) {
                // This is the `local` keyword
                out.push_str("local");
                for _ in 0..word.len() - 5 {
                    chars.next();
                }

                // Skip whitespace after `local`
                while chars.peek() == Some(&' ') || chars.peek() == Some(&'\t') {
                    out.push(chars.next().unwrap());
                }

                // Read the variable name
                let mut var_name = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        var_name.push(c);
                        out.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }

                // Map this variable to an obfuscated name
                if !var_name.is_empty() && !is_engine_api(&var_name) {
                    let obfuscated = format!("_{}", var_counter);
                    var_counter += 1;
                    var_map.insert(var_name.clone(), obfuscated);
                }

                continue;
            }
        }

        // Check if this is the start of an identifier
        if ch.is_alphabetic() || ch == '_' {
            let mut temp = chars.clone();
            let mut ident = String::from(ch);

            while let Some(&c) = temp.peek() {
                if c.is_alphanumeric() || c == '_' {
                    ident.push(c);
                    temp.next();
                } else {
                    break;
                }
            }

            // Check if this identifier should be obfuscated
            if let Some(obfuscated) = var_map.get(&ident) {
                out.push_str(obfuscated);
                // Consume the original identifier
                for _ in 0..ident.len() - 1 {
                    chars.next();
                }
            } else {
                out.push(ch);
            }
            continue;
        }

        out.push(ch);
    }

    out
}

/// Collapse multiple spaces/tabs into single spaces, remove trailing whitespace on lines.
fn collapse_excess_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_was_space = false;

    for ch in text.chars() {
        if ch == ' ' || ch == '\t' {
            if !prev_was_space && !out.ends_with('\n') && !out.is_empty() {
                out.push(' ');
                prev_was_space = true;
            }
        } else if ch == '\n' {
            // Remove trailing spaces before newlines
            while out.ends_with(' ') {
                out.pop();
            }
            out.push('\n');
            prev_was_space = false;
        } else {
            out.push(ch);
            prev_was_space = false;
        }
    }

    out
}

/// Check if an identifier is part of the engine API and should not be obfuscated.
fn is_engine_api(name: &str) -> bool {
    matches!(
        name,
        // Engine API tables
        "gfx" | "input" | "sfx" | "music" | "usagi" | "effect"
            // Common Lua builtins
            | "print" | "require" | "tostring" | "tonumber" | "type" | "ipairs"
            | "pairs" | "next" | "table" | "string" | "math" | "os" | "io"
            // Common game state (usually capitalized, but be safe)
            | "State" | "Player" | "Enemy" | "Bullet"
            // Lua keywords we might encounter in identifier position
            | "function" | "end" | "if" | "then" | "else" | "elseif"
            | "for" | "do" | "while" | "repeat" | "until" | "return"
            | "nil" | "true" | "false" | "and" | "or" | "not" | "in"
            // Standard callbacks
            | "_init" | "_update" | "_draw" | "_update_buttons"
            | "on_pain" | "on_heal"
    )
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
        assert!(!text.contains("this is a comment"));
    }

    #[test]
    fn strip_comments_removes_block_comments() {
        let src = b"x = 1 --[[ block comment ]] y = 2\n";
        let result = strip_comments(src);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(!text.contains("block comment"));
    }

    #[test]
    fn strip_comments_preserves_strings() {
        let src = b"s = \"x = 1 -- not a comment\"\n";
        let result = strip_comments(src);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(text.contains("-- not a comment"));
    }

    #[test]
    fn strip_comments_obfuscates_locals() {
        let src = b"local foo = 1\nlocal bar = foo + 2\n";
        let result = strip_comments(src);
        let text = std::str::from_utf8(&result).unwrap();
        // Variables should be renamed to _0, _1, etc
        assert!(text.contains("_0"));
        assert!(!text.contains("foo"));
    }

    #[test]
    fn strip_comments_preserves_engine_api() {
        let src = b"gfx.clear()\ninput.btn(0)\nmusic.play()\n";
        let result = strip_comments(src);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(text.contains("gfx"));
        assert!(text.contains("input"));
        assert!(text.contains("music"));
    }

    #[test]
    fn strip_comments_preserves_callbacks() {
        let src = b"function _init() end\nfunction _update() end\nfunction _draw() end\n";
        let result = strip_comments(src);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(text.contains("_init"));
        assert!(text.contains("_update"));
        assert!(text.contains("_draw"));
    }
}
