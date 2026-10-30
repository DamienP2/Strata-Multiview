//! Animated pictures (GIF, APNG) split into PNG frames for the Image
//! widget's sequences: the engine then only reads still pictures.

use image::codecs::gif::GifDecoder;
use image::codecs::png::PngDecoder;
use image::{AnimationDecoder, ImageFormat};
use std::io::Cursor;

/// The frames (PNG files) of an animated GIF / APNG and its frame rate
/// (from the mean delay). A still picture gives one frame.
pub fn split(bytes: &[u8]) -> Result<(Vec<Vec<u8>>, f32), String> {
    let format = image::guess_format(bytes).map_err(|e| e.to_string())?;
    let frames = match format {
        ImageFormat::Gif => GifDecoder::new(Cursor::new(bytes))
            .map_err(|e| e.to_string())?
            .into_frames()
            .collect_frames()
            .map_err(|e| e.to_string())?,
        ImageFormat::Png => {
            let d = PngDecoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
            if d.is_apng().map_err(|e| e.to_string())? {
                d.apng()
                    .map_err(|e| e.to_string())?
                    .into_frames()
                    .collect_frames()
                    .map_err(|e| e.to_string())?
            } else {
                return Ok((vec![bytes.to_vec()], 1.0));
            }
        }
        other => {
            return Err(format!(
                "{other:?} is not an animated picture (GIF or APNG)"
            ))
        }
    };
    if frames.is_empty() {
        return Err("no frame in this picture".into());
    }
    if frames.len() > 2000 {
        return Err(format!("{} frames: at most 2000", frames.len()));
    }
    let mut total_ms = 0.0f64;
    let mut out = Vec::with_capacity(frames.len());
    for f in &frames {
        let (n, d) = f.delay().numer_denom_ms();
        // Browsers play 0 / 10 ms delays at 100 ms.
        let ms = if d == 0 { 100.0 } else { n as f64 / d as f64 };
        total_ms += if ms <= 10.0 { 100.0 } else { ms };
        let mut png = Vec::new();
        f.buffer()
            .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        out.push(png);
    }
    let fps = (frames.len() as f64 * 1000.0 / total_ms.max(1.0)).clamp(0.1, 120.0) as f32;
    Ok((out, fps))
}

/// Width and height of a picture file.
pub fn size(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::GifEncoder;
    use image::{Delay, Frame, Rgba, RgbaImage};

    #[test]
    fn a_gif_becomes_png_frames_at_its_rate() {
        let mut gif = Vec::new();
        {
            let mut enc = GifEncoder::new(&mut gif);
            for c in [255u8, 0, 128] {
                let img = RgbaImage::from_pixel(4, 4, Rgba([c, 0, 0, 255]));
                enc.encode_frame(Frame::from_parts(
                    img,
                    0,
                    0,
                    Delay::from_numer_denom_ms(50, 1),
                ))
                .unwrap();
            }
        }
        let (frames, fps) = split(&gif).unwrap();
        assert_eq!(frames.len(), 3);
        assert!((fps - 20.0).abs() < 0.5, "{fps}");
        assert_eq!(image::guess_format(&frames[0]).unwrap(), ImageFormat::Png);
        assert!(split(b"not a picture").is_err());
    }
}
