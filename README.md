<div align="center">

<img src="static/logo.svg" alt="PiCCTV" height="56">

### Lightweight self-hosted CCTV for Orange Pi, Raspberry Pi and PC

USB cameras · cheap **A9 Wi-Fi** cameras · RTSP · HTTP-MJPEG — in one fast web dashboard.

![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust&logoColor=white)
![React](https://img.shields.io/badge/UI-React-61dafb?logo=react&logoColor=black)
![License](https://img.shields.io/badge/license-GPL--3.0-blue)
![Platform](https://img.shields.io/badge/platform-Linux-lightgrey?logo=linux&logoColor=white)

</div>

---

## ✨ Features

| | |
|---|---|
| 🔐 **Password login** | Sign in on the site, then manage everything from the control panel. |
| 🛠️ **Control panel** | Add, edit, test, enable/disable, reconnect and delete cameras without touching config files. |
| 📡 **A9 Wi-Fi cameras** | Native driver (no ffmpeg): iLnk/PPPP (UDP 32108) and legacy UDP 8080, auto-detected. One-click LAN discovery. |
| 🎥 **RTSP and HTTP-MJPEG** | HTTP-MJPEG is passed through without re-encoding. |
| 🔌 **USB cameras** | Auto-detected through V4L2 ioctl. Hardware codecs and helper nodes are filtered out. |
| ⚡ **Low CPU usage** | One process per camera regardless of viewers, starts only on demand, MJPEG passthrough, remembered working mode, low-priority ffmpeg. |
| 🖥️ **Clean UI** | Flat React interface, dark and light themes, mobile friendly, English and Ukrainian. |
| 📸 **Extras** | Snapshots, fullscreen, focus mode, 1/2/3 column layouts. |

## 🚀 Quick start

```bash
sudo apt install ffmpeg
sudo usermod -aG video "$USER"

cargo build --release
PICCTV_PASS=change-me ./target/release/surveillance
```

Open `http://<device-address>:8080` and sign in.

> **Note**
> Without `PICCTV_PASS` the control panel is disabled and viewing is open to everyone who can reach the port.

No camera at hand? Run with `PICCTV_DEMO=1` to add a test pattern.

### 🧩 Run as a systemd service

```bash
sudo install -m755 target/release/surveillance /usr/local/bin/
sudo install -m600 deploy/pi-cctv.env /etc/pi-cctv.env
sudo install -m644 deploy/pi-cctv.service /etc/systemd/system/
sudo systemctl enable --now pi-cctv
journalctl -u pi-cctv -f
```

Camera settings are stored in `PICCTV_DATA` (`/var/lib/pi-cctv/cameras.json`, mode 600).

## 📡 A9 Wi-Fi cameras

1. Power on the camera. It runs as an access point (a network like `A9_…` or `MiniCam…`) or is joined to your router via the official app.
2. Put the Pi **on the same network as the camera**: join the camera's Wi-Fi, or configure the camera to join your router.
3. Open **Control → IP cameras → Find A9**, then press **Add**. Or choose **Add camera → A9 Wi-Fi** and enter the IP.
4. Use **Test** to see a real frame before saving.

**Good to know**

- The camera serves one session at a time, so close the official app first.
- Do not press **Test** for a camera you are currently watching, it can drop the live stream.
- The camera tries to reach vendor cloud servers. You can block its outbound traffic on the router, local streaming does not depend on it.
- The camera password is `admin`.
- Wi-Fi provisioning (joining the camera to your router) is not implemented, use the official app for that.

The protocol is implemented from public write-ups ([DavidVentura/cam-reverse](https://github.com/DavidVentura/cam-reverse), [HoffmannP/A9](https://github.com/HoffmannP/A9)). Firmware differs between batches. If yours does not connect, run with `RUST_LOG=debug` and check the log.

## 🍊 Orange Pi: `/dev/video0..2` do not work

On the Orange Pi Zero 2W, `video0`, `video1` and `video2` usually belong to the hardware video decoder (cedrus) and other helper devices, not to cameras. Earlier versions mistook them for cameras.

PiCCTV now asks the driver via `VIDIOC_QUERYCAP`. A node counts as a camera only if it can capture video, is not an M2M codec, and exposes image formats. Open **Control → USB devices** to see every node and the reason it was rejected. A real USB camera appears there automatically once plugged in.

## ⚙️ Configuration

All settings are environment variables.

| Variable | Default | Description |
|---|---|---|
| `PICCTV_PORT` | `8080` | Port |
| `PICCTV_BIND` | `0.0.0.0` | Listen address |
| `PICCTV_PASS` | none | Login password, enables the control panel |
| `PICCTV_USER` | `admin` | User name for HTTP Basic (VLC, Home Assistant) |
| `PICCTV_DATA` | `picctv-data` | Directory for `cameras.json` |
| `PICCTV_SESSION_HOURS` | `168` | Login session lifetime |
| `PICCTV_RESOLUTION` | `1280x720` | Size for USB cameras |
| `PICCTV_FPS` | `15` | Frames per second (1-60) |
| `PICCTV_QUALITY` | `5` | JPEG quality when transcoding, 2-31 (lower is better) |
| `PICCTV_MODE` | `auto` | USB mode: `auto`, `copy`, `transcode` |
| `PICCTV_NET_WIDTH` | `0` | Downscale RTSP to this width (0 keeps original) |
| `PICCTV_IDLE_SECS` | `10` | How long to keep a stream after the last viewer leaves |
| `PICCTV_MAX_VIEWERS` | `8` | Viewers per camera |
| `PICCTV_THREADS` | `2` | Tokio worker threads |
| `PICCTV_NAMES` | none | USB camera names: `video0=Yard,video2=Garage` |
| `PICCTV_DEMO` | off | Add a test camera |
| `RUST_LOG` | `info` | Log level |

## 🪶 Reducing CPU load

- MJPEG cameras (most USB ones) run in `copy` mode: close to 0% CPU.
- A9 frames are received and served directly, ffmpeg is never started.
- RTSP with H.264 needs decoding. Lower `PICCTV_FPS` and `PICCTV_NET_WIDTH`, and pick a low-resolution substream in the camera.
- With 4 or more cameras the grid requests 10 fps per tile (`/stream/<id>?fps=10`), and full rate in focus mode.

## 🌐 HTTP API

| Endpoint | Description |
|---|---|
| `GET /` | Web interface |
| `POST /api/login`, `POST /api/logout`, `GET /api/session` | Session |
| `GET /api/cameras` | Camera list and status |
| `GET /stream/<id>[?fps=N]` | MJPEG stream (`multipart/x-mixed-replace`) |
| `GET /snapshot/<id>` | JPEG (`?download=1` to save) |
| `GET /healthz` | Liveness check (no auth) |
| `/api/admin/*` | Camera management (needs login and `PICCTV_PASS`) |

Streams and snapshots also accept HTTP Basic, for example `http://admin:password@pi:8080/stream/ip1`.

## 🔒 Security

- Session cookie is `HttpOnly` and `SameSite=Strict`, login attempts are rate limited per IP.
- Passwords and HTTP Basic travel in clear text. Use a trusted LAN, a VPN, or a TLS reverse proxy (`PICCTV_BIND=127.0.0.1`).
- Camera URLs with credentials are stored in `cameras.json` (mode 600). ffmpeg is restricted to rtsp/http protocols, `file://` is blocked.

## 🧑‍💻 Development

**Backend (Rust)**

```bash
cargo build --release
cargo test
```

**Web UI (React + Vite)**

The source lives in `web/`. The built result is committed to `static/` and embedded into the binary, so `cargo build` does not need Node.js.

```bash
cd web
npm install
npm run dev
npm run build
```

`npm run build` overwrites `static/`, rebuild the server afterwards. `npm run dev` starts a dev server proxying to `http://localhost:8080` (another backend: `PICCTV_BACKEND=http://host:port npm run dev`).

```
web/src
├── pages        Login, Live, Admin
├── components   Header, Modal, Logo, Icon, Toasts, live/, admin/
├── hooks        session, polling, page visibility, persistent state
├── api          fetch client
├── i18n         English and Ukrainian dictionaries
└── styles       base, layout, live, admin
```

## 📄 License

GPL-3.0, see [LICENSE](LICENSE).
