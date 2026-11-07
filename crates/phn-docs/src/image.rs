//! Minimal image inspection for embedding (PNG and JPEG).

/// Supported image formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

/// Format and pixel size of an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageInfo {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    /// JPEG color components (1 = gray, 3 = RGB, 4 = CMYK).
    pub components: u8,
}

/// Inspect PNG or JPEG bytes.
pub fn inspect(bytes: &[u8]) -> Option<ImageInfo> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 24 {
        let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
        let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
        return Some(ImageInfo {
            format: ImageFormat::Png,
            width: w,
            height: h,
            components: 4,
        });
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = bytes[i + 1];
            let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            // SOF0..SOF15 except DHT (C4), JPG (C8), DAC (CC).
            if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
                let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
                let components = bytes[i + 9];
                return Some(ImageInfo {
                    format: ImageFormat::Jpeg,
                    width: w,
                    height: h,
                    components,
                });
            }
            i += 2 + len;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_png_size() {
        let mut pm = resvg::tiny_skia::Pixmap::new(7, 3).unwrap();
        pm.fill(resvg::tiny_skia::Color::WHITE);
        let png = pm.encode_png().unwrap();
        let info = inspect(&png).unwrap();
        assert_eq!(
            (info.format, info.width, info.height),
            (ImageFormat::Png, 7, 3)
        );
        assert!(inspect(b"nope").is_none());
    }
}
