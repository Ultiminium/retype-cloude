use axum::{
    Router,
    body::Bytes,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use dashmap::DashMap;
use rand::Rng;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower_http::limit::RequestBodyLimitLayer;

const CODE_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
const CODE_LEN: usize = 6;
const MAX_BODY: usize = 2 * 1024 * 1024 * 1024;
const TTL_SECS: u64 = 60 * 8;

struct Entry {
    data: Vec<u8>,
    filename: String,
    created: Instant,
}

type Store = Arc<DashMap<String, Entry>>;

fn gen_code(store: &Store) -> String {
    let mut rng = rand::thread_rng();
    loop {
        let c: String = (0..CODE_LEN)
            .map(|_| CODE_CHARS[rng.gen_range(0..CODE_CHARS.len())] as char)
            .collect();
        if !store.contains_key(&c) { return c; }
    }
}

async fn push(
    State(store): State<Store>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    body: Bytes,
) -> impl IntoResponse {
    if body.is_empty() {
        return (StatusCode::BAD_REQUEST, "Empty body".to_string());
    }
    let filename = params.get("filename").cloned().unwrap_or_else(|| "project.retype".into());
    let code = gen_code(&store);
    store.insert(code.clone(), Entry { data: body.to_vec(), filename, created: Instant::now() });
    (StatusCode::OK, code)
}

async fn pull(
    State(store): State<Store>,
    Path(code): Path<String>,
) -> Response {
    let code = code.to_lowercase();
    match store.get(&code) {
        Some(entry) => {
            if entry.created.elapsed() > Duration::from_secs(TTL_SECS) {
                drop(entry);
                store.remove(&code);
                return (StatusCode::NOT_FOUND, "Code expired").into_response();
            }
            let filename = entry.filename.clone();
            let data = entry.data.clone();
            (
                StatusCode::OK,
                [(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", filename))],
                data,
            ).into_response()
        }
        None => (StatusCode::NOT_FOUND, "Code not found").into_response(),
    }
}

async fn index() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        include_str!("index.html"),
    )
}

async fn health() -> &'static str { "ok" }

#[tokio::main]
async fn main() {
    let store: Store = Arc::new(DashMap::new());

    let store_clone = store.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            store_clone.retain(|_, v| v.created.elapsed() < Duration::from_secs(TTL_SECS));
        }
    });

    let app = Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/push", post(push))
        .route("/pull/:code", get(pull))
        .layer(RequestBodyLimitLayer::new(MAX_BODY))
        .with_state(store);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
    let addr = format!("0.0.0.0:{}", port);
    println!("retype-cloud listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
