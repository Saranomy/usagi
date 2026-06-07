//! Lua source obfuscation for bundle exports.
//!
//! Thin wrapper around `darklua_core`. Single public entry point:
//!
//! - [`obfuscate`]  — strip comments + whitespace + rename locals
//!
//! Obfuscation preserves:
//! - Lua standard globals (`print`, `pairs`, `math.*`, …)
//! - Engine table names: `usagi`, `gfx`, `sfx`
//! - Other globals automatically detected at call sites
//! - String contents, number literals, table keys

use darklua_core::Resources;
use darklua_core::generator::{LuaGenerator, TokenBasedLuaGenerator};
use darklua_core::rules::{
    ContextBuilder, FlawlessRule, RemoveComments, RemoveSpaces, RenameVariables,
};

fn obfuscate_full(src: &[u8]) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(src) else {
        return src.to_vec();
    };

    let parser = darklua_core::Parser::default().preserve_tokens();
    let mut block = match parser.parse(text) {
        Ok(b) => b,
        Err(_) => return src.to_vec(),
    };

    let resources = Resources::from_memory();
    let ctx = ContextBuilder::new("input.lua", &resources, text).build();

    // 1. Strip comments
    RemoveComments::default().flawless_process(&mut block, &ctx);

    // 2. Strip excessive whitespace
    RemoveSpaces::default().flawless_process(&mut block, &ctx);

    // 3. Rename local variables to short anonymous identifiers.
    // These tables are provided by the engine at runtime and must keep
    // their original names.  Everything else referenced as a global
    // but never declared with `local` in this file (including Lua
    // builtins like `print`, `pairs`, `math` and user callbacks like
    // `_init`, `_update`, `_draw`) is auto-detected by darklua's
    // `detect_globals` (default: true).
    let globals: Vec<String> = ["usagi", "gfx", "sfx"].map(String::from).to_vec();
    RenameVariables::new(globals)
        .with_function_names()
        .flawless_process(&mut block, &ctx);

    let mut generator = TokenBasedLuaGenerator::new(text);
    generator.write_block(&block);
    let mut out = generator.into_string();

    // Drop lines that are empty or whitespace-only (orphaned from
    // comment-only lines after comment removal).
    let trailing_nl = out.ends_with('\n');
    let filtered: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    out = filtered.join("\n");
    if trailing_nl && !out.is_empty() {
        out.push('\n');
    }

    out.into_bytes()
}

/// Obfuscate Lua source: strip comments, remove whitespace, rename local
/// variables to short anonymous identifiers.
///
/// Non-UTF-8 input is returned unchanged.
pub fn obfuscate(src: &[u8]) -> Vec<u8> {
    obfuscate_full(src)
}

// ---------------------------------------------------------------------------
// Tests — small integration checks
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obfuscate_renames_locals() {
        let src = b"local x = 1\nprint(x)\n";
        let out = obfuscate(src);
        let t = std::str::from_utf8(&out).unwrap();
        assert!(!t.contains("x ="), "local should be renamed, got: {t:?}");
    }

    #[test]
    fn obfuscate_preserves_globals() {
        let src = b"local x = 1\nprint(x)\n";
        let out = obfuscate(src);
        let t = std::str::from_utf8(&out).unwrap();
        assert!(t.contains("print("), "global function preserved: {t:?}");
    }

    #[test]
    fn obfuscate_preserves_engine_tables() {
        let src = b"usagi.draw_text(0, 0, 'hello')\ngfx.clear(6)\nsfx.play(0)\n";
        let out = obfuscate(src);
        let t = std::str::from_utf8(&out).unwrap();
        assert!(t.contains("usagi"), "usagi table preserved: {t:?}");
        assert!(t.contains("gfx."), "gfx table preserved: {t:?}");
        assert!(t.contains("sfx."), "sfx table preserved: {t:?}");
    }

    #[test]
    fn obfuscate_preserves_lua_builtins() {
        let src = b"local t = {1, 2, 3}\nfor i, v in ipairs(t) do print(i, v) end\n";
        let out = obfuscate(src);
        let t = std::str::from_utf8(&out).unwrap();
        assert!(t.contains("ipairs("), "ipairs preserved: {t:?}");
        assert!(t.contains("print("), "print preserved: {t:?}");
    }

    #[test]
    fn obfuscate_handles_local_function() {
        let src = b"local function f(x) return x end\nprint(f(1))\n";
        let out = obfuscate(src);
        let t = std::str::from_utf8(&out).unwrap();
        assert!(!t.contains("local function f"));
        assert!(t.contains("print("));
    }

    #[test]
    fn obfuscate_handles_for_loop_vars() {
        let src = b"for i = 1, 10 do print(i) end\n";
        let out = obfuscate(src);
        let t = std::str::from_utf8(&out).unwrap();
        assert!(t.contains("print("));
    }

    #[test]
    fn obfuscate_preserves_string_content() {
        let src = b"local msg = 'hello world'\nprint(msg)\n";
        let out = obfuscate(src);
        let t = std::str::from_utf8(&out).unwrap();
        assert!(t.contains("hello world"), "string content preserved: {t:?}");
    }

    #[test]
    fn non_utf8_passthrough() {
        let src = b"\xff\xfe\x00\x01";
        let out = obfuscate(src);
        assert_eq!(out, src);
    }

    #[test]
    fn obfuscate_strips_comments() {
        let src = b"local x = 1 -- this is a comment\nprint(x) --[[ block ]]\n";
        let out = obfuscate(src);
        let t = std::str::from_utf8(&out).unwrap();
        assert!(!t.contains("comment"), "comments stripped: {t:?}");
    }

    #[test]
    fn obfuscate_drops_comment_only_lines() {
        let src = b" -- main\nx = 1\n";
        let out = obfuscate(src);
        let t = std::str::from_utf8(&out).unwrap();
        assert!(
            !t.starts_with('\n'),
            "no leading newline from comment-only line: {t:?}"
        );
        assert!(t.contains("x=1") || t.contains("x ="));
    }

    #[test]
    fn obfuscate_drops_comment_only_lines_multi() {
        let src = b"  -- main\n  -- setup\nx = 1\n";
        let out = obfuscate(src);
        let t = std::str::from_utf8(&out).unwrap();
        assert!(
            !t.starts_with('\n'),
            "no leading newlines from comment-only lines: {t:?}"
        );
        let lines: Vec<&str> = t.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines.len(), 1, "only code lines remain: {t:?}");
    }
}
