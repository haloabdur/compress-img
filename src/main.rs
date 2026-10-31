mod compress;

use std::{net::SocketAddr, sync::Arc};

use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use tokio::sync::Semaphore;

use compress::{Format, Options};

const INDEX: &str = include_str!("../static/index.html");

#[derive(Clone)]
struct AppState {
    // Bounds CPU-heavy work to one job per core; extra requests wait in line.
    workers: Arc<Semaphore>,
}

#[derive(Deserialize)]
struct Params {
    /// same | png | jpeg | webp
    format: Option<String>,
    quality: Option<u8>,
    lossless: Option<bool>,
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

    let state = AppState { workers: Arc::new(Semaphore::new(workers)) };
    let app = Router::new()
        .route("/", get(|| async { Html(INDEX) }))
        .route("/api/compress", post(compress_handler))
        .layer(DefaultBodyLimit::max(max_mb * 1024 * 1024))
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

const HELP: &str = "compress-img: batch PNG / JPEG / WebP compressor with a web UI

USAGE: compress-img [--no-browser] [--version]

ENVIRONMENT:
  BIND=127.0.0.1:8080   listen address (0.0.0.0:8080 to share on your network)
  MAX_UPLOAD_MB=200     largest single file accepted
  WORKERS=<cores>       images compressed at the same time
  NO_BROWSER=1          don't open the browser on start";

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

async fn compress_handler(
    State(state): State<AppState>,
    Query(p): Query<Params>,
    body: Bytes,
) -> Response {
    let output = match p.format.as_deref() {
        None | Some("") | Some("same") => None,
        Some(f) => match Format::parse(f) {
            Some(f) => Some(f),
            None => return err(StatusCode::BAD_REQUEST, "format must be same, png, jpeg or webp"),
        },
    };
    let opts = Options {
        output,
        quality: p.quality.unwrap_or(80),
        lossless: p.lossless.unwrap_or(false),
    };
    if body.is_empty() {
        return err(StatusCode::BAD_REQUEST, "empty body");
    }

    let _permit = state.workers.acquire().await.expect("semaphore closed");
    let original = body.len();
    let result = tokio::task::spawn_blocking(move || compress::compress(&body, opts)).await;

    match result {
        Ok(Ok(out)) => {
            let mut h = HeaderMap::new();
            h.insert(header::CONTENT_TYPE, HeaderValue::from_static(out.format.mime()));
            h.insert("x-original-size", original.into());
            h.insert("x-output-ext", HeaderValue::from_static(out.format.ext()));
            h.insert("x-kept-original", HeaderValue::from_static(if out.kept_original { "1" } else { "0" }));
            (h, out.data).into_response()
        }
        Ok(Err(e)) => err(StatusCode::UNPROCESSABLE_ENTITY, &e),
        Err(_) => err(StatusCode::INTERNAL_SERVER_ERROR, "compression crashed"),
    }
}

fn err(code: StatusCode, msg: &str) -> Response {
    (code, msg.to_string()).into_response()
}
