# compress

Self-hosted batch image compressor for **PNG, JPEG and WebP**: one Rust binary with the web UI built in. It has no file-count or file-size quota and no account.

| Format | Engine | What it does |
|---|---|---|
| PNG  | [libimagequant](https://github.com/ImageOptim/libimagequant) + [oxipng](https://github.com/shssoichiro/oxipng) | Lossy 256-colour palette (the pngquant technique), then lossless deflate re-optimisation |
| JPEG | [mozjpeg](https://github.com/mozilla/mozjpeg) | Trellis quantisation, progressive, optimised Huffman tables |
| WebP | libwebp | Method 6, sharp YUV, alpha compression |

- EXIF orientation is applied to the pixels, then all metadata is stripped.
- If the result isn't smaller than the input (same format), the original is returned untouched.
- Files are processed in parallel: the browser sends several at once and the server runs one job per CPU core.

## Run

```bash
cargo run --release
```

Open http://127.0.0.1:8080, then drop files, paste (⌘V / Ctrl+V) or press ⌘O / Ctrl+O.

Building needs `cmake` and `nasm` (for mozjpeg's SIMD).

### Docker

```bash
docker build -t compress .
docker run -p 8080:8080 compress
```

### Settings (environment)

| Variable | Default | |
|---|---|---|
| `BIND` | `127.0.0.1:8080` | Listen address (`0.0.0.0:8080` to expose on the network) |
| `MAX_UPLOAD_MB` | `200` | Largest single file accepted |
| `WORKERS` | number of CPU cores | Images compressed at the same time |

## UI options

- **Output**: Same, WebP, PNG or JPEG. WebP is usually the smallest; photos saved as PNG shrink far more as WebP or JPEG.
- **Quality** (Smaller ↔ Sharper, default 80).
- **Lossless**: PNG is re-optimised without changing pixels; WebP uses lossless mode; JPEG has no lossless re-encode here, so it uses quality ≥ 95.

## API

```bash
curl --data-binary @photo.jpg "http://127.0.0.1:8080/api/compress?format=same&quality=80" -o out.jpg
```

Query: `format` (`same|png|jpeg|webp`), `quality` (1–100), `lossless` (`true|false`).
Response headers: `x-original-size`, `x-output-ext`, `x-kept-original`.
