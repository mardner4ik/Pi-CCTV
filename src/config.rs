use std::{collections::HashMap, net::IpAddr, path::PathBuf, str::FromStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeSetting {
    Auto,
    Copy,
    Transcode,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: IpAddr,
    pub port: u16,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub quality: u8,
    pub mode: ModeSetting,
    pub auth: Option<(String, String)>,
    pub idle_secs: u64,
    pub max_viewers: usize,
    pub demo: bool,
    pub names: HashMap<String, String>,
    pub data_dir: PathBuf,
    pub net_width: u32,
    pub session_hours: u64,
}

fn env_parse<T: FromStr>(key: &str, default: T) -> T {
    match std::env::var(key) {
        Ok(v) if !v.trim().is_empty() => v.trim().parse().unwrap_or_else(|_| {
            tracing::warn!("{key}={v:?} некоректне, використовую значення за замовчуванням");
            default
        }),
        _ => default,
    }
}

fn env_flag(key: &str) -> bool {
    matches!(
        std::env::var(key).as_deref().map(str::trim),
        Ok("1") | Ok("true") | Ok("yes") | Ok("on")
    )
}

fn parse_resolution(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s
        .trim()
        .to_lowercase()
        .split_once('x')
        .map(|(w, h)| (w.to_string(), h.to_string()))?;
    let (w, h) = (w.parse::<u32>().ok()?, h.parse::<u32>().ok()?);
    (w >= 160 && h >= 120 && w <= 7680 && h <= 4320).then_some((w, h))
}

fn parse_names(s: &str) -> HashMap<String, String> {
    s.split(',')
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .filter(|(k, v)| !k.is_empty() && !v.is_empty())
        .collect()
}

impl Config {
    pub fn from_env() -> Self {
        let (width, height) = match std::env::var("PICCTV_RESOLUTION") {
            Ok(v) if !v.trim().is_empty() => parse_resolution(&v).unwrap_or_else(|| {
                tracing::warn!("PICCTV_RESOLUTION={v:?} некоректне (очікується 1280x720)");
                (1280, 720)
            }),
            _ => (1280, 720),
        };

        let mode = match std::env::var("PICCTV_MODE")
            .unwrap_or_default()
            .trim()
            .to_lowercase()
            .as_str()
        {
            "copy" => ModeSetting::Copy,
            "transcode" => ModeSetting::Transcode,
            "" | "auto" => ModeSetting::Auto,
            other => {
                tracing::warn!("PICCTV_MODE={other:?} невідомий (auto|copy|transcode)");
                ModeSetting::Auto
            }
        };

        let auth = std::env::var("PICCTV_PASS")
            .ok()
            .filter(|p| !p.is_empty())
            .map(|p| (std::env::var("PICCTV_USER").unwrap_or_else(|_| "admin".into()), p));

        Self {
            bind: env_parse("PICCTV_BIND", IpAddr::from([0, 0, 0, 0])),
            port: env_parse("PICCTV_PORT", 8080),
            width,
            height,
            fps: env_parse::<u32>("PICCTV_FPS", 15).clamp(1, 60),
            quality: env_parse::<u8>("PICCTV_QUALITY", 5).clamp(2, 31),
            mode,
            auth,
            idle_secs: env_parse("PICCTV_IDLE_SECS", 10),
            max_viewers: env_parse::<usize>("PICCTV_MAX_VIEWERS", 8).max(1),
            demo: env_flag("PICCTV_DEMO"),
            names: parse_names(&std::env::var("PICCTV_NAMES").unwrap_or_default()),
            data_dir: PathBuf::from(
                std::env::var("PICCTV_DATA")
                    .ok()
                    .filter(|v| !v.trim().is_empty())
                    .unwrap_or_else(|| "picctv-data".into()),
            ),
            net_width: env_parse::<u32>("PICCTV_NET_WIDTH", 0).min(7680),
            session_hours: env_parse::<u64>("PICCTV_SESSION_HOURS", 168).clamp(1, 24 * 90),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution() {
        assert_eq!(parse_resolution("1280x720"), Some((1280, 720)));
        assert_eq!(parse_resolution(" 640X480 "), Some((640, 480)));
        assert_eq!(parse_resolution("abc"), None);
        assert_eq!(parse_resolution("10x10"), None);
    }

    #[test]
    fn names() {
        let m = parse_names("video0=Двір, video2 = Вхід,bad,=x");
        assert_eq!(m.get("video0").map(String::as_str), Some("Двір"));
        assert_eq!(m.get("video2").map(String::as_str), Some("Вхід"));
        assert_eq!(m.len(), 2);
    }
}
