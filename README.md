# Pi-CCTV / Surveillance System

A lightweight, high-performance CCTV streaming server built with **Rust**. This system automatically discovers connected cameras and streams video via a web interface using **FFmpeg** and **Axum**.

## 🛠 Features
*   **Automatic Camera Discovery**: Automatically scans `/dev/video*` to find compatible devices.
*   **High Performance**: Leveraging Rust's safety and speed for efficient video handling.
*   **Web Interface**: Minimalist dashboard to view streams directly in your browser.
*   **Low Latency**: Uses FFmpeg with optimized arguments for near real-time monitoring.

## 🏗 Tech Stack
*   **Backend**: Rust (Axum, Tokio).
*   **Streaming Engine**: FFmpeg.
*   **Frontend**: HTML/CSS/JavaScript (Vanilla or Minimalist frameworks).

## 🚀 Quick Start (on Raspberry Pi / Orange Pi)

### Prerequisites
Make sure you have `ffmpeg` and `v4l-utils` installed:
```bash
sudo apt update && sudo apt install -y ffmpeg v4l-utils
