use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr},
    time::{Duration, Instant},
};

use bytes::{Bytes, BytesMut};
use tokio::{
    net::UdpSocket,
    sync::mpsc::{self, error::TrySendError},
    time::{self, MissedTickBehavior},
};

use crate::{
    jpeg::{self, Scan},
    store::A9Protocol,
};

const PPPP_PORT: u16 = 32108;
const LEGACY_PORT: u16 = 8080;

const CMD_LAN_SEARCH: u16 = 0xf130;
const CMD_PUNCH_PKT: u16 = 0xf141;
const CMD_P2P_RDY: u16 = 0xf142;
const CMD_ALIVE: u16 = 0xf1e0;
const CMD_ALIVE_ACK: u16 = 0xf1e1;
const CMD_DRW: u16 = 0xf1d0;
const CMD_DRW_ACK: u16 = 0xf1d1;
const CMD_CLOSE: u16 = 0xf1f0;

const CTRL_CONNECT_USER: u16 = 0x2010;
const CTRL_CONNECT_USER_ACK: u16 = 0x2011;
const CTRL_START_VIDEO: u16 = 0x1030;
const CTRL_VIDEO_PARAM_SET: u16 = 0x1830;

const CTRL_START_MARK: u16 = 0x110a;
const FRAME_MAGIC: [u8; 4] = [0x55, 0xaa, 0x15, 0xa8];
const JPEG_MAGIC: [u8; 4] = [0xff, 0xd8, 0xff, 0xdb];
const STREAM_JPEG: u8 = 0x03;
const CHUNK: usize = 1024;
const FRAME_HEADER: usize = 32;
const MAX_FRAME: usize = 2 * 1024 * 1024;
const SILENCE_LIMIT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub ip: Ipv4Addr,
    pub id: String,
}

fn be16(d: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([d[o], d[o + 1]])
}

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&x| x == 0).unwrap_or(b.len());
    b[..end]
        .iter()
        .filter(|c| c.is_ascii_graphic())
        .map(|&c| c as char)
        .collect()
}

fn scramble(d: &mut [u8]) {
    for b in d.iter_mut() {
        *b ^= 1;
    }
}

fn encrypt(d: &mut [u8]) {
    scramble(d);
    let k = 4.min(d.len());
    d.rotate_left(k);
}

fn decrypt(d: &mut [u8]) {
    scramble(d);
    let k = 4.min(d.len());
    d.rotate_right(k);
}

fn lan_search() -> [u8; 4] {
    let c = CMD_LAN_SEARCH.to_be_bytes();
    [c[0], c[1], 0, 0]
}

fn alive() -> [u8; 4] {
    let c = CMD_ALIVE.to_be_bytes();
    [c[0], c[1], 0, 0]
}

fn alive_ack() -> [u8; 4] {
    let c = CMD_ALIVE_ACK.to_be_bytes();
    [c[0], c[1], 0, 0]
}

fn p2p_rdy(punch: &[u8]) -> Vec<u8> {
    let mut body = [0u8; 20];
    let src = punch.get(4..).unwrap_or(&[]);
    let n = src.len().min(20);
    body[..n].copy_from_slice(&src[..n]);
    let mut v = Vec::with_capacity(24);
    v.extend(CMD_P2P_RDY.to_be_bytes());
    v.extend(20u16.to_be_bytes());
    v.extend(body);
    v
}

fn drw_ack(stream: u8, id: u16) -> Vec<u8> {
    let mut v = vec![0u8; 32];
    v[..2].copy_from_slice(&CMD_DRW_ACK.to_be_bytes());
    v[2..4].copy_from_slice(&6u16.to_be_bytes());
    v[4] = 0xd2;
    v[5] = stream;
    v[6..8].copy_from_slice(&1u16.to_be_bytes());
    v[8..10].copy_from_slice(&id.to_be_bytes());
    v
}

fn drw(cmd: u16, id: u16, ticket: [u8; 4], data: Option<&[u8]>) -> Vec<u8> {
    let mut body: Vec<u8> = match data {
        Some(d) if d.len() > 4 => d.to_vec(),
        _ => Vec::new(),
    };
    encrypt_if_long(&mut body);
    let pkt_len = 0x10 + 4 + body.len();
    let payload_len = (4 + body.len()) as u16;
    let dest: u16 = if cmd == CTRL_CONNECT_USER { 0xff00 } else { 0 };
    let mut v = Vec::with_capacity(pkt_len);
    v.extend(CMD_DRW.to_be_bytes());
    v.extend(((pkt_len - 4) as u16).to_be_bytes());
    v.push(0xd1);
    v.push(0);
    v.extend(id.to_be_bytes());
    v.extend(CTRL_START_MARK.to_be_bytes());
    v.extend(cmd.to_be_bytes());
    v.extend(payload_len.to_le_bytes());
    v.extend(dest.to_be_bytes());
    v.extend(ticket);
    v.extend(body);
    v
}

fn encrypt_if_long(body: &mut [u8]) {
    if body.len() > 4 {
        encrypt(body);
    }
}

fn login_payload() -> Vec<u8> {
    let mut p = vec![0u8; 0x20 + 0x80];
    p[..5].copy_from_slice(b"admin");
    p[0x20..0x25].copy_from_slice(b"admin");
    p
}

fn parse_punch(d: &[u8]) -> Option<String> {
    if d.len() < 16 || be16(d, 0) != CMD_PUNCH_PKT {
        return None;
    }
    let prefix = cstr(&d[4..8]);
    let serial = u64::from_be_bytes(d[8..16].try_into().ok()?);
    let end = (4 + be16(d, 2) as usize).min(d.len());
    let suffix = if end > 16 { cstr(&d[16..end]) } else { String::new() };
    Some(format!("{prefix}{serial}{suffix}"))
}

pub async fn discover(window: Duration) -> Vec<Found> {
    let Ok(sock) = UdpSocket::bind("0.0.0.0:0").await else {
        return Vec::new();
    };
    let _ = sock.set_broadcast(true);
    let target = SocketAddr::new(Ipv4Addr::BROADCAST.into(), PPPP_PORT);
    let mut found: Vec<Found> = Vec::new();
    let mut buf = [0u8; 512];
    let deadline = Instant::now() + window;
    let mut next_probe = Instant::now();
    while Instant::now() < deadline {
        if Instant::now() >= next_probe {
            let _ = sock.send_to(&lan_search(), target).await;
            next_probe = Instant::now() + Duration::from_millis(700);
        }
        let wait = next_probe
            .saturating_duration_since(Instant::now())
            .max(Duration::from_millis(20));
        if let Ok(Ok((n, SocketAddr::V4(src)))) = time::timeout(wait, sock.recv_from(&mut buf)).await {
            if let Some(id) = parse_punch(&buf[..n]) {
                let f = Found { ip: *src.ip(), id };
                if !found.contains(&f) {
                    found.push(f);
                }
            }
        }
    }
    found
}

async fn locate(sock: &UdpSocket, host: Ipv4Addr, attempts: u32, wait: Duration) -> Option<(SocketAddr, Vec<u8>)> {
    let broadcast = host == Ipv4Addr::BROADCAST;
    if broadcast {
        let _ = sock.set_broadcast(true);
    }
    let target = SocketAddr::new(host.into(), PPPP_PORT);
    let mut buf = [0u8; 512];
    for _ in 0..attempts {
        sock.send_to(&lan_search(), target).await.ok()?;
        let deadline = Instant::now() + wait;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match time::timeout(left, sock.recv_from(&mut buf)).await {
                Ok(Ok((n, src))) => {
                    let ours = broadcast || src.ip() == std::net::IpAddr::V4(host);
                    if ours && parse_punch(&buf[..n]).is_some() {
                        return Some((src, buf[..n].to_vec()));
                    }
                }
                _ => break,
            }
        }
    }
    None
}

pub async fn run(host: &str, protocol: A9Protocol, resolution: u8, tx: mpsc::Sender<Bytes>) -> Result<(), String> {
    let ip: Ipv4Addr = host.trim().parse().map_err(|_| "invalid camera address".to_string())?;
    let sock = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| format!("cannot open UDP socket: {e}"))?;
    match protocol {
        A9Protocol::Legacy => legacy(sock, ip, tx).await,
        A9Protocol::Pppp => {
            let (peer, punch) = locate(&sock, ip, 3, Duration::from_millis(1200))
                .await
                .ok_or_else(|| format!("no reply from {ip} on UDP {PPPP_PORT}"))?;
            tracing::info!(
                "A9 {ip}: знайдено камеру (PPPP), {}",
                parse_punch(&punch).unwrap_or_default()
            );
            pppp(sock, peer, punch, resolution, tx).await
        }
        A9Protocol::Auto => match locate(&sock, ip, 2, Duration::from_millis(1200)).await {
            Some((peer, punch)) => {
                tracing::info!(
                    "A9 {ip}: знайдено камеру (PPPP), {}",
                    parse_punch(&punch).unwrap_or_default()
                );
                pppp(sock, peer, punch, resolution, tx).await
            }
            None => {
                tracing::info!("A9 {ip}: PPPP не відповів, пробую старий протокол (UDP {LEGACY_PORT})");
                legacy(sock, ip, tx).await
            }
        },
    }
}

fn emit(tx: &mpsc::Sender<Bytes>, frame: Bytes) -> bool {
    !matches!(tx.try_send(frame), Err(TrySendError::Closed(_)))
}

async fn legacy(sock: UdpSocket, ip: Ipv4Addr, tx: mpsc::Sender<Bytes>) -> Result<(), String> {
    let target = SocketAddr::new(ip.into(), LEGACY_PORT);
    let trigger = [0x42u8, 0x76];
    sock.send_to(&trigger, target)
        .await
        .map_err(|e| format!("send failed: {e}"))?;
    let mut buf = vec![0u8; 4096];
    let mut acc = BytesMut::with_capacity(128 * 1024);
    let mut last_rx = Instant::now();
    let mut last_trigger = Instant::now();
    let mut tick = time::interval(Duration::from_secs(1));
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            r = sock.recv_from(&mut buf) => {
                let (n, src) = r.map_err(|e| format!("receive failed: {e}"))?;
                if src.ip() != std::net::IpAddr::V4(ip) || src.port() != LEGACY_PORT || n <= 8 {
                    continue;
                }
                last_rx = Instant::now();
                acc.extend_from_slice(&buf[8..n]);
                while let Some(f) = jpeg::take_frame(&mut acc) {
                    if !emit(&tx, f) {
                        return Ok(());
                    }
                }
                if acc.len() > MAX_FRAME {
                    acc.clear();
                }
            }
            _ = tick.tick() => {
                if last_rx.elapsed() > SILENCE_LIMIT + Duration::from_secs(2) {
                    return Err(format!("no data from {ip} on UDP {LEGACY_PORT}"));
                }
                if last_rx.elapsed() > Duration::from_secs(3) && last_trigger.elapsed() > Duration::from_secs(2) {
                    let _ = sock.send_to(&trigger, target).await;
                    last_trigger = Instant::now();
                }
            }
        }
    }
}

struct Assembler {
    buf: Vec<u8>,
    active: bool,
    bad: bool,
    seq: u16,
}

impl Assembler {
    fn new() -> Self {
        Self {
            buf: Vec::with_capacity(64 * 1024),
            active: false,
            bad: false,
            seq: 0,
        }
    }

    fn flush(&mut self, out: &mut Vec<Bytes>) {
        if self.active && !self.bad && !self.buf.is_empty() {
            if let Scan::Complete(n) = jpeg::scan_frame(&self.buf) {
                out.push(Bytes::copy_from_slice(&self.buf[..n]));
            }
        }
        self.active = false;
        self.buf.clear();
    }

    fn begin(&mut self, data: &[u8], id: u16, out: &mut Vec<Bytes>) {
        self.flush(out);
        self.active = true;
        self.bad = false;
        self.seq = id;
        self.buf.extend_from_slice(data);
    }

    fn feed(&mut self, d: &[u8], out: &mut Vec<Bytes>) {
        if d.len() < 12 {
            return;
        }
        let pkt_len = be16(d, 2) as usize;
        if pkt_len < 12 {
            return;
        }
        let id = be16(d, 6);
        let end = (4 + pkt_len).min(d.len());
        if d[8..12] == FRAME_MAGIC {
            if d.len() > 12 && d[12] == STREAM_JPEG && end > 8 + FRAME_HEADER {
                self.begin(&d[8 + FRAME_HEADER..end], id, out);
            }
            return;
        }
        let data = &d[8..end];
        if data.len() >= 4 && data[..4] == JPEG_MAGIC {
            self.begin(data, id, out);
            return;
        }
        if !self.active || self.bad {
            return;
        }
        let diff = id.wrapping_sub(self.seq) as i16;
        if diff <= 0 {
            return;
        }
        if diff > 1 {
            self.bad = true;
            return;
        }
        self.seq = id;
        if self.buf.len() + data.len() > MAX_FRAME {
            self.active = false;
            self.buf.clear();
            return;
        }
        self.buf.extend_from_slice(data);
        if data.len() < CHUNK {
            self.flush(out);
        }
    }
}

struct Pending {
    sent: Instant,
    tries: u32,
    packet: Vec<u8>,
}

struct Session {
    ticket: [u8; 4],
    next_id: u16,
    pending: HashMap<u16, Pending>,
    login_sent: bool,
    logged_in: bool,
    resolution: u8,
}

impl Session {
    fn queue(&mut self, cmd: u16, data: Option<&[u8]>) -> Vec<u8> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let packet = drw(cmd, id, self.ticket, data);
        self.pending.insert(
            id,
            Pending {
                sent: Instant::now(),
                tries: 0,
                packet: packet.clone(),
            },
        );
        packet
    }

    fn resend_due(&mut self) -> Vec<Vec<u8>> {
        let now = Instant::now();
        let due: Vec<u16> = self
            .pending
            .iter()
            .filter(|(_, p)| now.duration_since(p.sent) > Duration::from_millis(300))
            .map(|(k, _)| *k)
            .collect();
        let mut out = Vec::new();
        for old in due {
            let Some(mut p) = self.pending.remove(&old) else {
                continue;
            };
            if p.tries >= 8 {
                continue;
            }
            let id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1);
            p.packet[6..8].copy_from_slice(&id.to_be_bytes());
            p.sent = now;
            p.tries += 1;
            out.push(p.packet.clone());
            self.pending.insert(id, p);
        }
        out
    }
}

async fn pppp(
    sock: UdpSocket,
    peer: SocketAddr,
    punch: Vec<u8>,
    resolution: u8,
    tx: mpsc::Sender<Bytes>,
) -> Result<(), String> {
    sock.connect(peer).await.map_err(|e| format!("connect failed: {e}"))?;
    let io = |e: std::io::Error| format!("network error: {e}");
    let rdy = p2p_rdy(&punch);
    sock.send(&rdy).await.map_err(io)?;

    let mut s = Session {
        ticket: [0; 4],
        next_id: 0,
        pending: HashMap::new(),
        login_sent: false,
        logged_in: false,
        resolution,
    };
    let mut asm = Assembler::new();
    let mut frames: Vec<Bytes> = Vec::new();
    let mut buf = vec![0u8; 2048];
    let mut last_rx = Instant::now();
    let mut login_at = Instant::now();
    let mut rdy_tries = 0u32;
    let mut tick = time::interval(Duration::from_millis(200));
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            r = sock.recv(&mut buf) => {
                let n = r.map_err(io)?;
                if n < 4 {
                    continue;
                }
                last_rx = Instant::now();
                let pkt = &buf[..n];
                match be16(pkt, 0) {
                    CMD_ALIVE => {
                        sock.send(&alive_ack()).await.map_err(io)?;
                    }
                    CMD_P2P_RDY => {
                        if !s.login_sent {
                            s.login_sent = true;
                            login_at = Instant::now();
                            let p = s.queue(CTRL_CONNECT_USER, Some(&login_payload()));
                            sock.send(&p).await.map_err(io)?;
                        }
                    }
                    CMD_DRW_ACK => {
                        if n >= 10 {
                            let count = (be16(pkt, 6) as usize).min((n - 8) / 2);
                            for i in 0..count {
                                s.pending.remove(&be16(pkt, 8 + i * 2));
                            }
                        }
                    }
                    CMD_DRW => {
                        if n < 8 {
                            continue;
                        }
                        let stream = pkt[5];
                        sock.send(&drw_ack(stream, be16(pkt, 6))).await.map_err(io)?;
                        if stream == 1 {
                            asm.feed(pkt, &mut frames);
                            for f in frames.drain(..) {
                                if !emit(&tx, f) {
                                    return Ok(());
                                }
                            }
                        } else if stream == 0 && !s.logged_in {
                            if let Some(ticket) = parse_login_ack(pkt) {
                                s.ticket = ticket;
                                s.logged_in = true;
                                let res = [1u8, 0, 0, 0, s.resolution.clamp(1, 4), 0, 0, 0];
                                let a = s.queue(CTRL_VIDEO_PARAM_SET, Some(&res));
                                let b = s.queue(CTRL_START_VIDEO, None);
                                sock.send(&a).await.map_err(io)?;
                                sock.send(&b).await.map_err(io)?;
                            }
                        }
                    }
                    CMD_CLOSE => return Err("camera closed the session".into()),
                    _ => {}
                }
            }
            _ = tick.tick() => {
                let idle = last_rx.elapsed();
                if idle > SILENCE_LIMIT {
                    return Err(format!("camera {peer} stopped responding"));
                }
                if idle > Duration::from_millis(600) {
                    sock.send(&alive()).await.map_err(io)?;
                }
                if !s.login_sent && rdy_tries < 8 && idle > Duration::from_millis(800) {
                    rdy_tries += 1;
                    sock.send(&rdy).await.map_err(io)?;
                }
                if s.login_sent && !s.logged_in && login_at.elapsed() > Duration::from_secs(5) {
                    return Err("camera did not accept login (admin/admin)".into());
                }
                for p in s.resend_due() {
                    sock.send(&p).await.map_err(io)?;
                }
            }
        }
    }
}

fn parse_login_ack(pkt: &[u8]) -> Option<[u8; 4]> {
    if pkt.len() < 28 || be16(pkt, 8) != CTRL_START_MARK || be16(pkt, 10) != CTRL_CONNECT_USER_ACK {
        return None;
    }
    let payload_len = u16::from_le_bytes([pkt[12], pkt[13]]) as usize;
    let dlen = payload_len.saturating_sub(4).min(pkt.len() - 20);
    if dlen < 8 {
        return None;
    }
    let mut data = pkt[20..20 + dlen].to_vec();
    decrypt(&mut data);
    data[4..8].try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg_sample(len: usize) -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8, 0xFF, 0xDB, 0x00, 0x05, 1, 2, 3];
        v.extend([0xFF, 0xDA, 0x00, 0x04, 0xAA, 0xBB]);
        while v.len() < len {
            v.push(0x11);
            v.push(0x22);
        }
        v.extend([0xFF, 0xD9]);
        v
    }

    fn data_packet(id: u16, framed: bool, payload: &[u8]) -> Vec<u8> {
        let head = if framed { FRAME_HEADER } else { 0 };
        let pkt_len = 4 + head + payload.len();
        let mut v = vec![0xf1, 0xd0];
        v.extend((pkt_len as u16).to_be_bytes());
        v.extend([0xd1, 0x01]);
        v.extend(id.to_be_bytes());
        if framed {
            let mut h = vec![0u8; FRAME_HEADER];
            h[..4].copy_from_slice(&FRAME_MAGIC);
            h[4] = STREAM_JPEG;
            v.extend(h);
        }
        v.extend(payload);
        v
    }

    fn split_frame(jpeg: &[u8], first_id: u16, framed: bool) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let mut id = first_id;
        let mut rest = jpeg;
        let mut first = true;
        while !rest.is_empty() {
            let take = rest.len().min(CHUNK);
            out.push(data_packet(id, framed && first, &rest[..take]));
            rest = &rest[take..];
            id = id.wrapping_add(1);
            first = false;
        }
        out
    }

    #[test]
    fn cipher_roundtrip() {
        let original: Vec<u8> = (0..37u8).collect();
        let mut d = original.clone();
        encrypt(&mut d);
        assert_ne!(d, original);
        decrypt(&mut d);
        assert_eq!(d, original);
    }

    #[test]
    fn drw_layout() {
        let p = drw(CTRL_START_VIDEO, 7, [1, 2, 3, 4], None);
        assert_eq!(p.len(), 20);
        assert_eq!(&p[..4], &[0xf1, 0xd0, 0x00, 0x10]);
        assert_eq!(&p[4..8], &[0xd1, 0x00, 0x00, 0x07]);
        assert_eq!(&p[8..12], &[0x11, 0x0a, 0x10, 0x30]);
        assert_eq!(&p[12..16], &[0x04, 0x00, 0x00, 0x00]);
        assert_eq!(&p[16..20], &[1, 2, 3, 4]);
        let login = drw(CTRL_CONNECT_USER, 0, [0; 4], Some(&login_payload()));
        assert_eq!(login.len(), 20 + 0xa0);
        assert_eq!(&login[14..16], &[0xff, 0x00]);
        assert_eq!(u16::from_le_bytes([login[12], login[13]]), 4 + 0xa0);
        let mut body = login[20..].to_vec();
        decrypt(&mut body);
        assert_eq!(&body[..5], b"admin");
        assert_eq!(&body[0x20..0x25], b"admin");
    }

    #[test]
    fn punch_parsing() {
        let mut p = vec![0xf1, 0x41, 0x00, 0x14];
        p.extend(b"BATC");
        p.extend(609531u64.to_be_bytes());
        p.extend(b"EXLVS\0\0\0");
        assert_eq!(parse_punch(&p).as_deref(), Some("BATC609531EXLVS"));
        assert_eq!(parse_punch(&p[..8]), None);
        let rdy = p2p_rdy(&p);
        assert_eq!(rdy.len(), 24);
        assert_eq!(&rdy[..4], &[0xf1, 0x42, 0x00, 0x14]);
        assert_eq!(&rdy[4..], &p[4..24]);
    }

    #[test]
    fn assembles_framed_and_unframed() {
        for framed in [true, false] {
            let j = jpeg_sample(3000);
            let mut asm = Assembler::new();
            let mut out = Vec::new();
            for p in split_frame(&j, 100, framed) {
                asm.feed(&p, &mut out);
            }
            assert_eq!(out.len(), 1, "framed={framed}");
            assert_eq!(&out[0][..], &j[..]);
        }
    }

    #[test]
    fn drops_frame_with_lost_packet_and_recovers() {
        let j = jpeg_sample(4000);
        let mut asm = Assembler::new();
        let mut out = Vec::new();
        let mut pkts = split_frame(&j, 10, true);
        pkts.remove(2);
        for p in &pkts {
            asm.feed(p, &mut out);
        }
        assert!(out.is_empty());
        for p in split_frame(&j, 20, true) {
            asm.feed(&p, &mut out);
        }
        assert_eq!(out.len(), 1);
        assert_eq!(&out[0][..], &j[..]);
    }

    #[test]
    fn ignores_retransmits_and_handles_wraparound() {
        let j = jpeg_sample(3500);
        let mut asm = Assembler::new();
        let mut out = Vec::new();
        let pkts = split_frame(&j, 65534, true);
        asm.feed(&pkts[0], &mut out);
        asm.feed(&pkts[1], &mut out);
        asm.feed(&pkts[1], &mut out);
        for p in &pkts[2..] {
            asm.feed(p, &mut out);
        }
        assert_eq!(out.len(), 1);
        assert_eq!(&out[0][..], &j[..]);
    }

    #[tokio::test]
    async fn full_session_against_fake_camera() {
        let cam = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let cam_addr = cam.local_addr().unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let jpg = jpeg_sample(5000);
        let jpg2 = jpg.clone();

        let fake = tokio::spawn(async move {
            let mut buf = vec![0u8; 2048];
            let mut client_addr = None;
            let mut started = false;
            let mut res_seen = 0u8;
            let mut ticket_ok = false;
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                let Ok(Ok((n, src))) = time::timeout(Duration::from_millis(200), cam.recv_from(&mut buf)).await else {
                    continue;
                };
                client_addr = Some(src);
                let p = buf[..n].to_vec();
                match be16(&p, 0) {
                    CMD_P2P_RDY => {
                        cam.send_to(&p, src).await.unwrap();
                    }
                    CMD_DRW => {
                        cam.send_to(&drw_ack(0, be16(&p, 6)), src).await.unwrap();
                        let cmd = be16(&p, 10);
                        if cmd == CTRL_CONNECT_USER {
                            let mut body = vec![0u8; 12];
                            body[4..8].copy_from_slice(&[9, 8, 7, 6]);
                            let mut enc = body.clone();
                            encrypt(&mut enc);
                            let mut ack = vec![0xf1, 0xd0, 0x00, 0x00, 0xd1, 0x00, 0x00, 0x00];
                            ack.extend(CTRL_START_MARK.to_be_bytes());
                            ack.extend(CTRL_CONNECT_USER_ACK.to_be_bytes());
                            ack.extend(((4 + enc.len()) as u16).to_le_bytes());
                            ack.extend([0xaa, 0x55, 0, 0, 0, 0]);
                            ack.extend(enc);
                            let len = (ack.len() - 4) as u16;
                            ack[2..4].copy_from_slice(&len.to_be_bytes());
                            cam.send_to(&ack, src).await.unwrap();
                        } else if cmd == CTRL_VIDEO_PARAM_SET {
                            let mut body = p[20..].to_vec();
                            decrypt(&mut body);
                            res_seen = body[4];
                            ticket_ok = p[16..20] == [9, 8, 7, 6];
                        } else if cmd == CTRL_START_VIDEO {
                            started = true;
                            for pkt in split_frame(&jpg2, 500, true) {
                                cam.send_to(&pkt, src).await.unwrap();
                            }
                        }
                    }
                    _ => {}
                }
                if started {
                    time::sleep(Duration::from_millis(500)).await;
                    break;
                }
            }
            let _ = client_addr;
            (started, res_seen, ticket_ok)
        });

        let mut punch = vec![0xf1, 0x41, 0x00, 0x14];
        punch.extend(b"TEST");
        punch.extend(1u64.to_be_bytes());
        punch.extend(b"ABC\0\0\0\0\0");
        let (tx, mut rx) = mpsc::channel::<Bytes>(4);
        let session = tokio::spawn(async move { pppp(client, cam_addr, punch, 3, tx).await });

        let frame = match time::timeout(Duration::from_secs(5), rx.recv()).await.expect("timeout") {
            Some(f) => f,
            None => panic!("session ended: {:?}", session.await),
        };
        assert_eq!(&frame[..], &jpg[..]);
        let (started, res, ticket_ok) = fake.await.unwrap();
        assert!(started);
        assert_eq!(res, 3);
        assert!(ticket_ok);
        drop(rx);
        session.abort();
    }

    #[tokio::test]
    async fn legacy_stream_is_split_into_frames() {
        let cam = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let client_addr = client.local_addr().unwrap();
        let jpg = jpeg_sample(2500);
        let j2 = jpg.clone();
        let (tx, mut rx) = mpsc::channel::<Bytes>(4);

        let ip: Ipv4Addr = "127.0.0.1".parse().unwrap();
        let cam_port = cam.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let sock = client;
            let mut acc = BytesMut::new();
            let mut buf = vec![0u8; 4096];
            loop {
                let (n, src) = sock.recv_from(&mut buf).await.unwrap();
                assert_eq!(src.port(), cam_port);
                acc.extend_from_slice(&buf[8..n]);
                if let Some(f) = jpeg::take_frame(&mut acc) {
                    let _ = tx.send(f).await;
                    return;
                }
            }
        });
        for chunk in j2.chunks(900) {
            let mut d = vec![0u8; 8];
            d.extend(chunk);
            cam.send_to(&d, client_addr).await.unwrap();
        }
        let f = time::timeout(Duration::from_secs(3), rx.recv()).await.unwrap().unwrap();
        assert_eq!(&f[..], &jpg[..]);
        let _ = (task.await, ip);
    }
}
