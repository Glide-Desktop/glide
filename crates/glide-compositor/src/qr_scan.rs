use std::os::unix::net::UnixStream;

use glide_ipc::keychain::Hidden;
use smithay::output::Output;

use crate::state::GlideState;

#[derive(Debug)]
pub struct Scan {
    pub output: Output,
    pub waiters: Vec<UnixStream>,
}

#[must_use]
pub fn decode(pixels: &[u8], width: usize, height: usize) -> Vec<Hidden> {
    if width == 0 || height == 0 || pixels.len() < width * height * 4 {
        return Vec::new();
    }
    let mut image = rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| {
        let index = (y * width + x) * 4;
        let (blue, green, red) = (u32::from(pixels[index]), u32::from(pixels[index + 1]), u32::from(pixels[index + 2]));
        u8::try_from((red * 77 + green * 150 + blue * 29) >> 8).unwrap_or(u8::MAX)
    });
    let mut codes = Vec::new();
    for grid in image.detect_grids() {
        if let Ok((_, content)) = grid.decode()
            && !codes.iter().any(|known: &Hidden| known.expose() == content)
        {
            codes.push(Hidden::new(content));
        }
    }
    codes
}

impl GlideState {
    pub fn start_qr_scan(&mut self, waiter: UnixStream) {
        if self.screen_locked() {
            crate::ipc::reply(&waiter, &glide_ipc::Response::Error { message: "the screen is locked".into() });
            return;
        }
        if let Some(scan) = self.session.qr_scan.as_mut() {
            scan.waiters.push(waiter);
            return;
        }
        let pointer = self.seat.get_pointer().map(|pointer| pointer.current_location()).unwrap_or_default();
        let outputs: Vec<_> = self
            .space
            .outputs()
            .filter_map(|output| Some((output.clone(), self.space.output_geometry(output)?)))
            .collect();
        let Some(output) = crate::screenshot_ui::target_output(&outputs, pointer) else {
            crate::ipc::reply(&waiter, &glide_ipc::Response::Error { message: "no output to scan".into() });
            return;
        };
        self.pending_screenshot = Some(output.clone());
        self.session.qr_scan = Some(Scan { output, waiters: vec![waiter] });
        self.queue_redraw();
    }
}

pub fn deliver(scan: Scan, pixels: Vec<u8>, size: (i32, i32)) {
    let spawned = std::thread::Builder::new().name("glide-qr-scan".into()).spawn(move || {
        let width = usize::try_from(size.0).unwrap_or(0);
        let height = usize::try_from(size.1).unwrap_or(0);
        let codes = decode(&pixels, width, height);
        tracing::info!(found = codes.len(), "scanned the screen for QR codes");
        let answer = glide_ipc::Response::QrCodes { codes };
        for waiter in &scan.waiters {
            crate::ipc::reply(waiter, &answer);
        }
    });
    if let Err(err) = spawned {
        tracing::warn!(%err, "could not start the QR scan");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(text: &str, module: usize) -> (Vec<u8>, usize) {
        let code = qrcode::QrCode::new(text.as_bytes()).expect("qr");
        let modules = code.width();
        let quiet = 4;
        let side = (modules + quiet * 2) * module + 40;
        let mut pixels = vec![255_u8; side * side * 4];
        let colors = code.to_colors();
        for (index, color) in colors.iter().enumerate() {
            if *color != qrcode::Color::Dark {
                continue;
            }
            let (cx, cy) = (index % modules + quiet, index / modules + quiet);
            for y in cy * module..(cy + 1) * module {
                for x in cx * module..(cx + 1) * module {
                    let at = ((y + 20) * side + x + 20) * 4;
                    pixels[at..at + 3].copy_from_slice(&[0, 0, 0]);
                }
            }
        }
        (pixels, side)
    }

    #[test]
    fn a_code_on_the_screen_is_read_back() {
        let uri = "otpauth://totp/GitHub:artem?secret=JBSWY3DPEHPK3PXP&issuer=GitHub";
        let (pixels, side) = rendered(uri, 6);
        let codes = decode(&pixels, side, side);
        assert_eq!(codes.iter().map(Hidden::expose).collect::<Vec<_>>(), [uri]);
    }

    #[test]
    fn a_small_code_in_a_whole_frame_is_found() {
        let uri = "otpauth://totp/Example:me?secret=GEZDGNBVGY3TQOJQ&issuer=Example&digits=8";
        let (code, side) = rendered(uri, 4);
        let (width, height) = (2560, 1600);
        let mut frame = vec![0xf4_u8; width * height * 4];
        for y in 0..side {
            let from = y * side * 4;
            let to = ((y + 700) * width + 1900) * 4;
            frame[to..to + side * 4].copy_from_slice(&code[from..from + side * 4]);
        }
        let codes = decode(&frame, width, height);
        assert_eq!(codes.iter().map(Hidden::expose).collect::<Vec<_>>(), [uri]);
    }

    #[test]
    fn a_blank_or_short_frame_finds_nothing() {
        assert!(decode(&vec![255; 64 * 64 * 4], 64, 64).is_empty());
        assert!(decode(&[0; 12], 64, 64).is_empty());
    }

    #[test]
    fn found_codes_never_reach_the_debug_output() {
        let answer = glide_ipc::Response::QrCodes { codes: vec![Hidden::new("otpauth://totp/x?secret=ABC".into())] };
        assert!(!format!("{answer:?}").contains("ABC"));
    }
}
