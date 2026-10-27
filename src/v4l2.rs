use std::{
    fs::{File, OpenOptions},
    io,
    mem::size_of,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
};

const VIDIOC_QUERYCAP: u64 = 0x8068_5600;
const VIDIOC_ENUM_FMT: u64 = 0xC040_5602;

const CAP_VIDEO_CAPTURE: u32 = 0x0000_0001;
const CAP_VIDEO_OUTPUT: u32 = 0x0000_0002;
const CAP_VIDEO_M2M_MPLANE: u32 = 0x0000_4000;
const CAP_VIDEO_M2M: u32 = 0x0000_8000;
const CAP_VIDEO_CAPTURE_MPLANE: u32 = 0x0000_1000;
const CAP_META_CAPTURE: u32 = 0x0080_0000;
const CAP_DEVICE_CAPS: u32 = 0x8000_0000;

const BUF_TYPE_CAPTURE: u32 = 1;
const BUF_TYPE_CAPTURE_MPLANE: u32 = 9;

#[repr(C)]
struct Capability {
    driver: [u8; 16],
    card: [u8; 32],
    bus_info: [u8; 32],
    version: u32,
    capabilities: u32,
    device_caps: u32,
    reserved: [u32; 3],
}

#[repr(C)]
struct FmtDesc {
    index: u32,
    buf_type: u32,
    flags: u32,
    description: [u8; 32],
    pixelformat: u32,
    mbus_code: u32,
    reserved: [u32; 3],
}

const _: () = assert!(size_of::<Capability>() == 104);
const _: () = assert!(size_of::<FmtDesc>() == 64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub driver: String,
    pub card: String,
    pub bus: String,
    pub caps: u32,
    pub formats: Vec<String>,
}

#[derive(Debug)]
pub enum QueryError {
    NotV4l2,
    Transient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reject {
    Metadata,
    Hardware,
    OutputOnly,
    NoFormats,
    NoCapture,
}

impl Reject {
    pub fn code(self) -> &'static str {
        match self {
            Reject::Metadata => "metadata",
            Reject::Hardware => "hardware",
            Reject::OutputOnly => "output",
            Reject::NoFormats => "no_formats",
            Reject::NoCapture => "no_capture",
        }
    }
}

impl DeviceInfo {
    fn effective(&self) -> u32 {
        self.caps
    }

    pub fn classify(&self) -> Result<(), Reject> {
        let caps = self.effective();
        let capture = caps & (CAP_VIDEO_CAPTURE | CAP_VIDEO_CAPTURE_MPLANE) != 0;
        let m2m = caps & (CAP_VIDEO_M2M | CAP_VIDEO_M2M_MPLANE) != 0;
        if m2m {
            return Err(Reject::Hardware);
        }
        if !capture {
            if caps & CAP_META_CAPTURE != 0 {
                return Err(Reject::Metadata);
            }
            if caps & CAP_VIDEO_OUTPUT != 0 {
                return Err(Reject::OutputOnly);
            }
            return Err(Reject::NoCapture);
        }
        if self.formats.is_empty() {
            return Err(Reject::NoFormats);
        }
        Ok(())
    }
}

fn cstr(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

fn fourcc(v: u32) -> String {
    v.to_le_bytes()
        .iter()
        .map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '?' })
        .collect::<String>()
        .trim()
        .to_string()
}

fn open(path: &str) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .or_else(|_| OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(path))
}

fn enum_formats(file: &File, buf_type: u32) -> Vec<String> {
    let mut out = Vec::new();
    for index in 0..64u32 {
        let mut d = FmtDesc {
            index,
            buf_type,
            flags: 0,
            description: [0; 32],
            pixelformat: 0,
            mbus_code: 0,
            reserved: [0; 3],
        };
        let rc = unsafe { libc::ioctl(file.as_raw_fd(), VIDIOC_ENUM_FMT as _, &mut d as *mut FmtDesc) };
        if rc != 0 {
            break;
        }
        out.push(fourcc(d.pixelformat));
    }
    out
}

pub fn query(path: &str) -> Result<DeviceInfo, QueryError> {
    let file = open(path).map_err(|_| QueryError::Transient)?;
    let mut cap = Capability {
        driver: [0; 16],
        card: [0; 32],
        bus_info: [0; 32],
        version: 0,
        capabilities: 0,
        device_caps: 0,
        reserved: [0; 3],
    };
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), VIDIOC_QUERYCAP as _, &mut cap as *mut Capability) };
    if rc != 0 {
        return match io::Error::last_os_error().raw_os_error() {
            Some(libc::ENOTTY) | Some(libc::EINVAL) | Some(libc::ENODEV) => Err(QueryError::NotV4l2),
            _ => Err(QueryError::Transient),
        };
    }
    let caps = if cap.capabilities & CAP_DEVICE_CAPS != 0 {
        cap.device_caps
    } else {
        cap.capabilities
    };
    let buf_type = if caps & CAP_VIDEO_CAPTURE_MPLANE != 0 && caps & CAP_VIDEO_CAPTURE == 0 {
        BUF_TYPE_CAPTURE_MPLANE
    } else {
        BUF_TYPE_CAPTURE
    };
    let formats = if caps & (CAP_VIDEO_CAPTURE | CAP_VIDEO_CAPTURE_MPLANE) != 0
        && caps & (CAP_VIDEO_M2M | CAP_VIDEO_M2M_MPLANE) == 0
    {
        enum_formats(&file, buf_type)
    } else {
        Vec::new()
    };
    Ok(DeviceInfo {
        driver: cstr(&cap.driver),
        card: cstr(&cap.card),
        bus: cstr(&cap.bus_info),
        caps,
        formats,
    })
}

pub fn has_mjpeg(formats: &[String]) -> bool {
    formats.iter().any(|f| f == "MJPG" || f == "JPEG")
}

pub fn raw_input(formats: &[String]) -> Option<&'static str> {
    const MAP: [(&str, &str); 6] = [
        ("YUYV", "yuyv422"),
        ("NV12", "nv12"),
        ("YU12", "yuv420p"),
        ("UYVY", "uyvy422"),
        ("RGB3", "rgb24"),
        ("GREY", "gray"),
    ];
    MAP.iter()
        .find(|(f, _)| formats.iter().any(|x| x == f))
        .map(|(_, n)| *n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(caps: u32, formats: &[&str]) -> DeviceInfo {
        DeviceInfo {
            driver: "x".into(),
            card: "x".into(),
            bus: "x".into(),
            caps,
            formats: formats.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn webcam_is_camera() {
        assert_eq!(info(0x0420_0001, &["MJPG", "YUYV"]).classify(), Ok(()));
    }

    #[test]
    fn metadata_node_rejected() {
        assert_eq!(info(0x04a0_0000, &[]).classify(), Err(Reject::Metadata));
    }

    #[test]
    fn hardware_codec_rejected() {
        assert_eq!(info(0x0420_4000, &[]).classify(), Err(Reject::Hardware));
        assert_eq!(info(0x0420_8000, &["NV12"]).classify(), Err(Reject::Hardware));
    }

    #[test]
    fn capture_without_formats_rejected() {
        assert_eq!(info(0x0420_0001, &[]).classify(), Err(Reject::NoFormats));
    }

    #[test]
    fn format_helpers() {
        let f = vec!["YUYV".to_string(), "MJPG".to_string()];
        assert!(has_mjpeg(&f));
        assert_eq!(raw_input(&f), Some("yuyv422"));
        assert_eq!(raw_input(&["H264".to_string()]), None);
        assert_eq!(fourcc(0x4750_4a4d), "MJPG");
    }
}
