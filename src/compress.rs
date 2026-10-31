//! Image compression pipeline.
//!
//! - PNG  : libimagequant (palette quantization, the pngquant engine) + oxipng
//! - JPEG : mozjpeg (trellis quantization, optimized progressive scans)
//! - WebP : libwebp (method 6, sharp YUV)
//!
//! EXIF orientation is applied to the pixels before re-encoding, so stripping
//! metadata never rotates a photo.

use std::io::Cursor;

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, RgbaImage};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    Webp,
}

impl Format {
    pub fn mime(self) -> &'static str {
        match self {
            Format::Png => "image/png",
            Format::Jpeg => "image/jpeg",
            Format::Webp => "image/webp",
        }
    }
    pub fn ext(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Webp => "webp",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "png" => Some(Format::Png),
            "jpg" | "jpeg" => Some(Format::Jpeg),
            "webp" => Some(Format::Webp),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// None = keep the input format.
    pub output: Option<Format>,
    /// 1..=100
    pub quality: u8,
    pub lossless: bool,
}

pub struct Output {
    pub data: Vec<u8>,
    pub format: Format,
    /// True when the original was returned because re-encoding made it bigger.
    pub kept_original: bool,
}

pub fn detect(data: &[u8]) -> Option<Format> {
    match image::guess_format(data).ok()? {
        ImageFormat::Png => Some(Format::Png),
        ImageFormat::Jpeg => Some(Format::Jpeg),
        ImageFormat::WebP => Some(Format::Webp),
        _ => None,
    }
}

pub fn compress(input: &[u8], opts: Options) -> Result<Output, String> {
    let in_fmt = detect(input).ok_or("unsupported file: only PNG, JPEG and WebP are accepted")?;
    let out_fmt = opts.output.unwrap_or(in_fmt);
    let quality = opts.quality.clamp(1, 100);

    let data = match out_fmt {
        Format::Png => {
            if opts.lossless && in_fmt == Format::Png {
                optimize_png(input, 3)?
            } else {
                let img = decode(input)?;
                if opts.lossless {
                    optimize_png(&encode_png_rgba(&img.to_rgba8())?, 3)?
                } else {
                    quantize_png(&img.to_rgba8(), quality)?
                }
            }
        }
        Format::Jpeg => {
            let img = decode(input)?;
            // JPEG has no lossless mode here; "lossless" means near-visually-lossless.
            let q = if opts.lossless { quality.max(95) } else { jpeg_target(quality) };
            encode_jpeg(&img, q)?
        }
        Format::Webp => {
            let img = decode(input)?;
            encode_webp(&img.to_rgba8(), quality, opts.lossless)?
        }
    };

    if out_fmt == in_fmt && data.len() >= input.len() {
        return Ok(Output { data: input.to_vec(), format: in_fmt, kept_original: true });
    }
    Ok(Output { data, format: out_fmt, kept_original: false })
}

fn decode(input: &[u8]) -> Result<DynamicImage, String> {
    let reader = ImageReader::new(Cursor::new(input))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().ok();
    let mut img = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    Ok(img)
}

// ---------- PNG ----------

fn quantize_png(img: &RgbaImage, quality: u8) -> Result<Vec<u8>, String> {
    let (w, h) = img.dimensions();
    let pixels: Vec<imagequant::RGBA> = img
        .as_raw()
        .chunks_exact(4)
        .map(|p| imagequant::RGBA::new(p[0], p[1], p[2], p[3]))
        .collect();

    let mut liq = imagequant::new();
    liq.set_speed(3).map_err(|e| e.to_string())?;
    liq.set_quality(0, png_target(quality)).map_err(|e| e.to_string())?;
    let mut qimg = liq
        .new_image(&pixels[..], w as usize, h as usize, 0.0)
        .map_err(|e| e.to_string())?;
    let mut res = liq.quantize(&mut qimg).map_err(|e| e.to_string())?;
    res.set_dithering_level(0.5).map_err(|e| e.to_string())?;
    let (palette, indexes) = res.remapped(&mut qimg).map_err(|e| e.to_string())?;

    let mut rgb = Vec::with_capacity(palette.len() * 3);
    let mut trns = Vec::with_capacity(palette.len());
    for c in &palette {
        rgb.extend_from_slice(&[c.r, c.g, c.b]);
        trns.push(c.a);
    }
    // Only the leading run of non-opaque entries is needed in tRNS.
    while trns.last() == Some(&255) {
        trns.pop();
    }

    let mut buf = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut buf, w, h);
        enc.set_color(png::ColorType::Indexed);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_palette(rgb);
        if !trns.is_empty() {
            enc.set_trns(trns);
        }
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&indexes).map_err(|e| e.to_string())?;
    }
    // Deeper deflate search on normal-sized images; lighter on huge ones.
    let preset = if (w as u64) * (h as u64) <= 4_000_000 { 2 } else { 1 };
    let out = optimize_png(&buf, preset)?;
    // Screenshots and graphics usually end up small: a Zopfli pass shaves
    // another ~10-15% there for a few seconds of CPU. On big photo-like
    // PNGs it gains <1% at 5-8x the time, so skip it.
    if out.len() <= ZOPFLI_MAX_BYTES && (w as u64) * (h as u64) <= 3_000_000 {
        if let Ok(z) = optimize_png_zopfli(&out) {
            if z.len() < out.len() {
                return Ok(z);
            }
        }
    }
    Ok(out)
}

const ZOPFLI_MAX_BYTES: usize = 400_000;

fn optimize_png_zopfli(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut o = oxipng::Options::from_preset(2);
    o.strip = oxipng::StripChunks::Safe;
    o.optimize_alpha = true;
    o.deflater = oxipng::Deflater::Zopfli(oxipng::ZopfliOptions {
        iteration_count: std::num::NonZeroU64::new(5).unwrap(),
        ..Default::default()
    });
    oxipng::optimize_from_memory(data, &o).map_err(|e| e.to_string())
}

fn encode_png_rgba(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut buf, img.width(), img.height());
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fastest);
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(img.as_raw()).map_err(|e| e.to_string())?;
    }
    Ok(buf)
}

fn optimize_png(data: &[u8], preset: u8) -> Result<Vec<u8>, String> {
    let mut o = oxipng::Options::from_preset(preset);
    o.strip = oxipng::StripChunks::Safe;
    o.optimize_alpha = true;
    oxipng::optimize_from_memory(data, &o).map_err(|e| e.to_string())
}

// ---------- JPEG ----------

fn encode_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, String> {
    // Flatten transparency onto white.
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for p in rgba.as_raw().chunks_exact(4) {
        let a = p[3] as u32;
        for c in &p[..3] {
            rgb.push(((*c as u32 * a + 255 * (255 - a) + 127) / 255) as u8);
        }
    }

    std::panic::catch_unwind(move || -> Result<Vec<u8>, String> {
        let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
        comp.set_size(w as usize, h as usize);
        comp.set_quality(quality as f32);
        comp.set_progressive_mode();
        comp.set_optimize_scans(true);
        comp.set_optimize_coding(true);
        if quality >= 90 {
            // Keep full colour resolution at high quality.
            for c in comp.components_mut() {
                c.h_samp_factor = 1;
                c.v_samp_factor = 1;
            }
        }
        let mut started = comp.start_compress(Vec::new()).map_err(|e| e.to_string())?;
        started.write_scanlines(&rgb).map_err(|e| e.to_string())?;
        started.finish().map_err(|e| e.to_string())
    })
    .map_err(|_| "JPEG encoder failed".to_string())?
}

// ---------- WebP ----------

fn encode_webp(img: &RgbaImage, quality: u8, lossless: bool) -> Result<Vec<u8>, String> {
    let has_alpha = img.as_raw().chunks_exact(4).any(|p| p[3] != 255);
    let (w, h) = img.dimensions();
    let rgb_buf;
    let enc = if has_alpha {
        webp::Encoder::from_rgba(img.as_raw(), w, h)
    } else {
        rgb_buf = DynamicImage::ImageRgba8(img.clone()).to_rgb8();
        webp::Encoder::from_rgb(rgb_buf.as_raw(), w, h)
    };
    let mut cfg = webp::WebPConfig::new().map_err(|_| "webp config")?;
    cfg.method = 6;
    if lossless {
        cfg.lossless = 1;
        cfg.quality = 100.0;
        cfg.exact = 0;
    } else {
        cfg.quality = quality as f32;
        cfg.use_sharp_yuv = 1;
        cfg.alpha_compression = 1;
        cfg.alpha_quality = 90;
    }
    let mem = enc
        .encode_advanced(&cfg)
        .map_err(|e| format!("WebP encoder failed: {e:?}"))?;
    Ok(mem.to_vec())
}

/// Maps the UI quality to mozjpeg quality. TinyJPG output lands between
/// mozjpeg q70 and q80 in published comparisons (~q72-75), so the UI default
/// of 80 encodes at q75, mozjpeg's own default.
fn jpeg_target(quality: u8) -> u8 {
    quality.saturating_sub(5).max(1)
}

/// Maps the UI quality (40..=95) to libimagequant's target quality.
/// imagequant stops adding palette colours once the target is met, so a raw
/// "80" collapses noisy photos to a few dozen colours. Shifting the scale up
/// keeps the full 256-colour palette at the default setting, like TinyPNG.
fn png_target(quality: u8) -> u8 {
    (quality as u16 + 20).min(100) as u8
}
