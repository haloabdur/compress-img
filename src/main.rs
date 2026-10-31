mod compress;

use std::{convert::Infallible, net::SocketAddr, sync::Arc};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use base64::Engine;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Semaphore;
use tower_http::cors::{AllowOrigin, CorsLayer};

use compress::{Format, Options};

const INDEX: &str = include_str!("../static/index.html");

#[derive(Clone)]
struct AppState {
    // Bounds CPU-heavy work to one job per core; extra requests wait in line.
    workers: Arc<Semaphore>,
    api_key: Option<Arc<str>>,
}

/// Options accepted in the query string, and (for multipart uploads) as form fields.
#[derive(Deserialize, Default, Clone)]
struct Params {
    /// same | png | jpeg | webp
    format: Option<String>,
    quality: Option<u8>,
    lossless: Option<bool>,
    max_width: Option<u32>,
    max_height: Option<u32>,
    /// Used for the Content-Disposition filename of a raw-body upload.
    filename: Option<String>,
    /// binary (default) | json
    response: Option<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "compress_img=info".into()),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("compress-img {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{HELP}");
        return;
    }
    let no_browser = args.iter().any(|a| a == "--no-browser") || std::env::var_os("NO_BROWSER").is_some();
    let explicit_bind = std::env::var("BIND").ok();

    let max_mb: usize = std::env::var("MAX_UPLOAD_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);
    let workers = std::env::var("WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(num_cpus::get)
        .max(1);
    let api_key = std::env::var("API_KEY").ok().filter(|k| !k.is_empty()).map(Arc::from);

    let state = AppState { workers: Arc::new(Semaphore::new(workers)), api_key: api_key.clone() };
    let app = Router::new()
        .route("/", get(|| async { Html(INDEX) }))
        .route("/api/health", get(health))
        .route("/api/compress", post(compress_handler))
        .layer(DefaultBodyLimit::max(max_mb * 1024 * 1024))
        .layer(cors_layer())
        .with_state(state);

    let listener = bind(explicit_bind.as_deref()).await;
    let addr = listener.local_addr().expect("local addr");
    let url = if addr.ip().is_unspecified() {
        format!("http://127.0.0.1:{}", addr.port())
    } else {
        format!("http://{addr}")
    };
    println!();
    println!("  compress {} is running at {url}", env!("CARGO_PKG_VERSION"));
    println!("  API: POST {url}/api/compress{}", if api_key.is_some() { " (API key required)" } else { "" });
    println!("  {workers} workers · {max_mb} MB max per file · close this window or press Ctrl+C to stop");
    println!();
    if !no_browser {
        let _ = open::that_detached(&url);
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(async { let _ = tokio::signal::ctrl_c().await; })
        .await
        .expect("server error");
}

const HELP: &str = "compress-img: batch PNG / JPEG / WebP compressor with a web UI and HTTP API

USAGE: compress-img [--no-browser] [--version]

ENVIRONMENT:
  BIND=127.0.0.1:8080   listen address (0.0.0.0:8080 to share on your network)
  API_KEY=<secret>      require this key (Authorization: Bearer / X-API-Key) on /api/compress
  CORS_ORIGINS=*        comma-separated origins allowed to call the API from a browser
  MAX_UPLOAD_MB=200     largest single file accepted
  WORKERS=<cores>       images compressed at the same time
  NO_BROWSER=1          don't open the browser on start

API docs: docs/API.md";

/// Binds the explicit address, or 127.0.0.1:8080 falling back to the next
/// free port so a double-clicked app never fails just because 8080 is busy.
async fn bind(explicit: Option<&str>) -> tokio::net::TcpListener {
    if let Some(a) = explicit {
        let addr: SocketAddr = a.parse().expect("BIND must look like 127.0.0.1:8080");
        return tokio::net::TcpListener::bind(addr)
            .await
            .unwrap_or_else(|e| panic!("cannot listen on {addr}: {e}"));
    }
    for port in (8080..8100).chain(std::iter::once(0)) {
        if let Ok(l) = tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
            return l;
        }
    }
    panic!("no free port found");
}

fn cors_layer() -> CorsLayer {
    let origins = std::env::var("CORS_ORIGINS").unwrap_or_else(|_| "*".into());
    let allow = if origins.trim() == "*" {
        AllowOrigin::any()
    } else {
        AllowOrigin::list(origins.split(',').filter_map(|o| HeaderValue::from_str(o.trim()).ok()))
    };
    let expose = [
        "content-disposition",
        "x-original-size",
        "x-compressed-size",
        "x-saved-percent",
        "x-output-ext",
        "x-kept-original",
        "x-width",
        "x-height",
    ]
    .map(HeaderName::from_static);
    CorsLayer::new()
        .allow_origin(allow)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, HeaderName::from_static("x-api-key")])
        .expose_headers(expose)
}

async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "auth_required": state.api_key.is_some(),
        "formats": ["png", "jpeg", "webp"],
    }))
}

fn authorized(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(key) = state.api_key.as_deref() else { return true };
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .or_else(|| headers.get("x-api-key").and_then(|v| v.to_str().ok()))
        .unwrap_or("");
    // Constant-time comparison.
    let (a, b) = (key.as_bytes(), given.trim().as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn compress_handler(
    State(state): State<AppState>,
    Query(mut p): Query<Params>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !authorized(&state, &headers) {
        return err(StatusCode::UNAUTHORIZED, "missing or invalid API key");
    }

    let content_type = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
    let data = if content_type.starts_with("multipart/form-data") {
        match read_multipart(content_type, body, &mut p).await {
            Ok(d) => d,
            Err(e) => return err(StatusCode::BAD_REQUEST, &e),
        }
    } else {
        body
    };
    if data.is_empty() {
        return err(StatusCode::BAD_REQUEST, "no image: send the file as the request body or as a multipart field named \"file\"");
    }

    let output = match p.format.as_deref().map(str::trim) {
        None | Some("") | Some("same") | Some("auto") => None,
        Some(f) => match Format::parse(f) {
            Some(f) => Some(f),
            None => return err(StatusCode::BAD_REQUEST, "format must be same, png, jpeg or webp"),
        },
    };
    let opts = Options {
        output,
        quality: p.quality.unwrap_or(80).clamp(1, 100),
        lossless: p.lossless.unwrap_or(false),
        max_width: p.max_width,
        max_height: p.max_height,
    };

    let _permit = state.workers.acquire().await.expect("semaphore closed");
    let original = data.len();
    let result = tokio::task::spawn_blocking(move || compress::compress(&data, opts)).await;

    let out = match result {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return err(StatusCode::UNPROCESSABLE_ENTITY, &e),
        Err(_) => return err(StatusCode::INTERNAL_SERVER_ERROR, "compression crashed"),
    };

    let size = out.data.len();
    let saved = if original > 0 { ((original as f64 - size as f64) / original as f64 * 100.0).round() as i64 } else { 0 };
    let filename = output_name(p.filename.as_deref(), out.format);

    if p.response.as_deref() == Some("json") {
        return Json(json!({
            "filename": filename,
            "mime": out.format.mime(),
            "format": out.format.ext(),
            "original_size": original,
            "size": size,
            "saved_percent": saved,
            "kept_original": out.kept_original,
            "width": out.width,
            "height": out.height,
            "data": base64::engine::general_purpose::STANDARD.encode(&out.data),
        }))
        .into_response();
    }

    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(out.format.mime()));
    h.insert(header::CONTENT_DISPOSITION, content_disposition(&filename));
    h.insert("x-original-size", original.into());
    h.insert("x-compressed-size", size.into());
    h.insert("x-saved-percent", saved.into());
    h.insert("x-output-ext", HeaderValue::from_static(out.format.ext()));
    h.insert("x-kept-original", HeaderValue::from_static(if out.kept_original { "1" } else { "0" }));
    h.insert("x-width", out.width.into());
    h.insert("x-height", out.height.into());
    (h, out.data).into_response()
}

/// Reads a multipart body: the first file field (preferably named "file" or
/// "image") is the image; other text fields override the query options.
async fn read_multipart(content_type: &str, body: Bytes, p: &mut Params) -> Result<Bytes, String> {
    let boundary = multer::parse_boundary(content_type).map_err(|e| e.to_string())?;
    let stream = futures_util::stream::once(async move { Ok::<_, Infallible>(body) });
    let mut mp = multer::Multipart::new(stream, boundary);
    let mut image: Option<(Bytes, Option<String>)> = None;

    while let Some(field) = mp.next_field().await.map_err(|e| e.to_string())? {
        let name = field.name().unwrap_or("").to_string();
        let file_name = field.file_name().map(str::to_string);
        if file_name.is_some() || name == "file" || name == "image" {
            let bytes = field.bytes().await.map_err(|e| e.to_string())?;
            let preferred = name == "file" || name == "image";
            if image.is_none() || preferred {
                image = Some((bytes, file_name));
            }
            continue;
        }
        let text = field.text().await.map_err(|e| e.to_string())?;
        let text = text.trim().to_string();
        match name.as_str() {
            "format" => p.format = Some(text),
            "quality" => p.quality = text.parse().ok().or(p.quality),
            "lossless" => p.lossless = Some(matches!(text.as_str(), "true" | "1" | "on" | "yes")),
            "max_width" => p.max_width = text.parse().ok().or(p.max_width),
            "max_height" => p.max_height = text.parse().ok().or(p.max_height),
            "filename" => p.filename = Some(text),
            "response" => p.response = Some(text),
            _ => {}
        }
    }

    let (bytes, file_name) = image.ok_or("multipart body has no file field (use the name \"file\")")?;
    if p.filename.is_none() {
        p.filename = file_name;
    }
    Ok(bytes)
}

/// `photo.PNG` + WebP output -> `photo.webp`; no name -> `image.webp`.
fn output_name(input: Option<&str>, fmt: Format) -> String {
    let base = input
        .map(|n| n.rsplit(['/', '\\']).next().unwrap_or(n))
        .map(|n| match n.rfind('.') {
            Some(i) if i > 0 => &n[..i],
            _ => n,
        })
        .filter(|b| !b.trim().is_empty())
        .unwrap_or("image");
    format!("{base}.{}", fmt.ext())
}

fn content_disposition(filename: &str) -> HeaderValue {
    let ascii: String = filename
        .chars()
        .map(|c| if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' { c } else { '_' })
        .collect();
    let mut encoded = String::new();
    for b in filename.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    HeaderValue::from_str(&format!("inline; filename=\"{ascii}\"; filename*=UTF-8''{encoded}"))
        .unwrap_or_else(|_| HeaderValue::from_static("inline"))
}

fn err(code: StatusCode, msg: &str) -> Response {
    (code, Json(json!({ "error": msg }))).into_response()
}
