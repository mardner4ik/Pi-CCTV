use bytes::Bytes;
use tokio::sync::mpsc;
use std::process::Stdio;
use tokio::io::AsyncReadExt;

pub struct MjpegStream {
    pub rx: mpsc::Receiver<Bytes>,
}

pub fn start_camera_stream(device_path: String) -> MjpegStream {
    let (tx, rx) = mpsc::channel::<Bytes>(16);

    tokio::spawn(async move {
        loop {
            let result = stream_from_device(&device_path, tx.clone()).await;
            if let Err(e) = result {
                tracing::warn!("Помилка потоку {}: {}. Перезапуск...", device_path, e);
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            if tx.is_closed() {
                break;
            }
        }
    });

    MjpegStream { rx }
}

async fn stream_from_device(
    device: &str,
    tx: mpsc::Sender<Bytes>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut child = tokio::process::Command::new("ffmpeg")
        .args([
            "-f", "v4l2",
            "-framerate", "15",
            "-video_size", "1280x720",
            "-i", device,
            "-vf", "scale=1280:720",
            "-q:v", "5",
            "-f", "mjpeg",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;

    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let mut buf = vec![0u8; 65536];
    let mut frame_buf: Vec<u8> = Vec::with_capacity(65536);

    loop {
        let n = stdout.read(&mut buf).await?;
        if n == 0 {
            break;
        }

        frame_buf.extend_from_slice(&buf[..n]);

        while let Some(frame) = extract_jpeg_frame(&mut frame_buf) {
            if tx.send(Bytes::from(frame)).await.is_err() {
                return Ok(());
            }
        }
    }

    Ok(())
}

fn extract_jpeg_frame(buf: &mut Vec<u8>) -> Option<Vec<u8>> {
    let start = find_sequence(buf, &[0xFF, 0xD8])?;
    let search_from = start + 2;
    let end = find_sequence(&buf[search_from..], &[0xFF, 0xD9])
        .map(|i| search_from + i + 2)?;

    let frame = buf[start..end].to_vec();
    buf.drain(..end);
    Some(frame)
}

fn find_sequence(data: &[u8], seq: &[u8]) -> Option<usize> {
    data.windows(seq.len()).position(|w| w == seq)
}