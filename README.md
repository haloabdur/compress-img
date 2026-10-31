# compress

Batch image compressor for **PNG, JPEG and WebP** that runs on your own computer.
It's one small app with no account, no upload quota, and no limit on file count or size.

Drop files, paste from the clipboard or pick a folder's worth of images, then download them one by one or all at once as a `.zip`.

## Download and run

Get the archive for your system from the [latest release](https://github.com/haloabdur/compress-img/releases/latest):

| System | File |
|---|---|
| Windows 10/11 (64-bit) | `compress-img-vX.Y.Z-windows-x64.zip` |
| macOS, Apple Silicon (M1–M4) | `compress-img-vX.Y.Z-macos-arm64.tar.gz` |
| macOS, Intel | `compress-img-vX.Y.Z-macos-x64.tar.gz` |
| Linux x86-64 | `compress-img-vX.Y.Z-linux-x64.tar.gz` |
| Linux ARM64 (Raspberry Pi 4/5, etc.) | `compress-img-vX.Y.Z-linux-arm64.tar.gz` |

Unzip it and start `compress-img` (`compress-img.exe` on Windows). It needs no installer.
Your browser opens to the app at `http://127.0.0.1:8080`. To stop it, close the terminal window or press Ctrl+C.

- **Windows**: if SmartScreen says "Windows protected your PC", click *More info*, then *Run anyway*. The binary isn't code-signed.
- **macOS**: the app isn't notarized. Either right-click → *Open* the first time, or run
  `xattr -d com.apple.quarantine ./compress-img`.
- **Linux**: `./compress-img`

If port 8080 is busy, the app picks the next free port and prints the address.

## Using it

| | |
|---|---|
| Add files | Drag and drop anywhere, ⌘V / Ctrl+V to paste, ⌘O / Ctrl+O to pick |
| **Output** | *Same* keeps each file's format, or convert everything to WebP, PNG or JPEG |
| **Smaller ↔ Sharper** | Quality, default 80 (JPEG encodes at slider − 5, so q75 by default) |
| **Lossless** | PNG: re-optimized without changing a pixel. WebP: lossless mode. JPEG: quality ≥ 95 (JPEG has no true lossless re-encode here) |
| **Re-run** | Re-compress everything already in the list with the current settings |
| **Download all** | Every result in one `.zip` |

Settings are remembered between visits.

## How it compresses

| Format | Engine | Technique |
|---|---|---|
| PNG  | [libimagequant](https://github.com/ImageOptim/libimagequant) + [oxipng](https://github.com/shssoichiro/oxipng) | Lossy reduction to a 256-colour palette with light dithering (the pngquant technique TinyPNG is built on), then lossless deflate re-optimization, plus a Zopfli pass for screenshots and graphics |
| JPEG | [mozjpeg](https://github.com/mozilla/mozjpeg) | Trellis quantization, progressive scans, optimized Huffman tables. The default setting encodes at mozjpeg q75, close to where TinyJPG output lands (≈ q72–75 in published comparisons); 4:4:4 chroma at quality ≥ 90 |
| WebP | [libwebp](https://chromium.googlesource.com/webm/libwebp) | Method 6 (slowest, smallest), sharp RGB→YUV, compressed alpha |

- EXIF orientation is applied to the pixels, then all metadata (EXIF, GPS, comments) is stripped, so phone photos never end up rotated.
- If re-encoding to the same format wouldn't make the file smaller, the original is returned untouched ("Already optimal — kept").
- Several files upload at once, and the server compresses one image per CPU core.

### Measured results (default settings)

| File | Original | Same format | → WebP |
|---|---|---|---|
| Lake photo, 4000×2250 JPEG | 1.8 MB | **−77%** (0.6 s) | −75% |
| Same photo saved at JPEG q95 | 2.4 MB | **−82%** | −78% |
| Same photo saved as PNG | 8.2 MB | −46% | **−95%** |
| UI screenshot PNG with transparency | 11 KB | **−67%** | — |

PNG is the wrong format for photos. Convert those to WebP or JPEG for much smaller files.

### Comparing with TinyJPG / TinyPNG / compresspng.com

1. Pick 5–10 of your real images: phone photos, screenshots, graphics with transparency.
2. Run them through the online service and download the results.
3. Drop the same originals into compress with **Output: Same** and the default quality, then *Download all*.
4. Compare file sizes, then open matching pairs side by side at 100% zoom.

If compress comes out larger on some kind of image, move the slider toward *Smaller*. Please open an issue with the numbers.

## Run from source

Requires [Rust](https://rustup.rs) 1.85+, `cmake` and `nasm`.

```bash
cargo run --release
```

### Docker / server

```bash
docker build -t compress .
docker run -p 8080:8080 compress
```

### Options

```
compress-img [--no-browser] [--version] [--help]
```

| Environment variable | Default | |
|---|---|---|
| `BIND` | `127.0.0.1:8080` | Listen address. Use `0.0.0.0:8080` to use it from other devices on your network |
| `MAX_UPLOAD_MB` | `200` | Largest single file accepted |
| `WORKERS` | CPU cores | Images compressed at the same time |
| `NO_BROWSER` | unset | Set to anything to skip opening the browser |

## HTTP API

```bash
curl --data-binary @photo.jpg \
  "http://127.0.0.1:8080/api/compress?format=same&quality=80" -o photo.min.jpg
```

| Query | Values |
|---|---|
| `format` | `same` (default), `png`, `jpeg`, `webp` |
| `quality` | `1`–`100`, default `80` |
| `lossless` | `true` / `false` |

The response body is the compressed image. Headers: `x-original-size`, `x-output-ext`, `x-kept-original` (`1` when the original was returned).

## Releasing

Bump `version` in `Cargo.toml` and push to `main`. GitHub Actions builds Windows, macOS and Linux, creates the `v<version>` tag and publishes the release. Pushes that don't change the version skip the release.

## License

Source code: [MIT](LICENSE).

The prebuilt binaries include libimagequant, which is GPL-3.0, so the binaries as distributed are GPL-3.0-or-later. See [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
