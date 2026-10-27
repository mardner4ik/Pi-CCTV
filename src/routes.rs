use std::{
    convert::Infallible,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderName, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

use crate::{
    a9,
    camera::{Camera, DeviceRow},
    state::AppState,
    store::{Record, Source},
    stream::Status,
};

const INDEX_HTML: &str = include_str!("../static/index.html");
const APP_CSS: &str = include_str!("../static/app.css");
const APP_JS: &str = include_str!("../static/app.js");
const LOGO_SVG: &str = include_str!("../static/logo.svg");
const FAVICON_SVG: &str = include_str!("../static/favicon.svg");

const CSP: &str = "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; \
                   script-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

pub async fn index() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
            (header::CONTENT_SECURITY_POLICY, CSP),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        INDEX_HTML,
    )
}

fn asset(content_type: &'static str, body: &'static str) -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "no-cache"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        body,
    )
}

pub async fn app_css() -> impl IntoResponse {
    asset("text/css; charset=utf-8", APP_CSS)
}

pub async fn app_js() -> impl IntoResponse {
    asset("text/javascript; charset=utf-8", APP_JS)
}

pub async fn logo_svg() -> impl IntoResponse {
    asset("image/svg+xml", LOGO_SVG)
}

pub async fn favicon_svg() -> impl IntoResponse {
    asset("image/svg+xml", FAVICON_SVG)
}

pub async fn healthz() -> &'static str {
    "ok"
}

#[derive(Serialize)]
struct CameraInfo {
    id: String,
    name: String,
    device: String,
    source: &'static str,
    #[serde(flatten)]
    status: Status,
}

pub async fn cameras_list(State(state): State<AppState>) -> impl IntoResponse {
    let cams = state.cameras.read().await;
    let list: Vec<CameraInfo> = cams
        .iter()
        .map(|c| CameraInfo {
            id: c.id.clone(),
            name: c.name.clone(),
            device: if c.label() == "usb" {
                c.device_path.clone()
            } else {
                String::new()
            },
            source: c.label(),
            status: state.hub.status(c),
        })
        .collect();
    ([(header::CACHE_CONTROL, "no-store")], Json(list))
}

#[derive(Deserialize)]
pub struct StreamQuery {
    fps: Option<u32>,
}

pub async fn stream_mjpeg(
    Path(id): Path<String>,
    Query(q): Query<StreamQuery>,
    State(state): State<AppState>,
) -> Response {
    let Some(cam) = state.find_camera(&id).await else {
        return (StatusCode::NOT_FOUND, "Camera not found").into_response();
    };
    let pipeline = state.hub.pipeline(&cam);
    if pipeline.viewers() >= state.cfg.max_viewers {
        return (StatusCode::SERVICE_UNAVAILABLE, "Too many viewers").into_response();
    }

    let min_gap = q
        .fps
        .filter(|f| *f > 0)
        .map(|f| Duration::from_secs_f64(1.0 / f.min(60) as f64));
    let mut last_sent: Option<Instant> = None;
    let (guard, rx) = pipeline.subscribe();
    let body = BroadcastStream::new(rx).filter_map(move |item| {
        let _keep = &guard;
        let frame = item.ok()?;
        if let Some(gap) = min_gap {
            let now = Instant::now();
            if last_sent.is_some_and(|l| now.duration_since(l) < gap) {
                return None;
            }
            last_sent = Some(now);
        }
        Some(Ok::<Bytes, Infallible>(frame.part))
    });

    (
        [
            (header::CONTENT_TYPE, "multipart/x-mixed-replace; boundary=frame"),
            (header::CACHE_CONTROL, "no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        [(HeaderName::from_static("x-accel-buffering"), "no")],
        Body::from_stream(body),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct SnapshotQuery {
    download: Option<String>,
}

pub async fn snapshot(
    Path(id): Path<String>,
    Query(q): Query<SnapshotQuery>,
    State(state): State<AppState>,
) -> Response {
    let Some(cam) = state.find_camera(&id).await else {
        return (StatusCode::NOT_FOUND, "Camera not found").into_response();
    };
    let pipeline = state.hub.pipeline(&cam);
    match pipeline.snapshot(Duration::from_secs(2), Duration::from_secs(8)).await {
        Some(jpeg) => {
            let mut resp = (
                [
                    (header::CONTENT_TYPE, "image/jpeg"),
                    (header::CACHE_CONTROL, "no-store"),
                ],
                jpeg,
            )
                .into_response();
            if q.download.is_some() {
                let ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let value = format!("attachment; filename=\"{}-{}.jpg\"", cam.id, ts);
                if let Ok(v) = value.parse() {
                    resp.headers_mut().insert(header::CONTENT_DISPOSITION, v);
                }
            }
            resp
        }
        None => (StatusCode::SERVICE_UNAVAILABLE, "No frame available").into_response(),
    }
}

pub struct ApiError(StatusCode, &'static str);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

fn require_admin(state: &AppState) -> Result<(), ApiError> {
    if state.cfg.auth.is_none() {
        return Err(ApiError(StatusCode::FORBIDDEN, "no_password"));
    }
    Ok(())
}

fn bad(code: &'static str) -> ApiError {
    let status = if code == "not_found" {
        StatusCode::NOT_FOUND
    } else {
        StatusCode::BAD_REQUEST
    };
    ApiError(status, code)
}

#[derive(Serialize)]
struct AdminCamera {
    #[serde(flatten)]
    record: Record,
    #[serde(flatten)]
    status: Status,
}

pub async fn admin_cameras(State(state): State<AppState>) -> Result<Response, ApiError> {
    require_admin(&state)?;
    let list: Vec<AdminCamera> = state
        .store
        .cameras()
        .await
        .into_iter()
        .map(|r| {
            let status = state.hub.status(&Camera::net(&r.id, &r.name, r.source.clone()));
            AdminCamera { record: r, status }
        })
        .collect();
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(list)).into_response())
}

#[derive(Deserialize)]
pub struct NewCamera {
    name: String,
    source: Source,
}

pub async fn admin_add(State(state): State<AppState>, Json(body): Json<NewCamera>) -> Result<Response, ApiError> {
    require_admin(&state)?;
    let rec = state.store.add(body.name, body.source).await.map_err(bad)?;
    state.rebuild().await;
    tracing::info!("Додано камеру {} ({})", rec.id, rec.source.label());
    Ok((StatusCode::CREATED, Json(rec)).into_response())
}

#[derive(Deserialize)]
pub struct UpdateCamera {
    name: Option<String>,
    enabled: Option<bool>,
    source: Option<Source>,
}

pub async fn admin_update(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<UpdateCamera>,
) -> Result<Response, ApiError> {
    require_admin(&state)?;
    let reconnect = body.source.is_some() || body.enabled == Some(false);
    let rec = state
        .store
        .update(&id, body.name, body.enabled, body.source)
        .await
        .map_err(bad)?;
    if reconnect {
        state.hub.restart(&format!("net:{id}"));
    }
    state.rebuild().await;
    Ok(Json(rec).into_response())
}

pub async fn admin_delete(Path(id): Path<String>, State(state): State<AppState>) -> Result<Response, ApiError> {
    require_admin(&state)?;
    if !state.store.remove(&id).await {
        return Err(bad("not_found"));
    }
    state.hub.restart(&format!("net:{id}"));
    state.rebuild().await;
    tracing::info!("Видалено камеру {id}");
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub async fn admin_restart(Path(id): Path<String>, State(state): State<AppState>) -> Result<Response, ApiError> {
    require_admin(&state)?;
    let known = state.cameras.read().await.iter().find(|c| c.id == id).cloned();
    let Some(cam) = known else {
        return Err(bad("not_found"));
    };
    state.hub.restart(&cam.device_path);
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub async fn admin_devices(State(state): State<AppState>) -> Result<Response, ApiError> {
    require_admin(&state)?;
    let rows: Vec<DeviceRow> = state.devices.read().await.clone();
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(rows)).into_response())
}

#[derive(Deserialize)]
pub struct HideBody {
    hidden: bool,
}

pub async fn admin_device_hide(
    Path(node): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<HideBody>,
) -> Result<Response, ApiError> {
    require_admin(&state)?;
    let valid = node
        .strip_prefix("video")
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    if !valid {
        return Err(bad("bad_device"));
    }
    state.store.set_hidden(&node, body.hidden).await;
    state.rebuild().await;
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub async fn admin_discover(State(state): State<AppState>) -> Result<Response, ApiError> {
    require_admin(&state)?;
    let _busy = state
        .test_lock
        .try_lock()
        .map_err(|_| ApiError(StatusCode::TOO_MANY_REQUESTS, "busy"))?;
    let found = a9::discover(Duration::from_millis(2600)).await;
    let known: Vec<String> = state
        .store
        .cameras()
        .await
        .into_iter()
        .filter_map(|r| match r.source {
            Source::A9 { host, .. } => Some(host),
            _ => None,
        })
        .collect();
    let list: Vec<_> = found
        .into_iter()
        .map(|f| json!({ "ip": f.ip.to_string(), "id": f.id, "added": known.contains(&f.ip.to_string()) }))
        .collect();
    Ok(Json(list).into_response())
}

#[derive(Deserialize)]
pub struct TestBody {
    source: Source,
}

pub async fn admin_test(State(state): State<AppState>, Json(body): Json<TestBody>) -> Result<Response, ApiError> {
    require_admin(&state)?;
    let source = body.source.normalized();
    source.validate().map_err(bad)?;
    let _busy = state
        .test_lock
        .try_lock()
        .map_err(|_| ApiError(StatusCode::TOO_MANY_REQUESTS, "busy"))?;
    let cam = Camera::net("test", "test", source);
    let pipeline = state.hub.standalone(&cam);
    let frame = pipeline.snapshot(Duration::ZERO, Duration::from_secs(15)).await;
    let error = pipeline.last_error();
    pipeline.retire();
    let out = match frame {
        Some(jpeg) => json!({ "ok": true, "image": format!("data:image/jpeg;base64,{}", STANDARD.encode(&jpeg)) }),
        None => json!({ "ok": false, "error": error.unwrap_or_else(|| "no frame received".into()) }),
    };
    Ok(Json(out).into_response())
}

pub async fn admin_info(State(state): State<AppState>) -> Result<Response, ApiError> {
    require_admin(&state)?;
    let c = &state.cfg;
    Ok(Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "ffmpeg": *state.ffmpeg,
        "mode": format!("{:?}", c.mode).to_lowercase(),
        "resolution": format!("{}x{}", c.width, c.height),
        "fps": c.fps,
        "quality": c.quality,
        "idle_secs": c.idle_secs,
        "max_viewers": c.max_viewers,
        "net_width": c.net_width,
        "session_hours": c.session_hours,
        "uptime_s": state.started.elapsed().as_secs(),
        "cameras": state.cameras.read().await.len(),
    }))
    .into_response())
}
