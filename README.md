# 🍓 Pi-CCTV / Surveillance System 🦀

A lightweight, high-performance CCTV streaming server built with **Rust** for SBC devices like **Orange Pi Zero 2W** and **Raspberry Pi**.

## 🚀 Overview
This system automatically discovers connected USB cameras and streams video via a modern web interface. It’s designed to be fast, memory-safe, and easy to deploy.

## 🛠 Tech Stack
*   **Language**: ![Rust](https://img.shields.io/badge/rust-%23E32F26.svg?style=flat&logo=rust&logoColor=white) **Rust** (Tokio, Axum)
*   **Backend**: **Axum** for high-performance asynchronous networking
*   **Streaming Engine**: **FFmpeg** for real-time video processing
*   **OS Support**: **DietPi**, Armbian, and other Linux distributions

## 🌟 Key Features
*   🔍 **Auto-Discovery**: Scans `/dev/video*` to find your Logitech or other USB cameras automatically.
*   ⚡ **Low Latency**: Optimized FFmpeg pipes for near real-time streaming.
*   💻 **Web Dashboard**: A minimalist "Raspberry-style" interface to manage your streams.
*   📦 **Static Binary**: Can be compiled as a single file that runs without any dependencies (using `musl`).

## 🔧 Installation & Usage

### 1. Prepare your Orange/Raspberry Pi
Install the necessary system tools:
```bash
sudo apt update && sudo apt install -y ffmpeg v4l-utils
