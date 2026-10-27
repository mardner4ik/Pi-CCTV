use bytes::{Buf, Bytes, BytesMut};

#[derive(Debug, PartialEq, Eq)]
pub enum Scan {
    Complete(usize),
    NeedMore,
    Invalid,
}
pub fn scan_frame(d: &[u8]) -> Scan {
    let n = d.len();
    let mut i = 2;
    loop {
        if i >= n {
            return Scan::NeedMore;
        }
        if d[i] != 0xFF {
            return Scan::Invalid;
        }
        while i < n && d[i] == 0xFF {
            i += 1;
        }
        if i >= n {
            return Scan::NeedMore;
        }
        let m = d[i];
        i += 1;
        match m {
            0x00 | 0xD8 => return Scan::Invalid,
            0xD9 => return Scan::Complete(i),
            0x01 | 0xD0..=0xD7 => continue,
            _ => {}
        }
        if i + 2 > n {
            return Scan::NeedMore;
        }
        let len = u16::from_be_bytes([d[i], d[i + 1]]) as usize;
        if len < 2 {
            return Scan::Invalid;
        }
        i += len;
        if m == 0xDA {
            loop {
                match d.get(i..).and_then(|s| s.iter().position(|&b| b == 0xFF)) {
                    None => return Scan::NeedMore,
                    Some(p) => i += p,
                }
                if i + 1 >= n {
                    return Scan::NeedMore;
                }
                match d[i + 1] {
                    0x00 | 0xD0..=0xD7 => i += 2,
                    0xFF => i += 1,
                    _ => break,
                }
            }
        }
    }
}
pub fn take_frame(buf: &mut BytesMut) -> Option<Bytes> {
    loop {
        match buf.windows(2).position(|w| w == [0xFF, 0xD8]) {
            Some(p) => buf.advance(p),
            None => {
                let keep = usize::from(buf.last() == Some(&0xFF));
                let drop = buf.len() - keep;
                buf.advance(drop);
                return None;
            }
        }
        match scan_frame(buf) {
            Scan::Complete(n) => return Some(buf.split_to(n).freeze()),
            Scan::NeedMore => return None,
            Scan::Invalid => buf.advance(2),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8];
        let thumb = [0xFF, 0xD8, 1, 2, 3, 0xFF, 0xD9];
        v.extend([0xFF, 0xE0]);
        v.extend(((thumb.len() + 2) as u16).to_be_bytes());
        v.extend(thumb);
        v.extend([0xFF, 0xDB, 0x00, 0x05, 1, 2, 3]);
        v.extend([0xFF, 0xDA, 0x00, 0x04, 0xAA, 0xBB]);
        v.extend([0x11, 0x22, 0xFF, 0x00, 0x33, 0xFF, 0xD0, 0x44, 0xFF, 0x00]);
        v.extend([0xFF, 0xD9]);
        v
    }

    #[test]
    fn complete_frame_ignores_embedded_thumbnail() {
        let s = sample();
        assert_eq!(scan_frame(&s), Scan::Complete(s.len()));
    }

    #[test]
    fn every_split_point_needs_more() {
        let s = sample();
        for cut in 2..s.len() {
            assert_eq!(scan_frame(&s[..cut]), Scan::NeedMore, "cut={cut}");
        }
    }

    #[test]
    fn take_frames_from_stream_with_garbage() {
        let s = sample();
        let mut stream = vec![0x00, 0x13, 0xFF];
        stream.extend(&s);
        stream.extend(&s);
        stream.extend(&s[..10]);

        let mut buf = BytesMut::new();
        let mut got = 0;
        for chunk in stream.chunks(7) {
            buf.extend_from_slice(chunk);
            while let Some(f) = take_frame(&mut buf) {
                assert_eq!(&f[..], &s[..]);
                got += 1;
            }
        }
        assert_eq!(got, 2);
        assert_eq!(buf.len(), 10);
    }

    #[test]
    fn invalid_soi_resyncs() {
        let s = sample();
        let mut stream = vec![0xFF, 0xD8, 0x42, 0x42];
        stream.extend(&s);
        let mut buf = BytesMut::from(&stream[..]);
        let f = take_frame(&mut buf).expect("frame");
        assert_eq!(&f[..], &s[..]);
    }
}
