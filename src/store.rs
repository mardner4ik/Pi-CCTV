use std::{net::Ipv4Addr, os::unix::fs::PermissionsExt, path::PathBuf, sync::Arc};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

pub const MAX_CAMERAS: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum A9Protocol {
    #[default]
    Auto,
    Pppp,
    Legacy,
}

fn default_resolution() -> u8 {
    2
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Source {
    A9 {
        host: String,
        #[serde(default)]
        protocol: A9Protocol,
        #[serde(default = "default_resolution")]
        resolution: u8,
    },
    Rtsp {
        url: String,
    },
    Http {
        url: String,
    },
}

impl Source {
    pub fn label(&self) -> &'static str {
        match self {
            Source::A9 { .. } => "a9",
            Source::Rtsp { .. } => "rtsp",
            Source::Http { .. } => "http",
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Source::A9 { host, resolution, .. } => {
                host.trim().parse::<Ipv4Addr>().map_err(|_| "bad_host")?;
                if !(1..=4).contains(resolution) {
                    return Err("bad_resolution");
                }
                Ok(())
            }
            Source::Rtsp { url } => check_url(url, &["rtsp://", "rtsps://"]),
            Source::Http { url } => check_url(url, &["http://", "https://"]),
        }
    }

    pub fn normalized(self) -> Self {
        match self {
            Source::A9 {
                host,
                protocol,
                resolution,
            } => Source::A9 {
                host: host.trim().to_string(),
                protocol,
                resolution,
            },
            Source::Rtsp { url } => Source::Rtsp {
                url: url.trim().to_string(),
            },
            Source::Http { url } => Source::Http {
                url: url.trim().to_string(),
            },
        }
    }
}

fn check_url(url: &str, schemes: &[&str]) -> Result<(), &'static str> {
    let u = url.trim();
    if u.len() > 512 || u.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("bad_url");
    }
    let lower = u.to_lowercase();
    match schemes.iter().find(|s| lower.starts_with(*s)) {
        Some(s) if u.len() > s.len() => Ok(()),
        _ => Err("bad_url"),
    }
}

pub fn valid_name(name: &str) -> Result<String, &'static str> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 60 || n.chars().any(|c| c.is_control()) {
        return Err("bad_name");
    }
    Ok(n.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub source: Source,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Data {
    #[serde(default)]
    cameras: Vec<Record>,
    #[serde(default)]
    hidden: Vec<String>,
}

pub struct Store {
    path: PathBuf,
    data: RwLock<Data>,
}

impl Store {
    pub async fn load(dir: PathBuf) -> Arc<Self> {
        let path = dir.join("cameras.json");
        let data = match tokio::fs::read(&path).await {
            Ok(bytes) => serde_json::from_slice::<Data>(&bytes).unwrap_or_else(|e| {
                tracing::error!(
                    "{}: некоректний JSON ({e}), починаю з порожнього списку",
                    path.display()
                );
                Data::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Data::default(),
            Err(e) => {
                tracing::error!("{}: не вдалося прочитати: {e}", path.display());
                Data::default()
            }
        };
        Arc::new(Self {
            path,
            data: RwLock::new(data),
        })
    }

    pub async fn cameras(&self) -> Vec<Record> {
        self.data.read().await.cameras.clone()
    }

    pub async fn hidden(&self) -> Vec<String> {
        self.data.read().await.hidden.clone()
    }

    pub async fn add(&self, name: String, source: Source) -> Result<Record, &'static str> {
        let name = valid_name(&name)?;
        let source = source.normalized();
        source.validate()?;
        let mut d = self.data.write().await;
        if d.cameras.len() >= MAX_CAMERAS {
            return Err("too_many");
        }
        let mut n = 1;
        let id = loop {
            let id = format!("ip{n}");
            if !d.cameras.iter().any(|c| c.id == id) {
                break id;
            }
            n += 1;
        };
        let rec = Record {
            id,
            name,
            enabled: true,
            source,
        };
        d.cameras.push(rec.clone());
        self.persist(&d).await;
        Ok(rec)
    }

    pub async fn update(
        &self,
        id: &str,
        name: Option<String>,
        enabled: Option<bool>,
        source: Option<Source>,
    ) -> Result<Record, &'static str> {
        let name = name.map(|n| valid_name(&n)).transpose()?;
        let source = source.map(Source::normalized);
        if let Some(s) = &source {
            s.validate()?;
        }
        let mut d = self.data.write().await;
        let rec = d.cameras.iter_mut().find(|c| c.id == id).ok_or("not_found")?;
        if let Some(n) = name {
            rec.name = n;
        }
        if let Some(e) = enabled {
            rec.enabled = e;
        }
        if let Some(s) = source {
            rec.source = s;
        }
        let out = rec.clone();
        self.persist(&d).await;
        Ok(out)
    }

    pub async fn remove(&self, id: &str) -> bool {
        let mut d = self.data.write().await;
        let before = d.cameras.len();
        d.cameras.retain(|c| c.id != id);
        let changed = d.cameras.len() != before;
        if changed {
            self.persist(&d).await;
        }
        changed
    }

    pub async fn set_hidden(&self, node: &str, hidden: bool) {
        let mut d = self.data.write().await;
        let present = d.hidden.iter().any(|h| h == node);
        if hidden && !present {
            d.hidden.push(node.to_string());
        } else if !hidden && present {
            d.hidden.retain(|h| h != node);
        } else {
            return;
        }
        self.persist(&d).await;
    }

    async fn persist(&self, data: &Data) {
        let Ok(json) = serde_json::to_vec_pretty(data) else {
            return;
        };
        if let Some(dir) = self.path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(dir).await {
                tracing::error!("не вдалося створити {}: {e}", dir.display());
                return;
            }
        }
        let tmp = self.path.with_extension("json.tmp");
        let result = async {
            tokio::fs::write(&tmp, &json).await?;
            tokio::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).await?;
            tokio::fs::rename(&tmp, &self.path).await
        }
        .await;
        if let Err(e) = result {
            tracing::error!("не вдалося зберегти {}: {e}", self.path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a9(host: &str) -> Source {
        Source::A9 {
            host: host.into(),
            protocol: A9Protocol::Auto,
            resolution: 2,
        }
    }

    #[test]
    fn validates_sources() {
        assert!(a9("192.168.4.153").validate().is_ok());
        assert_eq!(a9("cam.local").validate(), Err("bad_host"));
        assert_eq!(
            Source::A9 {
                host: "10.0.0.1".into(),
                protocol: A9Protocol::Auto,
                resolution: 9
            }
            .validate(),
            Err("bad_resolution")
        );
        assert!(Source::Rtsp {
            url: "rtsp://u:p@10.0.0.5:554/s".into()
        }
        .validate()
        .is_ok());
        assert_eq!(
            Source::Rtsp {
                url: "file:///etc/passwd".into()
            }
            .validate(),
            Err("bad_url")
        );
        assert_eq!(Source::Http { url: "http://".into() }.validate(), Err("bad_url"));
        assert_eq!(
            Source::Http {
                url: "http://a b".into()
            }
            .validate(),
            Err("bad_url")
        );
    }

    #[test]
    fn names() {
        assert_eq!(valid_name("  Двір "), Ok("Двір".to_string()));
        assert_eq!(valid_name("   "), Err("bad_name"));
        assert_eq!(valid_name(&"x".repeat(61)), Err("bad_name"));
    }

    #[test]
    fn json_roundtrip() {
        let r = Record {
            id: "ip1".into(),
            name: "A9".into(),
            enabled: true,
            source: a9("10.0.0.2"),
        };
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"type\":\"a9\""));
        assert_eq!(serde_json::from_str::<Record>(&s).unwrap(), r);
        let minimal: Record =
            serde_json::from_str(r#"{"id":"ip2","name":"x","source":{"type":"a9","host":"1.2.3.4"}}"#).unwrap();
        assert!(minimal.enabled);
        assert_eq!(
            minimal.source,
            Source::A9 {
                host: "1.2.3.4".into(),
                protocol: A9Protocol::Auto,
                resolution: 2
            }
        );
    }
}
