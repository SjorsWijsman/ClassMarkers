//! Reading the icon PNGs in ClassMarkers/icons/.

/// An image as rows of [R, G, B, A] bytes.
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Decode a PNG of any colour type and bit depth to 8-bit RGBA.
pub fn decode_png(bytes: &[u8]) -> Result<Rgba, String> {
    use png::{ColorType, Transformations};
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(Transformations::EXPAND | Transformations::STRIP_16 | Transformations::ALPHA);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("image too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());
    let data = match info.color_type {
        ColorType::Rgba => buf,
        ColorType::GrayscaleAlpha => buf.chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        other => return Err(format!("unexpected colour type {:?}", other)),
    };
    Ok(Rgba { width: info.width, height: info.height, data })
}
