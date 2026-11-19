//! Required UA fonts (§5.7, Q-05 / DD-35): the pack's `KiteContext::new`
//! requires all 7 with non-empty bytes. The files are vendored in the repo
//! (Kitesurf's `packages/engine/public/fonts/`); the host embeds them so
//! the binary is self-contained. Paint is rect-fill in v1 (no glyph
//! rasterizer wired), so the bytes are a presence contract today and the
//! real rasterizer input later.

/// (font name, vendored bytes) for every `REQUIRED_UA_FONTS` entry, in the
/// pack's order.
pub fn required_ua_fonts() -> Vec<(String, Vec<u8>)> {
    vec![
        (
            "Noto Sans".to_string(),
            include_bytes!("../../../vendor/s3a-Navigator/packages/engine/public/fonts/NotoSans.ttf")
                .to_vec(),
        ),
        (
            "Noto Sans Italic".to_string(),
            include_bytes!(
                "../../../vendor/s3a-Navigator/packages/engine/public/fonts/NotoSans-Italic.ttf"
            )
            .to_vec(),
        ),
        (
            "Noto Serif".to_string(),
            include_bytes!("../../../vendor/s3a-Navigator/packages/engine/public/fonts/NotoSerif.ttf")
                .to_vec(),
        ),
        (
            "Noto Sans Mono".to_string(),
            include_bytes!(
                "../../../vendor/s3a-Navigator/packages/engine/public/fonts/NotoSansMono.ttf"
            )
            .to_vec(),
        ),
        (
            "Noto Sans Symbols".to_string(),
            include_bytes!(
                "../../../vendor/s3a-Navigator/packages/engine/public/fonts/NotoSansSymbols.ttf"
            )
            .to_vec(),
        ),
        (
            "Noto Sans SC".to_string(),
            include_bytes!(
                "../../../vendor/s3a-Navigator/packages/engine/public/fonts/NotoSansCJKsc-Regular.woff2"
            )
            .to_vec(),
        ),
        (
            "Noto Emoji".to_string(),
            include_bytes!(
                "../../../vendor/s3a-Navigator/packages/engine/public/fonts/noto-color-emoji.woff2"
            )
            .to_vec(),
        ),
    ]
}
