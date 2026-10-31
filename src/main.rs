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

    let addr: SocketAddr = std::env::var("BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".into())
        .parse()
        .expect("BIND must look like 127.0.0.1:8080");
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

    tracing::info!("compress listening on http://{addr} ({workers} workers, {max_mb} MB max upload)");
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind failed");
    axum::serve(listener, app).await.expect("server error");
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
