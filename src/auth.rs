use std::{
    collections::{hash_map::RandomState, HashMap},
    hash::{BuildHasher, Hasher},
    io::Read,
    net::{IpAddr, SocketAddr},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use serde_json::json;

use crate::state::AppState;

pub const COOKIE: &str = "picctv_session";
const MAX_SESSIONS: usize = 64;
const MAX_FAILS: u32 = 10;
const FAIL_WINDOW: Duration = Duration::from_secs(300);
const PUBLIC: [&str; 9] = [
    "/",
    "/healthz",
    "/api/login",
    "/api/logout",
    "/api/session",
    "/app.css",
    "/app.js",
    "/logo.svg",
    "/favicon.svg",
];

pub struct Sessions {
    ttl: Duration,
    tokens: Mutex<HashMap<String, Instant>>,
    fails: Mutex<HashMap<IpAddr, (u32, Instant)>>,
}

fn random_token() -> String {
    let mut bytes = [0u8; 24];
    let ok = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .is_ok();
    if !ok {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        for chunk in bytes.chunks_mut(8) {
            let mut h = RandomState::new().build_hasher();
            h.write_u128(nanos);
            let v = h.finish().to_le_bytes();
            chunk.copy_from_slice(&v[..chunk.len()]);
        }
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Sessions {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            tokens: Mutex::new(HashMap::new()),
            fails: Mutex::new(HashMap::new()),
        }
    }

    pub fn ttl_secs(&self) -> u64 {
        self.ttl.as_secs()
    }

    pub fn create(&self) -> String {
        let token = random_token();
        let now = Instant::now();
        let mut t = self.tokens.lock().unwrap();
        t.retain(|_, exp| *exp > now);
        while t.len() >= MAX_SESSIONS {
            let Some(oldest) = t.iter().min_by_key(|(_, exp)| **exp).map(|(k, _)| k.clone()) else {
                break;
            };
            t.remove(&oldest);
        }
        t.insert(token.clone(), now + self.ttl);
        token
    }

    pub fn valid(&self, token: &str) -> bool {
        self.tokens
            .lock()
            .unwrap()
            .get(token)
            .is_some_and(|exp| *exp > Instant::now())
    }

    pub fn revoke(&self, token: &str) {
        self.tokens.lock().unwrap().remove(token);
    }

    pub fn blocked(&self, ip: IpAddr) -> bool {
        let mut f = self.fails.lock().unwrap();
        f.retain(|_, (_, at)| at.elapsed() < FAIL_WINDOW);
        f.get(&ip).is_some_and(|(n, _)| *n >= MAX_FAILS)
    }

    pub fn fail(&self, ip: IpAddr) {
        let mut f = self.fails.lock().unwrap();
        let e = f.entry(ip).or_insert((0, Instant::now()));
        e.0 += 1;
        e.1 = Instant::now();
    }

    pub fn clear_fails(&self, ip: IpAddr) {
        self.fails.lock().unwrap().remove(&ip);
    }
}

fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= (*a.get(i).unwrap_or(&0) ^ *b.get(i).unwrap_or(&0)) as usize;
    }
    diff == 0
}

fn credentials_ok(header_value: Option<&str>, user: &str, pass: &str) -> bool {
    let Some(encoded) = header_value.and_then(|v| v.strip_prefix("Basic ")) else {
        return false;
    };
    let Ok(decoded) = STANDARD.decode(encoded.trim()) else {
        return false;
    };
    let Ok(text) = String::from_utf8(decoded) else {
        return false;
    };
    match text.split_once(':') {
        Some((u, p)) => ct_eq(u, user) & ct_eq(p, pass),
        None => false,
    }
}

fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

pub fn is_authed(state: &AppState, headers: &HeaderMap) -> bool {
    let Some((user, pass)) = &state.cfg.auth else {
        return true;
    };
    if let Some(token) = cookie_value(headers, COOKIE) {
        if state.sessions.valid(token) {
            return true;
        }
    }
    let basic = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    credentials_ok(basic, user, pass)
}

pub async fn require_auth(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let path = req.uri().path();
    if state.cfg.auth.is_none() || PUBLIC.contains(&path) || is_authed(&state, req.headers()) {
        return next.run(req).await;
    }
    if req.headers().contains_key(header::AUTHORIZATION) {
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    let api = path.starts_with("/api/");
    let browser = req.headers().contains_key("sec-fetch-mode");
    let mut resp = if api {
        (StatusCode::UNAUTHORIZED, Json(json!({ "error": "unauthorized" }))).into_response()
    } else {
        (StatusCode::UNAUTHORIZED, "Unauthorized").into_response()
    };
    if !api && !browser {
        resp.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Basic realm=\"PiCCTV\", charset=\"UTF-8\""),
        );
    }
    resp
}

#[derive(Deserialize)]
pub struct LoginBody {
    password: String,
}

fn session_cookie(value: &str, max_age: u64, secure: bool) -> String {
    format!(
        "{COOKIE}={value}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}{}",
        if secure { "; Secure" } else { "" }
    )
}

fn is_https(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("https"))
}

pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<LoginBody>,
) -> Response {
    let Some((_, pass)) = &state.cfg.auth else {
        return Json(json!({ "ok": true })).into_response();
    };
    let ip = addr.ip();
    if state.sessions.blocked(ip) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": "too_many_attempts" })),
        )
            .into_response();
    }
    if !ct_eq(&body.password, pass) {
        state.sessions.fail(ip);
        tokio::time::sleep(Duration::from_millis(600)).await;
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "bad_password" }))).into_response();
    }
    state.sessions.clear_fails(ip);
    let token = state.sessions.create();
    let cookie = session_cookie(&token, state.sessions.ttl_secs(), is_https(&headers));
    let mut resp = Json(json!({ "ok": true })).into_response();
    if let Ok(v) = HeaderValue::from_str(&cookie) {
        resp.headers_mut().insert(header::SET_COOKIE, v);
    }
    resp
}

pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = cookie_value(&headers, COOKIE) {
        state.sessions.revoke(token);
    }
    let mut resp = Json(json!({ "ok": true })).into_response();
    if let Ok(v) = HeaderValue::from_str(&session_cookie("", 0, is_https(&headers))) {
        resp.headers_mut().insert(header::SET_COOKIE, v);
    }
    resp
}

pub async fn session(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let required = state.cfg.auth.is_some();
    let authed = is_authed(&state, &headers);
    Json(json!({ "auth_required": required, "authed": authed, "admin": required && authed })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn basic(s: &str) -> String {
        format!("Basic {}", STANDARD.encode(s))
    }

    #[test]
    fn accepts_correct() {
        assert!(credentials_ok(Some(&basic("admin:secret")), "admin", "secret"));
        assert!(credentials_ok(Some(&basic("u:pa:ss")), "u", "pa:ss"));
    }

    #[test]
    fn rejects_wrong() {
        assert!(!credentials_ok(None, "a", "b"));
        assert!(!credentials_ok(Some(&basic("admin:wrong")), "admin", "secret"));
        assert!(!credentials_ok(Some(&basic("admin:secre")), "admin", "secret"));
        assert!(!credentials_ok(Some("Bearer abc"), "admin", "secret"));
        assert!(!credentials_ok(Some("Basic !!!"), "admin", "secret"));
        assert!(!credentials_ok(Some(&basic("nocolon")), "nocolon", ""));
    }

    #[test]
    fn sessions_lifecycle() {
        let s = Sessions::new(Duration::from_secs(60));
        let t = s.create();
        assert_eq!(t.len(), 48);
        assert!(s.valid(&t));
        assert!(!s.valid("nope"));
        s.revoke(&t);
        assert!(!s.valid(&t));
        assert_ne!(s.create(), s.create());
    }

    #[test]
    fn sessions_expire_and_are_capped() {
        let s = Sessions::new(Duration::from_millis(1));
        let t = s.create();
        std::thread::sleep(Duration::from_millis(5));
        assert!(!s.valid(&t));
        let s = Sessions::new(Duration::from_secs(60));
        for _ in 0..(MAX_SESSIONS + 10) {
            s.create();
        }
        assert!(s.tokens.lock().unwrap().len() <= MAX_SESSIONS);
    }

    #[test]
    fn login_throttle() {
        let s = Sessions::new(Duration::from_secs(60));
        let ip: IpAddr = "10.0.0.9".parse().unwrap();
        for _ in 0..MAX_FAILS {
            assert!(!s.blocked(ip));
            s.fail(ip);
        }
        assert!(s.blocked(ip));
        s.clear_fails(ip);
        assert!(!s.blocked(ip));
    }

    #[test]
    fn cookie_parsing() {
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, HeaderValue::from_static("a=1; picctv_session=abc; b=2"));
        assert_eq!(cookie_value(&h, COOKIE), Some("abc"));
        assert_eq!(cookie_value(&h, "zzz"), None);
    }
}
