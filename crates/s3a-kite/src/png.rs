//! PNG encoding for `/v1/frame` (the pack renders a pixmap; the host
//! encodes it as PNG and base64s it).

/// Encode an RGBA pixmap (row-major, `width * height * 4` bytes) as a PNG.
pub fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let expected = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| "pixmap dimensions overflow".to_string())?;
    if rgba.len() as u64 != expected as u64 {
        return Err(format!(
            "pixmap length {} does not match {width}x{height} RGBA",
            rgba.len()
        ));
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| format!("PNG header write failed: {e}"))?;
    writer
        .write_image_data(rgba)
        .map_err(|e| format!("PNG data write failed: {e}"))?;
    writer.finish().map_err(|e| format!("PNG finish failed: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_1x1() {
        let png = encode_png(&[255, 0, 0, 255], 1, 1).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert!(png.len() > 20);
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(encode_png(&[0u8; 3], 1, 1).is_err());
    }
}
