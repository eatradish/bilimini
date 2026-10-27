//! bili-core: local HTTP service that the QML front-end talks to.
//!
//! The host app spawns this binary, reads one JSON handshake line from stdout
//! (`{"port":..,"token":..,"pid":..}`) and then talks HTTP on loopback. Every
//! request must carry `Authorization: Bearer <token>`.

mod bili;

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};

use bili::Bili;

#[derive(Clone)]
struct AppState {
    token: Arc<String>,
    bili: Arc<Bili>,
}

#[tokio::main]
async fn main() {
    // Bilibili is reachable directly from here; make sure a development proxy
    // exported in the desktop session does not capture these requests.
    let no_proxy = std::env::var("NO_PROXY").unwrap_or_default();
    if !no_proxy.contains("bilibili.com") {
        let value = if no_proxy.is_empty() {
            "bilibili.com,hdslb.com,127.0.0.1,localhost".to_string()
        } else {
            format!("{no_proxy},bilibili.com,hdslb.com,127.0.0.1,localhost")
        };
        // SAFETY: nothing else runs concurrently at this point (single-threaded,
        // before the Tokio runtime spawns any worker threads), so mutating the
        // process environment cannot race with another thread reading it.
        unsafe {
            std::env::set_var("NO_PROXY", value);
            std::env::set_var("no_proxy", std::env::var("NO_PROXY").unwrap_or_default());
        }
    }

    let token = random_token();
    let state = AppState {
        token: Arc::new(token.clone()),
        bili: Arc::new(Bili::new()),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/feed", get(feed))
        .route("/search", get(search))
        .route("/video/{bvid}", get(video))
        .route("/play/{bvid}", get(play))
        .route("/danmaku/{cid}", get(danmaku))
        .route("/comments/{bvid}", get(comments))
        .route("/user/{mid}/videos", get(user_videos))
        .route("/auth/status", get(auth_status))
        .route("/auth/import", post(auth_import))
        .layer(middleware::from_fn_with_state(state.clone(), require_token))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bili-core: failed to bind loopback port");
    let port = listener
        .local_addr()
        .expect("bili-core: missing local address")
        .port();

    // The single line the host app parses. Keep it on stdout, logs on stderr.
    let handshake = json!({
        "port": port,
        "token": token,
        "pid": std::process::id(),
    });
    println!("{handshake}");
    let _ = std::io::stdout().flush();
    eprintln!("bili-core: listening on 127.0.0.1:{port}");

    axum::serve(listener, app)
        .await
        .expect("bili-core: server error");
}

async fn require_token(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let expected = format!("Bearer {}", state.token);
    let provided = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    if provided != expected {
        return (StatusCode::UNAUTHORIZED, "unauthorized\n").into_response();
    }

    next.run(request).await
}

async fn health() -> Json<Value> {
    Json(json!({
        "ok": true,
        "service": "bili-core",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn feed(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> (StatusCode, Json<Value>) {
    let fresh_idx = params
        .get("fresh_idx")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1);
    match state.bili.feed(fresh_idx).await {
        Ok(items) => (
            StatusCode::OK,
            Json(json!({ "items": items, "error": null })),
        ),
        Err(err) => {
            eprintln!("bili-core: feed failed: {err:#}");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "items": [], "error": format!("{err:#}") })),
            )
        }
    }
}

async fn search(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> (StatusCode, Json<Value>) {
    let keyword = params
        .get("q")
        .map(|value| value.trim().to_string())
        .unwrap_or_default();

    if keyword.is_empty() {
        return (StatusCode::OK, Json(json!({ "items": [], "error": null })));
    }

    let kind = params
        .get("type")
        .map(|value| value.as_str())
        .unwrap_or("video");

    match state.bili.search(&keyword, kind).await {
        Ok(items) => (
            StatusCode::OK,
            Json(json!({ "items": items, "error": null })),
        ),
        Err(err) => {
            eprintln!("bili-core: search failed: {err:#}");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "items": [], "error": format!("{err:#}") })),
            )
        }
    }
}

async fn video(
    State(state): State<AppState>,
    Path(bvid): Path<String>,
) -> (StatusCode, Json<Value>) {
    match state.bili.view(&bvid).await {
        Ok(data) => (
            StatusCode::OK,
            Json(json!({ "video": data, "error": null })),
        ),
        Err(err) => {
            eprintln!("bili-core: view failed: {err:#}");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "video": null, "error": format!("{err:#}") })),
            )
        }
    }
}

async fn auth_status(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    match state.bili.auth_status().await {
        Ok(data) => (StatusCode::OK, Json(data)),
        Err(err) => (
            StatusCode::OK,
            Json(json!({ "logged_in": false, "error": format!("{err:#}") })),
        ),
    }
}

async fn auth_import(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    match state.bili.import_cookies().await {
        Ok(data) => (StatusCode::OK, Json(data)),
        Err(err) => {
            eprintln!("bili-core: cookie import failed: {err:#}");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "ok": false, "error": format!("{err:#}") })),
            )
        }
    }
}

async fn play(
    State(state): State<AppState>,
    Path(bvid): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> (StatusCode, Json<Value>) {
    let cid = params
        .get("cid")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let qn = params
        .get("qn")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(80);

    match state.bili.play(&bvid, cid, qn).await {
        Ok(data) => (StatusCode::OK, Json(data)),
        Err(err) => {
            eprintln!("bili-core: play url failed: {err:#}");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("{err:#}") })),
            )
        }
    }
}

async fn danmaku(State(state): State<AppState>, Path(cid): Path<u64>) -> (StatusCode, Json<Value>) {
    match state.bili.danmaku(cid).await {
        Ok(items) => (StatusCode::OK, Json(json!({ "items": items }))),
        Err(err) => {
            eprintln!("bili-core: danmaku failed: {err:#}");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "items": [], "error": format!("{err:#}") })),
            )
        }
    }
}

async fn comments(
    State(state): State<AppState>,
    Path(bvid): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> (StatusCode, Json<Value>) {
    let offset = params
        .get("offset")
        .map(|value| value.as_str())
        .unwrap_or("");
    let sort = params
        .get("sort")
        .map(|value| value.as_str())
        .unwrap_or("hot");

    match state.bili.comments(&bvid, offset, sort).await {
        Ok(data) => (StatusCode::OK, Json(data)),
        Err(err) => {
            eprintln!("bili-core: comments failed: {err:#}");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "count": 0, "items": [], "error": format!("{err:#}") })),
            )
        }
    }
}

async fn user_videos(
    State(state): State<AppState>,
    Path(mid): Path<u64>,
    Query(params): Query<HashMap<String, String>>,
) -> (StatusCode, Json<Value>) {
    let page = params
        .get("page")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1);

    match state.bili.user_videos(mid, page).await {
        Ok(data) => (StatusCode::OK, Json(data)),
        Err(err) => {
            eprintln!("bili-core: user videos failed: {err:#}");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "items": [], "count": 0, "error": format!("{err:#}") })),
            )
        }
    }
}

fn random_token() -> String {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).expect("bili-core: no randomness available");
    buf.iter().map(|byte| format!("{byte:02x}")).collect()
}
