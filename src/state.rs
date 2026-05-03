use std::sync::Arc;
use tokio::sync::RwLock;

use crate::camera::{Camera, discover_cameras};

#[derive(Clone)]
pub struct AppState {
    pub cameras: Arc<RwLock<Vec<Camera>>>,
}

impl AppState {
    pub async fn new() -> Self {
        let cameras = discover_cameras();
        tracing::info!("Знайдено камер: {}", cameras.len());
        for cam in &cameras {
            tracing::info!("  {} -> {}", cam.name, cam.device_path);
        }

        let state = Self {
            cameras: Arc::new(RwLock::new(cameras)),
        };

        let state_clone = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                let fresh = discover_cameras();
                let mut lock = state_clone.cameras.write().await;
                *lock = fresh;
            }
        });

        state
    }
}