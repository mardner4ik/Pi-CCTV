mod a9;
mod auth;
mod camera;
mod config;
mod jpeg;
mod routes;
mod state;
mod store;
mod stream;
mod v4l2;

use std::{net::SocketAddr, sync::Arc};

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::Config;
use state::AppState;

fn main() {
    let threads = std::env::var("PICCTV_THREADS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get().min(2))
                .unwrap_or(1)
        })
        .clamp(1, 16);
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(threads)
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(run());
}

async fn ffmpeg_version() -> Option<String> {
    let out = tokio::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .map(|l| l.trim().to_string())
}

async fn run() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cfg = Arc::new(Config::from_env());

    let ffmpeg = ffmpeg_version().await;
    if ffmpeg.is_none() {
        tracing::error!("ffmpeg не знайдено в PATH — USB/RTSP стрімінг не працюватиме (sudo apt install ffmpeg)");
    }
    if cfg.auth.is_none() {
        tracing::warn!(
            "PICCTV_PASS не задано: панель керування вимкнена, а перегляд відкритий для всіх, хто бачить порт {}.",
            cfg.port
        );
    }

    let state = AppState::new(cfg.clone(), ffmpeg).await;

    let app = Router::new()
        .route("/", get(routes::index))
        .route("/app.css", get(routes::app_css))
        .route("/app.js", get(routes::app_js))
        .route("/logo.svg", get(routes::logo_svg))
        .route("/favicon.svg", get(routes::favicon_svg))
        .route("/healthz", get(routes::healthz))
        .route("/api/login", post(auth::login))
        .route("/api/logout", post(auth::logout))
        .route("/api/session", get(auth::session))
        .route("/api/cameras", get(routes::cameras_list))
        .route("/stream/:id", get(routes::stream_mjpeg))
        .route("/snapshot/:id", get(routes::snapshot))
        .route("/api/admin/info", get(routes::admin_info))
        .route("/api/admin/cameras", get(routes::admin_cameras).post(routes::admin_add))
        .route(
            "/api/admin/cameras/:id",
            axum::routing::put(routes::admin_update).delete(routes::admin_delete),
        )
        .route("/api/admin/cameras/:id/restart", post(routes::admin_restart))
        .route("/api/admin/devices", get(routes::admin_devices))
        .route("/api/admin/devices/:node/hidden", post(routes::admin_device_hide))
        .route("/api/admin/discover", post(routes::admin_discover))
        .route("/api/admin/test", post(routes::admin_test))
        .layer(middleware::from_fn_with_state(state.clone(), auth::require_auth))
        .with_state(state);

    let addr = SocketAddr::new(cfg.bind, cfg.port);
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("Не вдалося зайняти {addr}: {e}");
            std::process::exit(1);
        }
    };
    tracing::info!(
        "PiCCTV {} слухає http://{addr} (режим: {:?}, {}x{} @ {} fps, пароль: {}, дані: {})",
        env!("CARGO_PKG_VERSION"),
        cfg.mode,
        cfg.width,
        cfg.height,
        cfg.fps,
        if cfg.auth.is_some() {
            "увімкнено"
        } else {
            "вимкнено"
        },
        cfg.data_dir.display()
    );

    tokio::select! {
        r = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()) => {
            if let Err(e) = r {
                tracing::error!("Помилка сервера: {e}");
            }
        }
        _ = shutdown_signal() => tracing::info!("Зупинка…"),
    }
}

async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
}
