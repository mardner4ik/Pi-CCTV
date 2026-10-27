use std::{
    collections::{HashMap, HashSet},
    fs,
};

use serde::Serialize;

use crate::{
    store::Source,
    v4l2::{self, DeviceInfo, QueryError},
};

pub const DEMO_ID: &str = "demo0";
pub const DEMO_DEVICE: &str = "demo";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Usb { formats: Vec<String> },
    Demo,
    Net(Source),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Camera {
    pub id: String,
    pub device_path: String,
    pub name: String,
    pub kind: Kind,
}

impl Camera {
    pub fn label(&self) -> &'static str {
        match &self.kind {
            Kind::Usb { .. } => "usb",
            Kind::Demo => "demo",
            Kind::Net(s) => s.label(),
        }
    }

    pub fn net(id: &str, name: &str, source: Source) -> Self {
        Self {
            id: id.to_string(),
            device_path: format!("net:{id}"),
            name: name.to_string(),
            kind: Kind::Net(source),
        }
    }

    pub fn demo(names: &HashMap<String, String>) -> Self {
        Self {
            id: DEMO_ID.into(),
            device_path: DEMO_DEVICE.into(),
            name: names
                .get(DEMO_ID)
                .cloned()
                .unwrap_or_else(|| "Demo (test pattern)".into()),
            kind: Kind::Demo,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeviceRow {
    pub node: String,
    pub driver: String,
    pub card: String,
    pub bus: String,
    pub camera: bool,
    pub reason: Option<&'static str>,
    pub formats: Vec<String>,
    pub hidden: bool,
}

enum Probe {
    Info(DeviceInfo),
    NotV4l2,
}

#[derive(Default)]
pub struct Discovery {
    cache: HashMap<String, Probe>,
}

impl Discovery {
    pub fn scan(&mut self, names: &HashMap<String, String>, hidden: &HashSet<String>) -> (Vec<Camera>, Vec<DeviceRow>) {
        let nodes = list_video_nodes();
        let present: HashSet<&String> = nodes.iter().collect();
        self.cache.retain(|k, _| present.contains(k));

        let mut cams: Vec<Camera> = Vec::new();
        let mut rows: Vec<DeviceRow> = Vec::new();

        for node in &nodes {
            if !self.cache.contains_key(node) {
                match v4l2::query(&format!("/dev/{node}")) {
                    Ok(info) => {
                        self.cache.insert(node.clone(), Probe::Info(info));
                    }
                    Err(QueryError::NotV4l2) => {
                        self.cache.insert(node.clone(), Probe::NotV4l2);
                    }
                    Err(QueryError::Transient) => {}
                }
            }
            let Some(Probe::Info(info)) = self.cache.get(node) else {
                continue;
            };
            let verdict = info.classify();
            let is_hidden = hidden.contains(node);
            rows.push(DeviceRow {
                node: node.clone(),
                driver: info.driver.clone(),
                card: info.card.clone(),
                bus: info.bus.clone(),
                camera: verdict.is_ok(),
                reason: verdict.err().map(|r| r.code()),
                formats: info.formats.clone(),
                hidden: is_hidden,
            });
            if verdict.is_ok() && !is_hidden {
                let name = names.get(node).cloned().unwrap_or_else(|| clean_name(&info.card));
                cams.push(Camera {
                    id: node.clone(),
                    device_path: format!("/dev/{node}"),
                    name,
                    kind: Kind::Usb {
                        formats: info.formats.clone(),
                    },
                });
            }
        }

        disambiguate(&mut cams);
        (cams, rows)
    }
}

pub fn disambiguate(cams: &mut [Camera]) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let totals: HashMap<String, usize> = cams.iter().fold(HashMap::new(), |mut m, c| {
        *m.entry(c.name.clone()).or_default() += 1;
        m
    });
    for c in cams.iter_mut() {
        if totals[&c.name] > 1 {
            let n = seen.entry(c.name.clone()).or_default();
            *n += 1;
            if *n > 1 {
                c.name = format!("{} ({})", c.name, n);
            }
        }
    }
}

fn list_video_nodes() -> Vec<String> {
    let mut nodes: Vec<(u32, String)> = match fs::read_dir("/dev") {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let idx = name.strip_prefix("video")?.parse::<u32>().ok()?;
                Some((idx, name))
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    nodes.sort();
    nodes.into_iter().map(|(_, n)| n).collect()
}

fn clean_name(raw: &str) -> String {
    let head = raw.split(':').next().unwrap_or(raw).trim();
    if head.is_empty() {
        "Camera".into()
    } else {
        head.chars().take(60).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_names() {
        let mk = |id: &str| Camera {
            id: id.into(),
            device_path: format!("/dev/{id}"),
            name: "Cam".into(),
            kind: Kind::Usb { formats: vec![] },
        };
        let mut v = vec![mk("video0"), mk("video2")];
        disambiguate(&mut v);
        assert_eq!(v[0].name, "Cam");
        assert_eq!(v[1].name, "Cam (2)");
    }

    #[test]
    fn card_names() {
        assert_eq!(clean_name("HD Pro Webcam C920: HD Pro Webcam"), "HD Pro Webcam C920");
        assert_eq!(clean_name(""), "Camera");
    }
}
