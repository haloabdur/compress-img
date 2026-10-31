# API compress-img

Kompres gambar **sebelum** disimpan atau di-upload ke sistem Anda. Kirim gambar ke compress-img, dan Anda langsung menerima file yang sudah terkompresi, siap di-upload ke storage (S3, disk, Cloudinary, dan sebagainya).

```
Sistem Anda ──(gambar asli)──▶ compress-img ──(gambar terkompresi)──▶ Sistem Anda ──▶ storage
```

- Satu endpoint: `POST /api/compress`
- Input: PNG, JPEG, WebP, dikirim sebagai body mentah **atau** form upload (`multipart/form-data`)
- Output: file gambar terkompresi (default), atau JSON berisi base64
- Tanpa batas jumlah atau kuota. Batas ukuran per file bisa diatur (default 200 MB)

---

## 1. Menjalankan compress-img sebagai server API

### Docker (paling mudah untuk server)

```bash
docker build -t compress-img .
docker run -d --name compress-img --restart unless-stopped \
  -p 8080:8080 \
  -e API_KEY=ganti-dengan-rahasia-panjang \
  compress-img
```

### Binary dari Releases (tanpa Docker)

```bash
BIND=0.0.0.0:8080 API_KEY=ganti-dengan-rahasia-panjang NO_BROWSER=1 ./compress-img
```

Di Windows (PowerShell):

```powershell
$env:BIND="0.0.0.0:8080"; $env:API_KEY="ganti-dengan-rahasia-panjang"; $env:NO_BROWSER="1"; .\compress-img.exe
```

### Pengaturan (environment variable)

| Variabel | Default | Keterangan |
|---|---|---|
| `BIND` | `127.0.0.1:8080` | Alamat listen. Pakai `0.0.0.0:8080` agar bisa diakses dari server atau perangkat lain |
| `API_KEY` | kosong (tanpa auth) | Jika diisi, setiap request ke `/api/compress` wajib membawa key ini |
| `CORS_ORIGINS` | `*` | Domain yang boleh memanggil API langsung dari browser, pisahkan dengan koma. Contoh: `https://app.saya.com,https://admin.saya.com` |
| `MAX_UPLOAD_MB` | `200` | Ukuran maksimum per file |
| `WORKERS` | jumlah core CPU | Jumlah gambar yang dikompres bersamaan. Request lain mengantre, tidak ditolak |
| `NO_BROWSER` | – | Isi apa saja agar browser tidak dibuka saat start (wajib di server) |

> **Penting:** jika compress-img bisa diakses dari internet, **selalu set `API_KEY`**. Kalau tidak, siapa pun bisa memakai CPU server Anda.

### Cek server hidup

```bash
curl http://localhost:8080/api/health
# {"status":"ok","version":"0.2.0","auth_required":true,"formats":["png","jpeg","webp"]}
```

---

## 2. Endpoint `POST /api/compress`

### Autentikasi (jika `API_KEY` di-set)

Pilih salah satu header:

```
Authorization: Bearer <API_KEY>
X-API-Key: <API_KEY>
```

### Cara mengirim gambar

**A. Form upload (`multipart/form-data`)**, paling umum. Gambar dikirim di field `file` (atau `image`). Opsi bisa dikirim sebagai field form atau query string.

**B. Body mentah.** Byte gambar langsung menjadi body request; opsi dikirim lewat query string.

### Opsi

| Opsi | Nilai | Default | Keterangan |
|---|---|---|---|
| `format` | `same`, `webp`, `jpeg`, `png` | `same` | `same` = format sama dengan input. `webp` biasanya paling kecil |
| `quality` | `1`–`100` | `80` | Lebih kecil berarti file lebih kecil. 80 ≈ setara TinyJPG/TinyPNG |
| `lossless` | `true` / `false` | `false` | PNG/WebP tanpa mengubah piksel. JPEG memakai kualitas ≥ 95 |
| `max_width` | angka (px) | – | Perkecil jika lebih lebar dari ini. Rasio tetap; gambar tidak pernah diperbesar |
| `max_height` | angka (px) | – | Sama seperti `max_width`, untuk tinggi |
| `filename` | teks | nama file upload | Nama file di header `Content-Disposition`. Ekstensi disesuaikan otomatis |
| `response` | `binary` / `json` | `binary` | `json` = hasil dikembalikan sebagai JSON + base64 |

Selain itu, rotasi EXIF (foto HP) diterapkan otomatis, dan semua metadata (EXIF, GPS) dihapus.

### Respons sukses (`200`, mode binary)

Body berisi **file gambar terkompresi**. Header yang berguna:

| Header | Contoh | Arti |
|---|---|---|
| `Content-Type` | `image/webp` | Tipe file hasil |
| `Content-Disposition` | `inline; filename="foto.webp"` | Nama file yang disarankan |
| `X-Original-Size` | `1848667` | Ukuran asli (byte) |
| `X-Compressed-Size` | `441608` | Ukuran hasil (byte) |
| `X-Saved-Percent` | `76` | Persentase penghematan |
| `X-Output-Ext` | `webp` | Ekstensi file hasil (`jpg`, `png`, `webp`) |
| `X-Kept-Original` | `0` / `1` | `1` = file asli sudah optimal, jadi yang dikembalikan adalah file asli |
| `X-Width`, `X-Height` | `1200`, `675` | Dimensi hasil |

### Respons sukses (mode `response=json`)

```json
{
  "filename": "foto.webp",
  "mime": "image/webp",
  "format": "webp",
  "original_size": 1848667,
  "size": 34932,
  "saved_percent": 98,
  "kept_original": false,
  "width": 1200,
  "height": 675,
  "data": "UklGRmSIAABXRUJQVlA4..."
}
```

`data` = file hasil dalam base64.

### Error

Semua error berbentuk JSON `{"error": "pesan"}`:

| Status | Penyebab |
|---|---|
| `400` | Tidak ada file, field `file` tidak ditemukan, atau `format` tidak valid |
| `401` | API key salah atau tidak dikirim |
| `413` | File melebihi `MAX_UPLOAD_MB` |
| `422` | Bukan PNG/JPEG/WebP, atau file rusak |

> **Tips:** jika compress-img error atau mati, sistem Anda sebaiknya tetap meng-upload file asli (fallback), supaya upload user tidak gagal. Semua contoh di bawah sudah memakai pola ini.

---

## 3. Contoh pemakaian

Di semua contoh, ganti `http://localhost:8080` dengan alamat server compress-img Anda, dan `RAHASIA` dengan `API_KEY` Anda.

### cURL

```bash
# Form upload, hasil disimpan ke file
curl -H "X-API-Key: RAHASIA" \
  -F "file=@foto.jpg" -F "format=webp" -F "max_width=1920" \
  http://localhost:8080/api/compress -o foto.webp

# Body mentah
curl -H "X-API-Key: RAHASIA" --data-binary @foto.jpg \
  "http://localhost:8080/api/compress?quality=80" -o foto-kecil.jpg
```

### Browser (JavaScript): kompres dulu di sisi user, lalu upload ke backend Anda

Gambar dikompres sebelum dikirim ke backend Anda, jadi upload user juga lebih cepat.
Set `CORS_ORIGINS` ke domain web Anda.

> Key yang ditaruh di kode browser bisa dilihat siapa pun. Untuk aplikasi publik, gunakan pola **backend** (Node/PHP/Python/Go di bawah) supaya `API_KEY` tetap rahasia. Pola browser cocok untuk aplikasi internal atau admin panel.

```html
<input type="file" id="foto" accept="image/png,image/jpeg,image/webp">
<script>
const COMPRESS_URL = 'http://localhost:8080/api/compress';
const COMPRESS_KEY = 'RAHASIA';

/** Mengembalikan File yang sudah dikompres (atau file asli jika gagal). */
async function compressImage(file, options = {}) {
  const form = new FormData();
  form.append('file', file);
  for (const [k, v] of Object.entries(options)) form.append(k, v);
  try {
    const res = await fetch(COMPRESS_URL, {
      method: 'POST',
      headers: { 'X-API-Key': COMPRESS_KEY },
      body: form,
    });
    if (!res.ok) throw new Error((await res.json()).error);
    const blob = await res.blob();
    const name = /filename="([^"]+)"/.exec(res.headers.get('Content-Disposition'))?.[1] || file.name;
    return new File([blob], name, { type: blob.type });
  } catch (e) {
    console.warn('compress-img gagal, pakai file asli:', e);
    return file;
  }
}

document.getElementById('foto').addEventListener('change', async (e) => {
  const original = e.target.files[0];
  const compressed = await compressImage(original, { format: 'webp', max_width: 1920 });
  console.log(`${original.size} -> ${compressed.size} byte`);

  // Upload ke backend Anda seperti biasa, tapi pakai file yang sudah dikompres:
  const form = new FormData();
  form.append('foto', compressed);
  await fetch('/upload', { method: 'POST', body: form });
});
</script>
```

### Node.js (Express + multer): terima upload, kompres, lalu simpan

Node 18+ (sudah ada `fetch`, `FormData`, `Blob`).

```js
import express from 'express';
import multer from 'multer';
import fs from 'node:fs/promises';

const COMPRESS_URL = process.env.COMPRESS_URL || 'http://localhost:8080/api/compress';
const COMPRESS_KEY = process.env.COMPRESS_KEY || 'RAHASIA';

/** Buffer + nama file -> { buffer, filename, mime } terkompresi (atau asli jika gagal). */
async function compressImage(buffer, filename, options = {}) {
  const form = new FormData();
  form.append('file', new Blob([buffer]), filename);
  for (const [k, v] of Object.entries(options)) form.append(k, String(v));
  try {
    const res = await fetch(COMPRESS_URL, {
      method: 'POST',
      headers: { 'X-API-Key': COMPRESS_KEY },
      body: form,
      signal: AbortSignal.timeout(60_000),
    });
    if (!res.ok) throw new Error((await res.json()).error);
    return {
      buffer: Buffer.from(await res.arrayBuffer()),
      filename: /filename="([^"]+)"/.exec(res.headers.get('content-disposition'))?.[1] ?? filename,
      mime: res.headers.get('content-type'),
    };
  } catch (e) {
    console.warn('compress-img gagal, pakai file asli:', e.message);
    return { buffer, filename, mime: null };
  }
}

const app = express();
const upload = multer({ storage: multer.memoryStorage() });

app.post('/upload', upload.single('foto'), async (req, res) => {
  const img = await compressImage(req.file.buffer, req.file.originalname, { max_width: 1920 });
  await fs.writeFile(`uploads/${Date.now()}-${img.filename}`, img.buffer); // atau upload ke S3, dll.
  res.json({ ok: true, size: img.buffer.length });
});

app.listen(3000);
```

### PHP: Laravel

```php
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Http;
use Illuminate\Support\Facades\Storage;

/**
 * Kompres file upload. Mengembalikan [isi file, nama file].
 * Jika compress-img gagal, mengembalikan file asli.
 */
function compressImage(\Illuminate\Http\UploadedFile $file, array $options = []): array
{
    try {
        $res = Http::withHeaders(['X-API-Key' => env('COMPRESS_KEY')])
            ->timeout(60)
            ->attach('file', $file->get(), $file->getClientOriginalName())
            ->post(env('COMPRESS_URL', 'http://localhost:8080/api/compress'), $options);

        if ($res->successful()) {
            preg_match('/filename="([^"]+)"/', $res->header('Content-Disposition'), $m);
            return [$res->body(), $m[1] ?? $file->getClientOriginalName()];
        }
        logger()->warning('compress-img: ' . $res->json('error'));
    } catch (\Throwable $e) {
        logger()->warning('compress-img gagal: ' . $e->getMessage());
    }
    return [$file->get(), $file->getClientOriginalName()];
}

// Di controller:
public function upload(Request $request)
{
    $request->validate(['foto' => 'required|image|mimes:jpeg,png,webp']);

    [$content, $name] = compressImage($request->file('foto'), ['max_width' => 1920]);
    $path = 'uploads/' . uniqid() . '-' . $name;
    Storage::disk('public')->put($path, $content); // atau disk 's3'

    return ['path' => $path, 'size' => strlen($content)];
}
```

`.env`:

```
COMPRESS_URL=http://localhost:8080/api/compress
COMPRESS_KEY=RAHASIA
```

### PHP murni (cURL)

```php
function compressImage(string $path, array $options = []): string
{
    $ch = curl_init('http://localhost:8080/api/compress');
    curl_setopt_array($ch, [
        CURLOPT_POST => true,
        CURLOPT_RETURNTRANSFER => true,
        CURLOPT_TIMEOUT => 60,
        CURLOPT_HTTPHEADER => ['X-API-Key: RAHASIA'],
        CURLOPT_POSTFIELDS => ['file' => new CURLFile($path)] + $options,
    ]);
    $body = curl_exec($ch);
    $status = curl_getinfo($ch, CURLINFO_HTTP_CODE);
    curl_close($ch);
    return ($status === 200 && $body !== false) ? $body : file_get_contents($path); // fallback ke asli
}

// $_FILES['foto'] dari form upload
$compressed = compressImage($_FILES['foto']['tmp_name'], ['max_width' => 1920]);
file_put_contents('uploads/' . basename($_FILES['foto']['name']), $compressed);
```

### Python (requests / Django / FastAPI)

```python
import requests

COMPRESS_URL = "http://localhost:8080/api/compress"
COMPRESS_KEY = "RAHASIA"

def compress_image(data: bytes, filename: str, **options) -> tuple[bytes, str]:
    """Kembalikan (bytes terkompresi, nama file). Jika gagal, kembalikan yang asli."""
    try:
        r = requests.post(
            COMPRESS_URL,
            headers={"X-API-Key": COMPRESS_KEY},
            files={"file": (filename, data)},
            data=options,
            timeout=60,
        )
        r.raise_for_status()
        name = r.headers.get("Content-Disposition", "").split('filename="')[-1].split('"')[0] or filename
        return r.content, name
    except Exception as e:
        print("compress-img gagal, pakai file asli:", e)
        return data, filename

# Contoh:
with open("foto.jpg", "rb") as f:
    data, name = compress_image(f.read(), "foto.jpg", format="webp", max_width=1920)
open(name, "wb").write(data)
```

### Go

```go
package compress

import (
	"bytes"
	"fmt"
	"io"
	"mime/multipart"
	"net/http"
	"time"
)

var client = &http.Client{Timeout: 60 * time.Second}

// Image mengirim gambar ke compress-img dan mengembalikan hasilnya.
// Jika gagal, error dikembalikan; pemanggil sebaiknya memakai data asli.
func Image(data []byte, filename string, opts map[string]string) ([]byte, string, error) {
	var buf bytes.Buffer
	w := multipart.NewWriter(&buf)
	fw, _ := w.CreateFormFile("file", filename)
	fw.Write(data)
	for k, v := range opts {
		w.WriteField(k, v)
	}
	w.Close()

	req, _ := http.NewRequest("POST", "http://localhost:8080/api/compress", &buf)
	req.Header.Set("Content-Type", w.FormDataContentType())
	req.Header.Set("X-API-Key", "RAHASIA")
	res, err := client.Do(req)
	if err != nil {
		return nil, "", err
	}
	defer res.Body.Close()
	body, _ := io.ReadAll(res.Body)
	if res.StatusCode != http.StatusOK {
		return nil, "", fmt.Errorf("compress-img %d: %s", res.StatusCode, body)
	}
	return body, res.Header.Get("Content-Disposition"), nil
}
```

---

## 4. Tips produksi

- **Taruh compress-img di jaringan internal** (satu server, satu Docker network, atau VPN) dengan sistem Anda, lalu panggil dari backend. Lebih cepat dan aman.
- **Nginx di depannya?** Naikkan batas upload dan timeout:

  ```nginx
  location /api/ {
      proxy_pass http://127.0.0.1:8080;
      client_max_body_size 50m;
      proxy_read_timeout 120s;
  }
  ```

- **Lama proses:** foto HP 12 MP biasanya selesai dalam 0,5–1 detik untuk JPEG, dan 1–3 detik untuk WebP atau PNG. Screenshot PNG kecil bisa sampai sekitar 3 detik karena kompresi Zopfli ekstra. Set timeout klien ≥ 60 detik.
- **Mempercepat:** kirim `max_width=1920` (atau ukuran yang memang dipakai web Anda). Selain file jauh lebih kecil, proses juga lebih cepat.
- **Format terbaik untuk web:** `format=webp`, didukung semua browser modern dan biasanya 25–35% lebih kecil dari JPEG.
- **Banyak request bersamaan:** aman. Server memproses `WORKERS` gambar sekaligus, dan sisanya otomatis mengantre.
