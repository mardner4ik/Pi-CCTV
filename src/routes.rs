use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
};
use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_stream::wrappers::ReceiverStream;

use crate::state::AppState;
use crate::stream::start_camera_stream;

pub async fn index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

pub async fn cameras_list(State(state): State<AppState>) -> impl IntoResponse {
    let cameras = state.cameras.read().await;
    axum::Json(cameras.clone())
}

pub async fn stream_mjpeg(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    let cameras = state.cameras.read().await;
    let camera = cameras.iter().find(|c| c.id == device_id).cloned();
    drop(cameras);

    let camera = match camera {
        Some(c) => c,
        None => {
            return (StatusCode::NOT_FOUND, "Камера не знайдена").into_response();
        }
    };

    let mjpeg = start_camera_stream(camera.device_path.clone());
    let stream = MjpegBodyStream::new(mjpeg.rx);

    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Type",
        "multipart/x-mixed-replace; boundary=frame"
            .parse()
            .unwrap(),
    );
    headers.insert("Cache-Control", "no-cache".parse().unwrap());
    headers.insert("Connection", "keep-alive".parse().unwrap());

    (headers, axum::body::Body::from_stream(stream)).into_response()
}

struct MjpegBodyStream {
    inner: ReceiverStream<Bytes>,
}

impl MjpegBodyStream {
    fn new(rx: tokio::sync::mpsc::Receiver<Bytes>) -> Self {
        Self {
            inner: ReceiverStream::new(rx),
        }
    }
}

impl Stream for MjpegBodyStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(frame)) => {
                let header = format!(
                    "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                    frame.len()
                );
                let mut data = header.into_bytes();
                data.extend_from_slice(&frame);
                data.extend_from_slice(b"\r\n");
                Poll::Ready(Some(Ok(Bytes::from(data))))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}