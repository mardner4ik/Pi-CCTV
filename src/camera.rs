use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Camera {
    pub id: String,
    pub device_path: String,
    pub name: String,
    pub index: usize,
}

pub fn discover_cameras() -> Vec<Camera> {
    let mut cameras = Vec::new();
    let dev_path = PathBuf::from("/dev");

    let mut entries: Vec<_> = match fs::read_dir(&dev_path) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("video")
            })
            .collect(),
        Err(_) => return cameras,
    };

    entries.sort_by_key(|e| e.file_name());

    for (idx, entry) in entries.iter().enumerate() {
        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();

        if is_capture_device(&path_str) {
            let name = format!("Камера {}", idx + 1);
            cameras.push(Camera {
                id: format!("video{}", idx),
                device_path: path_str,
                name,
                index: idx,
            });
        }
    }

    cameras
}

fn is_capture_device(device: &str) -> bool {
    use std::process::Command;

    let output = Command::new("v4l2-ctl")
        .args(["--device", device, "--all"])
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout.contains("Video Capture")
        }
        Err(_) => {
            std::path::Path::new(device).exists()
        }
    }
}