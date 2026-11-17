//! AT-42 (fail-first): KiteContext::new with an empty font set returns a
//! KiteError that names the missing required UA fonts and does not
//! construct a context that would render with system or host fonts.
//!
//! Required UA fonts (SPEC v24.2 §5.7 / DD-22, DD-35, DD-101): Noto Sans,
//! Noto Sans Italic, Noto Serif, Noto Sans Mono, Noto Sans Symbols,
//! Noto Sans SC, and Noto Emoji (monochrome).
//!
//! Retirement: the step-3 assertions that the empty-set error must *not*
//! contain "Noto Color Emoji" or "Noto Sans SC" are retired because Ashley
//! reversed Q-05 / DD-35 on 2026-11-16 ("Add the fonts in the 4th if it
//! will help."). CJK and emoji are now required UA fonts. The required
//! emoji name is "Noto Emoji" (monochrome), not "Noto Color Emoji",
//! because color-emoji paint support is not established in the vendored
//! renderer.

use s3a_pack_kite::{KiteContext, KiteError};

const REQUIRED_UA_FONTS: &[&str] = &[
    "Noto Sans",
    "Noto Sans Italic",
    "Noto Serif",
    "Noto Sans Mono",
    "Noto Sans Symbols",
    "Noto Sans SC",
    "Noto Emoji",
];

fn complete_set() -> Vec<(String, Vec<u8>)> {
    REQUIRED_UA_FONTS
        .iter()
        .map(|name| ((*name).to_string(), format!("{name} fixture bytes").into_bytes()))
        .collect()
}

fn assert_names_missing_fonts(err: &KiteError) {
    let message = err.message();
    for name in REQUIRED_UA_FONTS {
        assert!(
            message.contains(name),
            "KiteError message must name missing font {name:?}; got {message:?}"
        );
    }
}

#[test]
fn empty_font_set_returns_error_naming_missing_fonts() {
    let err = KiteContext::new(Vec::<(String, Vec<u8>)>::new())
        .expect_err("AT-42: an empty UA font set must be KiteError, not a constructed context");
    assert_names_missing_fonts(&err);
    let code = err.code();
    assert!(!code.is_empty(), "KiteError must carry a stable non-empty code");
    let again = KiteContext::new(Vec::<(String, Vec<u8>)>::new()).expect_err("empty set must error again");
    assert_eq!(again.code(), code, "KiteError code for a missing UA font set must be stable");
}

#[test]
fn missing_one_required_font_is_named() {
    let mut fonts = complete_set();
    let removed = fonts.remove(2);
    let err = KiteContext::new(fonts).expect_err(
        "a set missing a required UA font must be KiteError",
    );
    assert!(
        err.message().contains(&removed.0),
        "KiteError must name the missing font {}; got {:?}",
        removed.0,
        err.message()
    );
}

#[test]
fn missing_cjk_font_is_named() {
    let fonts: Vec<(String, Vec<u8>)> = REQUIRED_UA_FONTS
        .iter()
        .filter(|name| **name != "Noto Sans SC")
        .map(|name| ((*name).to_string(), format!("{name} fixture bytes").into_bytes()))
        .collect();
    let err = KiteContext::new(fonts).expect_err(
        "a set missing required Noto Sans SC must be KiteError after the Q-05 reversal",
    );
    assert!(
        err.message().contains("Noto Sans SC"),
        "KiteError must name missing Noto Sans SC; got {:?}",
        err.message()
    );
}

#[test]
fn missing_emoji_font_is_named() {
    let fonts: Vec<(String, Vec<u8>)> = REQUIRED_UA_FONTS
        .iter()
        .filter(|name| **name != "Noto Emoji")
        .map(|name| ((*name).to_string(), format!("{name} fixture bytes").into_bytes()))
        .collect();
    let err = KiteContext::new(fonts).expect_err(
        "a set missing required Noto Emoji must be KiteError after the Q-05 reversal",
    );
    assert!(
        err.message().contains("Noto Emoji"),
        "KiteError must name missing Noto Emoji; got {:?}",
        err.message()
    );
}

#[test]
fn complete_required_set_builds_a_context_with_system_fonts_false() {
    let ctx = KiteContext::new(complete_set())
        .expect("a complete required UA font set must construct a KiteContext");
    assert!(
        !ctx.system_fonts(),
        "system_fonts stays false; a constructed context must not use system or host fonts (AT-42)"
    );
}

#[test]
fn contexts_are_not_process_global() {
    let a = KiteContext::new(complete_set()).expect("first context");
    let b = KiteContext::new(complete_set()).expect("second context");
    assert!(!a.system_fonts());
    assert!(!b.system_fonts());
}
