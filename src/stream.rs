use std::{
    collections::HashMap,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering::SeqCst},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use bytes::{Bytes, BytesMut};
use serde::Serialize;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
    sync::{broadcast, mpsc},
    task::JoinHandle,
    time::{self, MissedTickBehavior},
};

use crate::{
    a9,
    camera::{Camera, Kind},
    config::{Config, ModeSetting},
    jpeg,
    store::Source,
    v4l2,
};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(12);
const STALL_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
const PROTOCOLS: &str = "rtsp,rtsps,rtp,udp,tcp,tls,crypto,http,https";

#[derive(Clone)]
pub struct Frame {
    pub part: Bytes,
    pub jpeg: Bytes,
    pub at: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Idle,
    Starting,
    Live,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Profile {
    Copy,
    Transcode,
    TranscodeAuto,
    Demo,
    NetCopy,
    NetTranscode,
    Native,
}

impl Profile {
    fn name(self) -> &'static str {
        match self {
            Profile::Copy | Profile::NetCopy => "copy",
            Profile::Transcode | Profile::NetTranscode => "transcode",
            Profile::TranscodeAuto => "transcode-auto",
            Profile::Demo => "demo",
            Profile::Native => "native",
        }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct Status {
    pub state: State,
    pub fps: f32,
    pub viewers: usize,
    pub mode: Option<&'static str>,
    pub frame_age_ms: Option<u64>,
    pub uptime_s: Option<u64>,
    pub error: Option<String>,
}

impl Status {
    fn idle() -> Self {
        Self {
            state: State::Idle,
            fps: 0.0,
            viewers: 0,
            mode: None,
            frame_age_ms: None,
            uptime_s: None,
            error: None,
        }
    }
}

struct Info {
    state: State,
    profile: Option<&'static str>,
    error: Option<String>,
    live_since: Option<Instant>,
    latest: Option<Frame>,
}

pub struct Pipeline {
    label: String,
    kind: Kind,
    cfg: Arc<Config>,
    tx: broadcast::Sender<Frame>,
    viewers: AtomicUsize,
    running: AtomicBool,
    retired: AtomicBool,
    good: AtomicBool,
    profile_idx: AtomicUsize,
    fps_x10: AtomicU32,
    info: Mutex<Info>,
}

pub struct ViewerGuard(Arc<Pipeline>);

impl Drop for ViewerGuard {
    fn drop(&mut self) {
        self.0.viewers.fetch_sub(1, SeqCst);
    }
}

struct Outcome {
    frames: u64,
    stopped_idle: bool,
    error: Option<String>,
}

struct Feed {
    rx: mpsc::Receiver<Bytes>,
    task: JoinHandle<()>,
    reason: Arc<Mutex<String>>,
}

impl Feed {
    fn reason(&self) -> String {
        let r = self.reason.lock().unwrap().clone();
        if r.is_empty() {
            "stream ended".into()
        } else {
            r
        }
    }
}

impl Drop for Feed {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn spawn_ffmpeg(args: Vec<String>) -> Result<Feed, String> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    unsafe {
        cmd.pre_exec(|| {
            libc::nice(6);
            Ok(())
        });
    }
    let mut child = cmd.spawn().map_err(|e| format!("cannot start ffmpeg: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("ffmpeg stdout unavailable")?;
    let tail = Arc::new(Mutex::new(String::new()));
    if let Some(stderr) = child.stderr.take() {
        let tail = tail.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim();
                if !line.is_empty() {
                    *tail.lock().unwrap() = line.chars().take(300).collect();
                }
            }
        });
    }
    let (tx, rx) = mpsc::channel::<Bytes>(8);
    let reason = Arc::new(Mutex::new(String::new()));
    let reason_out = reason.clone();
    let task = tokio::spawn(async move {
        let mut buf = BytesMut::with_capacity(256 * 1024);
        let msg = loop {
            if buf.capacity() - buf.len() < 32 * 1024 {
                buf.reserve(128 * 1024);
            }
            match stdout.read_buf(&mut buf).await {
                Ok(0) => break "ffmpeg exited".to_string(),
                Ok(_) => {
                    while let Some(frame) = jpeg::take_frame(&mut buf) {
                        if tx.send(frame).await.is_err() {
                            return;
                        }
                    }
                    if buf.len() > MAX_FRAME_BYTES {
                        tracing::warn!("буфер перевищив {MAX_FRAME_BYTES} байт без кадру, скидаю");
                        buf.clear();
                    }
                }
                Err(e) => break format!("read error: {e}"),
            }
        };
        time::sleep(Duration::from_millis(150)).await;
        let tail = tail.lock().unwrap().clone();
        *reason_out.lock().unwrap() = if tail.is_empty() { msg } else { tail };
        let _ = child.kill().await;
    });
    Ok(Feed { rx, task, reason })
}

fn spawn_a9(host: String, protocol: crate::store::A9Protocol, resolution: u8) -> Feed {
    let (tx, rx) = mpsc::channel::<Bytes>(8);
    let reason = Arc::new(Mutex::new(String::new()));
    let reason_out = reason.clone();
    let task = tokio::spawn(async move {
        let result = a9::run(&host, protocol, resolution, tx).await;
        *reason_out.lock().unwrap() = match result {
            Ok(()) => "stream ended".into(),
            Err(e) => e,
        };
    });
    Feed { rx, task, reason }
}

fn add(a: &mut Vec<String>, xs: &[&str]) {
    a.extend(xs.iter().map(|s| s.to_string()));
}

impl Pipeline {
    fn new(cam: &Camera, cfg: Arc<Config>) -> Arc<Self> {
        let (tx, _) = broadcast::channel(4);
        Arc::new(Self {
            label: cam.device_path.clone(),
            kind: cam.kind.clone(),
            cfg,
            tx,
            viewers: AtomicUsize::new(0),
            running: AtomicBool::new(false),
            retired: AtomicBool::new(false),
            good: AtomicBool::new(false),
            profile_idx: AtomicUsize::new(0),
            fps_x10: AtomicU32::new(0),
            info: Mutex::new(Info {
                state: State::Idle,
                profile: None,
                error: None,
                live_since: None,
                latest: None,
            }),
        })
    }

    pub fn viewers(&self) -> usize {
        self.viewers.load(SeqCst)
    }

    pub fn retire(&self) {
        self.retired.store(true, SeqCst);
    }

    pub fn subscribe(self: &Arc<Self>) -> (ViewerGuard, broadcast::Receiver<Frame>) {
        self.viewers.fetch_add(1, SeqCst);
        let guard = ViewerGuard(self.clone());
        let rx = self.tx.subscribe();
        self.ensure_running();
        (guard, rx)
    }

    pub async fn snapshot(self: &Arc<Self>, max_age: Duration, timeout: Duration) -> Option<Bytes> {
        let (_guard, mut rx) = self.subscribe();
        if let Some(f) = self
            .info
            .lock()
            .unwrap()
            .latest
            .as_ref()
            .filter(|f| f.at.elapsed() < max_age)
        {
            return Some(f.jpeg.clone());
        }
        time::timeout(timeout, async {
            loop {
                match rx.recv().await {
                    Ok(f) => return Some(f.jpeg),
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        })
        .await
        .ok()
        .flatten()
    }

    pub fn last_error(&self) -> Option<String> {
        self.info.lock().unwrap().error.clone()
    }

    pub fn status(&self) -> Status {
        let i = self.info.lock().unwrap();
        Status {
            state: i.state,
            fps: if i.state == State::Live {
                self.fps_x10.load(SeqCst) as f32 / 10.0
            } else {
                0.0
            },
            viewers: self.viewers(),
            mode: i.profile,
            frame_age_ms: i.latest.as_ref().map(|f| f.at.elapsed().as_millis() as u64),
            uptime_s: i.live_since.map(|t| t.elapsed().as_secs()),
            error: i.error.clone(),
        }
    }

    fn ensure_running(self: &Arc<Self>) {
        if self.running.compare_exchange(false, true, SeqCst, SeqCst).is_ok() {
            let me = self.clone();
            tokio::spawn(async move { me.run().await });
        }
    }

    fn set_state(&self, state: State, profile: Option<&'static str>, error: Option<String>) {
        let mut i = self.info.lock().unwrap();
        i.state = state;
        if profile.is_some() || state == State::Idle {
            i.profile = profile;
        }
        i.error = error;
        if state != State::Live {
            i.live_since = None;
        }
        if state == State::Idle {
            i.latest = None;
            self.fps_x10.store(0, SeqCst);
        }
    }

    fn profiles(&self) -> Vec<Profile> {
        match &self.kind {
            Kind::Demo => vec![Profile::Demo],
            Kind::Net(Source::A9 { .. }) => vec![Profile::Native],
            Kind::Net(Source::Rtsp { .. }) => vec![Profile::NetTranscode],
            Kind::Net(Source::Http { .. }) => vec![Profile::NetCopy, Profile::NetTranscode],
            Kind::Usb { formats } => match self.cfg.mode {
                ModeSetting::Auto if v4l2::has_mjpeg(formats) => {
                    vec![Profile::Copy, Profile::Transcode, Profile::TranscodeAuto]
                }
                ModeSetting::Auto | ModeSetting::Transcode => vec![Profile::Transcode, Profile::TranscodeAuto],
                ModeSetting::Copy => vec![Profile::Copy],
            },
        }
    }

    async fn run(self: Arc<Self>) {
        let profiles = self.profiles();
        let mut idx = self.profile_idx.load(SeqCst).min(profiles.len() - 1);
        let mut zero_runs = 0u32;
        let mut backoff = 1u64;

        loop {
            if self.viewers() == 0 || self.retired.load(SeqCst) {
                self.set_state(State::Idle, None, None);
                self.running.store(false, SeqCst);
                if self.viewers() > 0
                    && !self.retired.load(SeqCst)
                    && self.running.compare_exchange(false, true, SeqCst, SeqCst).is_ok()
                {
                    continue;
                }
                tracing::info!("{}: нема глядачів, потік зупинено", self.label);
                return;
            }

            let profile = profiles[idx];
            self.set_state(State::Starting, Some(profile.name()), None);
            tracing::info!("{}: запуск потоку (режим {})", self.label, profile.name());

            let out = match self.start_feed(profile) {
                Ok(feed) => self.supervise(feed, profile).await,
                Err(e) => Outcome {
                    frames: 0,
                    stopped_idle: false,
                    error: Some(e),
                },
            };

            if out.frames > 0 {
                self.good.store(true, SeqCst);
                self.profile_idx.store(idx, SeqCst);
                zero_runs = 0;
                backoff = 1;
            } else {
                zero_runs += 1;
                let good = self.good.load(SeqCst);
                if profiles.len() > 1 && (!good || zero_runs >= 3) {
                    tracing::info!(
                        "{}: режим {} не спрацював, пробую наступний",
                        self.label,
                        profile.name()
                    );
                    idx = (idx + 1) % profiles.len();
                    zero_runs = 0;
                    if !good && idx != 0 {
                        continue;
                    }
                }
            }

            if out.stopped_idle {
                continue;
            }

            let msg = out.error.unwrap_or_else(|| "stream ended".into());
            tracing::warn!("{}: {} — перезапуск через {}с", self.label, msg, backoff);
            self.set_state(State::Error, Some(profile.name()), Some(msg));
            time::sleep(Duration::from_secs(backoff)).await;
            backoff = (backoff * 2).min(10);
        }
    }

    fn start_feed(&self, profile: Profile) -> Result<Feed, String> {
        if profile == Profile::Native {
            if let Kind::Net(Source::A9 {
                host,
                protocol,
                resolution,
            }) = &self.kind
            {
                return Ok(spawn_a9(host.clone(), *protocol, *resolution));
            }
        }
        spawn_ffmpeg(self.args(profile))
    }

    fn encode_args(&self) -> Vec<String> {
        let q = self.cfg.quality.to_string();
        [
            "-an", "-pix_fmt", "yuvj420p", "-q:v", &q, "-threads", "1", "-huffman", "default",
        ]
        .map(String::from)
        .to_vec()
    }

    fn net_input(&self, url: &str, rtsp: bool, decode: bool) -> Vec<String> {
        let mut a: Vec<String> = [
            "-fflags",
            "nobuffer",
            "-flags",
            "low_delay",
            "-protocol_whitelist",
            PROTOCOLS,
        ]
        .map(String::from)
        .to_vec();
        if rtsp {
            a.extend(
                [
                    "-rtsp_transport",
                    "tcp",
                    "-probesize",
                    "200000",
                    "-analyzeduration",
                    "500000",
                ]
                .map(String::from),
            );
        }
        if decode {
            a.extend(["-threads", "2"].map(String::from));
        }
        a.extend(["-i".to_string(), url.to_string()]);
        a
    }

    fn net_filter(&self) -> String {
        let c = &self.cfg;
        if c.net_width > 0 {
            format!("fps={},scale={}:-2:flags=fast_bilinear", c.fps, c.net_width)
        } else {
            format!("fps={}", c.fps)
        }
    }

    fn args(&self, p: Profile) -> Vec<String> {
        let c = &self.cfg;
        let size = format!("{}x{}", c.width, c.height);
        let fps = c.fps.to_string();
        let mut a: Vec<String> = ["-hide_banner", "-loglevel", "error", "-nostdin"]
            .map(String::from)
            .to_vec();
        let device = match &self.kind {
            Kind::Usb { .. } => self.label.clone(),
            _ => String::new(),
        };
        let url = match &self.kind {
            Kind::Net(Source::Rtsp { url }) | Kind::Net(Source::Http { url }) => url.clone(),
            _ => String::new(),
        };
        let raw = match &self.kind {
            Kind::Usb { formats } if !v4l2::has_mjpeg(formats) => v4l2::raw_input(formats),
            _ => None,
        };

        match p {
            Profile::Demo => {
                add(
                    &mut a,
                    &["-re", "-f", "lavfi", "-i", &format!("testsrc2=size={size}:rate={fps}")],
                );
                let enc = self.encode_args();
                a.extend(enc);
            }
            Profile::Copy => {
                add(&mut a, &["-fflags", "nobuffer", "-f", "v4l2", "-input_format", "mjpeg"]);
                add(
                    &mut a,
                    &[
                        "-framerate",
                        &fps,
                        "-video_size",
                        &size,
                        "-i",
                        &device,
                        "-an",
                        "-c:v",
                        "copy",
                    ],
                );
            }
            Profile::Transcode => {
                add(&mut a, &["-fflags", "nobuffer", "-flags", "low_delay", "-f", "v4l2"]);
                if let Some(fmt) = raw {
                    add(&mut a, &["-input_format", fmt]);
                }
                add(&mut a, &["-framerate", &fps, "-video_size", &size, "-i", &device]);
                let enc = self.encode_args();
                a.extend(enc);
            }
            Profile::TranscodeAuto => {
                add(
                    &mut a,
                    &[
                        "-fflags",
                        "nobuffer",
                        "-flags",
                        "low_delay",
                        "-f",
                        "v4l2",
                        "-i",
                        &device,
                    ],
                );
                let enc = self.encode_args();
                a.extend(enc);
            }
            Profile::NetCopy => {
                a.extend(self.net_input(&url, false, false));
                add(&mut a, &["-an", "-c:v", "copy"]);
            }
            Profile::NetTranscode => {
                let rtsp = matches!(self.kind, Kind::Net(Source::Rtsp { .. }));
                a.extend(self.net_input(&url, rtsp, true));
                a.extend(["-vf".to_string(), self.net_filter()]);
                let enc = self.encode_args();
                a.extend(enc);
            }
            Profile::Native => {}
        }
        a.extend(["-f", "mjpeg", "pipe:1"].map(String::from));
        a
    }

    async fn supervise(&self, mut feed: Feed, profile: Profile) -> Outcome {
        let mut tick = time::interval(Duration::from_secs(1));
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let idle_limit = Duration::from_secs(self.cfg.idle_secs);

        let mut frames = 0u64;
        let mut last_frame = Instant::now();
        let mut idle_since: Option<Instant> = None;
        let (mut fps_t, mut fps_n) = (Instant::now(), 0u64);
        let mut error: Option<String> = None;
        let mut stopped_idle = false;

        loop {
            tokio::select! {
                f = feed.rx.recv() => match f {
                    Some(j) => {
                        self.publish(j, profile);
                        frames += 1;
                        last_frame = Instant::now();
                    }
                    None => {
                        error = Some(feed.reason());
                        break;
                    }
                },
                _ = tick.tick() => {
                    let now = Instant::now();
                    if self.retired.load(SeqCst) {
                        stopped_idle = true;
                        break;
                    }
                    if self.viewers() == 0 {
                        let since = *idle_since.get_or_insert(now);
                        if now - since >= idle_limit {
                            stopped_idle = true;
                            break;
                        }
                    } else {
                        idle_since = None;
                    }
                    let dt = now - fps_t;
                    if dt >= Duration::from_secs(2) {
                        let fps = (frames - fps_n) as f32 / dt.as_secs_f32();
                        self.fps_x10.store((fps * 10.0).round() as u32, SeqCst);
                        fps_t = now;
                        fps_n = frames;
                    }
                    let limit = if frames == 0 { STARTUP_TIMEOUT } else { STALL_TIMEOUT };
                    if now - last_frame > limit {
                        error = Some("no frames from camera (stalled)".into());
                        break;
                    }
                }
            }
        }

        Outcome {
            frames,
            stopped_idle,
            error,
        }
    }

    fn publish(&self, jpeg: Bytes, profile: Profile) {
        let head = format!(
            "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
            jpeg.len()
        );
        let mut b = BytesMut::with_capacity(head.len() + jpeg.len() + 2);
        b.extend_from_slice(head.as_bytes());
        b.extend_from_slice(&jpeg);
        b.extend_from_slice(b"\r\n");
        let part = b.freeze();
        let jpeg = part.slice(head.len()..head.len() + jpeg.len());
        let frame = Frame {
            part,
            jpeg,
            at: Instant::now(),
        };
        {
            let mut i = self.info.lock().unwrap();
            if i.state != State::Live {
                i.state = State::Live;
                i.profile = Some(profile.name());
                i.live_since = Some(Instant::now());
                i.error = None;
            }
            i.latest = Some(frame.clone());
        }
        let _ = self.tx.send(frame);
    }
}

pub struct Hub {
    cfg: Arc<Config>,
    map: Mutex<HashMap<String, Arc<Pipeline>>>,
}

impl Hub {
    pub fn new(cfg: Arc<Config>) -> Self {
        Self {
            cfg,
            map: Mutex::new(HashMap::new()),
        }
    }

    pub fn pipeline(&self, cam: &Camera) -> Arc<Pipeline> {
        let mut map = self.map.lock().unwrap();
        if let Some(p) = map.get(&cam.device_path) {
            if p.kind == cam.kind {
                return p.clone();
            }
            p.retire();
        }
        let p = Pipeline::new(cam, self.cfg.clone());
        map.insert(cam.device_path.clone(), p.clone());
        p
    }

    pub fn standalone(&self, cam: &Camera) -> Arc<Pipeline> {
        Pipeline::new(cam, self.cfg.clone())
    }

    pub fn status(&self, cam: &Camera) -> Status {
        self.map
            .lock()
            .unwrap()
            .get(&cam.device_path)
            .map(|p| p.status())
            .unwrap_or_else(Status::idle)
    }

    pub fn restart(&self, device_path: &str) {
        if let Some(p) = self.map.lock().unwrap().remove(device_path) {
            p.retire();
        }
    }

    pub fn prune(&self, cams: &[Camera]) {
        self.map.lock().unwrap().retain(|path, p| {
            let keep = p.viewers() > 0 || cams.iter().any(|c| &c.device_path == path);
            if !keep {
                p.retire();
            }
            keep
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::A9Protocol;

    fn cfg() -> Arc<Config> {
        Arc::new(Config {
            bind: [127, 0, 0, 1].into(),
            port: 0,
            width: 1280,
            height: 720,
            fps: 15,
            quality: 5,
            mode: ModeSetting::Auto,
            auth: None,
            idle_secs: 10,
            max_viewers: 8,
            demo: false,
            names: HashMap::new(),
            data_dir: "x".into(),
            net_width: 0,
            session_hours: 1,
        })
    }

    fn usb(formats: &[&str]) -> Camera {
        Camera {
            id: "video0".into(),
            device_path: "/dev/video0".into(),
            name: "cam".into(),
            kind: Kind::Usb {
                formats: formats.iter().map(|s| s.to_string()).collect(),
            },
        }
    }

    fn pipe(cam: &Camera) -> Arc<Pipeline> {
        Pipeline::new(cam, cfg())
    }

    #[test]
    fn mjpeg_camera_tries_copy_first() {
        let p = pipe(&usb(&["MJPG", "YUYV"]));
        assert_eq!(
            p.profiles(),
            vec![Profile::Copy, Profile::Transcode, Profile::TranscodeAuto]
        );
        let args = p.args(Profile::Copy);
        assert!(args.windows(2).any(|w| w == ["-c:v", "copy"]));
        assert!(args.windows(2).any(|w| w == ["-input_format", "mjpeg"]));
    }

    #[test]
    fn raw_camera_skips_copy_and_uses_cheap_encoder_settings() {
        let p = pipe(&usb(&["YUYV"]));
        assert_eq!(p.profiles(), vec![Profile::Transcode, Profile::TranscodeAuto]);
        let args = p.args(Profile::Transcode);
        assert!(args.windows(2).any(|w| w == ["-input_format", "yuyv422"]));
        assert!(args.windows(2).any(|w| w == ["-pix_fmt", "yuvj420p"]));
        assert!(args.windows(2).any(|w| w == ["-huffman", "default"]));
        assert!(args.windows(2).any(|w| w == ["-threads", "1"]));
        assert!(!args.iter().any(|a| a == "copy"));
        assert_eq!(&args[args.len() - 3..], ["-f", "mjpeg", "pipe:1"]);
    }

    #[test]
    fn network_profiles() {
        let rtsp = Camera::net(
            "ip1",
            "r",
            Source::Rtsp {
                url: "rtsp://10.0.0.5/s".into(),
            },
        );
        let p = pipe(&rtsp);
        assert_eq!(p.profiles(), vec![Profile::NetTranscode]);
        let a = p.args(Profile::NetTranscode);
        assert!(a.windows(2).any(|w| w == ["-rtsp_transport", "tcp"]));
        assert!(a.windows(2).any(|w| w == ["-vf", "fps=15"]));
        assert!(a.windows(2).any(|w| w == ["-i", "rtsp://10.0.0.5/s"]));
        assert!(a
            .windows(2)
            .any(|w| w[0] == "-protocol_whitelist" && !w[1].contains("file")));

        let http = Camera::net(
            "ip2",
            "h",
            Source::Http {
                url: "http://10.0.0.6/video".into(),
            },
        );
        let p = pipe(&http);
        assert_eq!(p.profiles(), vec![Profile::NetCopy, Profile::NetTranscode]);
        assert!(!p.args(Profile::NetCopy).iter().any(|a| a == "-rtsp_transport"));

        let a9 = Camera::net(
            "ip3",
            "a",
            Source::A9 {
                host: "10.0.0.7".into(),
                protocol: A9Protocol::Auto,
                resolution: 2,
            },
        );
        assert_eq!(pipe(&a9).profiles(), vec![Profile::Native]);
    }

    #[test]
    fn hub_replaces_pipeline_when_source_changes() {
        let hub = Hub::new(cfg());
        let a = Camera::net(
            "ip1",
            "a",
            Source::Rtsp {
                url: "rtsp://1.1.1.1/a".into(),
            },
        );
        let p1 = hub.pipeline(&a);
        assert!(Arc::ptr_eq(&p1, &hub.pipeline(&a)));
        let b = Camera::net(
            "ip1",
            "a",
            Source::Rtsp {
                url: "rtsp://1.1.1.1/b".into(),
            },
        );
        let p2 = hub.pipeline(&b);
        assert!(!Arc::ptr_eq(&p1, &p2));
        assert!(p1.retired.load(SeqCst));
    }
}
