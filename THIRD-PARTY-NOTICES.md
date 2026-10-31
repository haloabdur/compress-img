# Third-party notices

The source code in this repository is MIT licensed (see `LICENSE`).
The prebuilt binaries statically link the libraries below.

| Library | Used for | License |
|---|---|---|
| [libimagequant](https://github.com/ImageOptim/libimagequant) | PNG palette quantization | **GPL-3.0-or-later** |
| [mozjpeg](https://github.com/mozilla/mozjpeg) | JPEG encoding | IJG, BSD-3-Clause, zlib |
| [libwebp](https://chromium.googlesource.com/webm/libwebp) | WebP encoding | BSD-3-Clause |
| [oxipng](https://github.com/shssoichiro/oxipng) | PNG optimization | MIT |
| [image](https://github.com/image-rs/image) | Decoding | MIT OR Apache-2.0 |
| [axum](https://github.com/tokio-rs/axum) / [tokio](https://github.com/tokio-rs/tokio) | Web server | MIT |

Because libimagequant is GPL-3.0, **the combined prebuilt binaries are
distributed under GPL-3.0-or-later**. The corresponding source is this
repository at the matching tag. Personal or internal use is unaffected. If you
need a binary under MIT terms only, replace `imagequant` with a permissively
licensed quantizer or obtain a commercial libimagequant license.
