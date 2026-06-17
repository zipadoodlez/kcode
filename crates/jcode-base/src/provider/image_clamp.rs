//! Clamp outbound image dimensions before sending a request to a provider.
//!
//! Anthropic's Messages API enforces a per-image pixel cap that depends on how
//! many images the request carries:
//!
//! | images in request | max edge per image |
//! | ------------------ | ------------------ |
//! | `> 20`             | `2000px`           |
//! | `<= 20`            | `8000px`           |
//!
//! jcode previously only enforced a 20 MB byte cap when reading images and
//! never clamped pixel dimensions, so a session that accumulated more than ~20
//! large screenshots would fail on resume with:
//!
//! ```text
//! image dimensions exceed max allowed size for many-image requests: 2000 pixels
//! ```
//!
//! This module downscales oversized images (preserving aspect ratio) so the
//! request stays within provider per-image limits. The clamp runs at the
//! `MultiProvider` chokepoint, so every provider benefits and the limits only
//! ever shrink genuinely oversized images. See issue #381.

use base64::Engine as _;
use jcode_message_types::{ContentBlock, Message};

/// Max edge (px) allowed when a request carries more than this many images.
const MANY_IMAGE_THRESHOLD: usize = 20;
/// Per-image max edge when a request carries `> MANY_IMAGE_THRESHOLD` images.
const MANY_IMAGE_MAX_EDGE: u32 = 2000;
/// Per-image max edge when a request carries `<= MANY_IMAGE_THRESHOLD` images.
const FEW_IMAGE_MAX_EDGE: u32 = 8000;

/// Inspect `messages` and, if any `ContentBlock::Image` exceeds the per-image
/// edge limit implied by the total image count, return a clamped clone of the
/// messages. Returns `None` when no downscaling is required so the common path
/// avoids cloning the (potentially large) message vector.
pub(crate) fn clamp_outbound_images(messages: &[Message]) -> Option<Vec<Message>> {
    let image_count = messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter(|b| matches!(b, ContentBlock::Image { .. }))
        .count();
    if image_count == 0 {
        return None;
    }

    let max_edge = if image_count > MANY_IMAGE_THRESHOLD {
        MANY_IMAGE_MAX_EDGE
    } else {
        FEW_IMAGE_MAX_EDGE
    };

    // Cheap pre-scan using header-only dimension probing: only do the expensive
    // decode/clone when at least one image actually exceeds the limit.
    let needs_clamp = messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::Image { data, .. } => Some(data),
            _ => None,
        })
        .any(|data| image_exceeds_edge(data, max_edge));
    if !needs_clamp {
        return None;
    }

    let mut clamped = messages.to_vec();
    let mut changed = false;
    for message in &mut clamped {
        for block in &mut message.content {
            if let ContentBlock::Image { media_type, data } = block
                && let Some(resized) = downscale_image(media_type, data, max_edge)
            {
                *data = resized;
                changed = true;
            }
        }
    }

    changed.then_some(clamped)
}

/// Return true when the base64-encoded image's larger edge exceeds `max_edge`,
/// using a cheap header-only dimension probe. When the dimensions cannot be
/// determined cheaply, returns true so the full decode path can make the final
/// decision (and skip re-encoding if it turns out to be within limits).
fn image_exceeds_edge(data_b64: &str, max_edge: u32) -> bool {
    match decode_b64(data_b64) {
        Some(bytes) => match probe_dimensions(&bytes) {
            Some((w, h)) => w > max_edge || h > max_edge,
            // Unknown format/dimensions: let the decode path decide.
            None => true,
        },
        None => false,
    }
}

/// Decode, downscale, and re-encode an image so its larger edge is `<= max_edge`.
/// Returns the new base64 payload, or `None` when no change is needed or the
/// image cannot be processed (in which case the original is left untouched).
fn downscale_image(media_type: &str, data_b64: &str, max_edge: u32) -> Option<String> {
    let bytes = decode_b64(data_b64)?;

    let img = image::load_from_memory(&bytes).ok()?;
    let (w, h) = (img.width(), img.height());
    if w <= max_edge && h <= max_edge {
        return None;
    }

    // `resize` preserves aspect ratio, fitting within the bounding box.
    let resized = img.resize(max_edge, max_edge, image::imageops::FilterType::Lanczos3);

    let format = if media_type.eq_ignore_ascii_case("image/jpeg")
        || media_type.eq_ignore_ascii_case("image/jpg")
    {
        image::ImageFormat::Jpeg
    } else {
        image::ImageFormat::Png
    };

    let mut out = std::io::Cursor::new(Vec::new());
    resized.write_to(&mut out, format).ok()?;
    Some(base64::engine::general_purpose::STANDARD.encode(out.into_inner()))
}

fn decode_b64(data_b64: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(data_b64.trim())
        .ok()
}

/// Cheap header-only dimension probe for PNG/JPEG/GIF, mirroring the probe used
/// when reading images. Returns `None` for unknown formats.
fn probe_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    // PNG: signature + IHDR chunk.
    if data.len() > 24 && &data[0..8] == b"\x89PNG\r\n\x1a\n" {
        let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        return Some((width, height));
    }

    // JPEG: scan for SOF0/SOF2 markers.
    if data.len() > 2 && data[0] == 0xFF && data[1] == 0xD8 {
        let mut i = 2;
        while i + 9 < data.len() {
            if data[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = data[i + 1];
            if marker == 0xC0 || marker == 0xC2 {
                let height = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
                let width = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
                return Some((width, height));
            }
            if i + 3 < data.len() {
                let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
                i += 2 + len;
            } else {
                break;
            }
        }
    }

    // GIF: header carries dimensions directly.
    if data.len() > 10 && (&data[0..6] == b"GIF87a" || &data[0..6] == b"GIF89a") {
        let width = u16::from_le_bytes([data[6], data[7]]) as u32;
        let height = u16::from_le_bytes([data[8], data[9]]) as u32;
        return Some((width, height));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, RgbImage};
    use jcode_message_types::Role;

    fn encode_png(w: u32, h: u32) -> String {
        let img = RgbImage::from_pixel(w, h, image::Rgb([10, 20, 30]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        base64::engine::general_purpose::STANDARD.encode(out.into_inner())
    }

    fn image_message(data: String) -> Message {
        Message {
            role: Role::User,
            content: vec![ContentBlock::Image {
                media_type: "image/png".to_string(),
                data,
            }],
            timestamp: None,
            tool_duration_ms: None,
        }
    }

    fn dims(data_b64: &str) -> (u32, u32) {
        let bytes = decode_b64(data_b64).unwrap();
        let img = image::load_from_memory(&bytes).unwrap();
        (img.width(), img.height())
    }

    #[test]
    fn no_images_returns_none() {
        let messages = vec![Message::user("hello")];
        assert!(clamp_outbound_images(&messages).is_none());
    }

    #[test]
    fn within_limits_returns_none() {
        let messages = vec![image_message(encode_png(100, 100))];
        assert!(clamp_outbound_images(&messages).is_none());
    }

    #[test]
    fn single_large_image_uses_few_image_limit() {
        // One image, larger than the many-image cap (2000) but under the
        // few-image cap (8000): must NOT be downscaled.
        let messages = vec![image_message(encode_png(3000, 1000))];
        assert!(clamp_outbound_images(&messages).is_none());
    }

    #[test]
    fn many_images_clamp_to_2000() {
        // 21 images, each 2500px wide -> exceeds the many-image 2000px cap.
        let messages: Vec<Message> = (0..21).map(|_| image_message(encode_png(2500, 1250))).collect();
        let clamped = clamp_outbound_images(&messages).expect("should clamp many oversized images");
        for m in &clamped {
            if let ContentBlock::Image { data, .. } = &m.content[0] {
                let (w, h) = dims(data);
                assert!(w <= 2000 && h <= 2000, "image not clamped: {w}x{h}");
                // Aspect ratio preserved (2:1).
                assert_eq!(w, 2000);
                assert_eq!(h, 1000);
            }
        }
    }

    #[test]
    fn few_images_clamp_to_8000() {
        let messages = vec![image_message(encode_png(9000, 3000))];
        let clamped = clamp_outbound_images(&messages).expect("should clamp oversized image");
        if let ContentBlock::Image { data, .. } = &clamped[0].content[0] {
            let (w, h) = dims(data);
            assert!(w <= 8000 && h <= 8000);
            assert_eq!(w, 8000);
            // 9000x3000 -> 8000 wide, height rounds to nearest.
            assert!((2666..=2667).contains(&h), "unexpected height {h}");
        }
    }
}
