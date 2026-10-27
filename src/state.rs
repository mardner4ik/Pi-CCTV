use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tokio::sync::RwLock;

use crate::{
    auth::Sessions,
    camera::{Camera, DeviceRow, Discovery},
    config::Config,
    store::Store,
    stream::Hub,
};

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub cameras: Arc<RwLock<Vec<Camera>>>,
    pub devices: Arc<RwLock<Vec<DeviceRow>>>,
    pub hub: Arc<Hub>,
    pub store: Arc<Store>,
    pub sessions: Arc<Sessions>,
    pub ffmpeg: Arc<Option<String>>,
    pub started: Instant,
    pub test_lock: Arc<tokio::sync::Mutex<()>>,
    discovery: Arc<Mutex<Discovery>>,
    rebuild_lock: Arc<tokio::sync::Mutex<()>>,
}

impl AppState {
    pub async fn new(cfg: Arc<Config>, ffmpeg: Option<String>) -> Self {
        let store = Store::load(cfg.data_dir.clone()).await;
        let state = Self {
            hub: Arc::new(Hub::new(cfg.clone())),
            sessions: Arc::new(Sessions::new(Duration::from_secs(cfg.session_hours * 3600))),
            cameras: Arc::new(RwLock::new(Vec::new())),
            devices: Arc::new(RwLock::new(Vec::new())),
            ffmpeg: Arc::new(ffmpeg),
            started: Instant::now(),
            test_lock: Arc::new(tokio::sync::Mutex::new(())),
            discovery: Arc::new(Mutex::new(Discovery::default())),
            rebuild_lock: Arc::new(tokio::sync::Mutex::new(())),
            store,
            cfg,
        };

        state.rebuild().await;
        let cams = state.cameras.read().await.clone();
        tracing::info!("Знайдено камер: {}", cams.len());
        for c in &cams {
            tracing::info!("  {} ({}) [{}] -> {}", c.id, c.name, c.label(), c.device_path);
        }

        let bg = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                bg.rebuild().await;
            }
        });

        state
    }

    pub async fn rebuild(&self) {
        let _guard = self.rebuild_lock.lock().await;
        let hidden: HashSet<String> = self.store.hidden().await.into_iter().collect();
        let discovery = self.discovery.clone();
        let cfg = self.cfg.clone();
        let (mut list, rows) = tokio::task::spawn_blocking(move || discovery.lock().unwrap().scan(&cfg.names, &hidden))
            .await
            .expect("discovery task panicked");

        if self.cfg.demo {
            list.push(Camera::demo(&self.cfg.names));
        }
        for rec in self.store.cameras().await {
            if rec.enabled {
                list.push(Camera::net(&rec.id, &rec.name, rec.source));
            }
        }

        *self.devices.write().await = rows;
        let changed = *self.cameras.read().await != list;
        if changed {
            tracing::info!("Список камер змінився: {}", list.len());
            self.hub.prune(&list);
            *self.cameras.write().await = list;
        }
    }

    pub async fn find_camera(&self, id: &str) -> Option<Camera> {
        self.cameras.read().await.iter().find(|c| c.id == id).cloned()
    }
}
