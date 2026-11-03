#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Icon {
    pub width: i32,
    pub height: i32,
    pub argb: Vec<u8>,
}

impl Icon {
    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.argb.chunks_exact(4).all(|pixel| pixel[0] == 0)
    }
}

fn unpremultiply(channel: u8, alpha: u8) -> u8 {
    if alpha == 0 {
        return 0;
    }
    let value = (u32::from(channel) * 255 + u32::from(alpha) / 2) / u32::from(alpha);
    u8::try_from(value.min(255)).unwrap_or(u8::MAX)
}

#[must_use]
pub fn from_zpixmap(data: &[u8], width: u16, height: u16, depth: u8) -> Option<Icon> {
    let pixels = usize::from(width) * usize::from(height);
    let source = data.get(..pixels * 4)?;
    let mut argb = Vec::with_capacity(pixels * 4);
    for pixel in source.chunks_exact(4) {
        let (blue, green, red) = (pixel[0], pixel[1], pixel[2]);
        let alpha = if depth == 32 { pixel[3] } else { u8::MAX };
        argb.extend_from_slice(&[
            alpha,
            unpremultiply(red, alpha),
            unpremultiply(green, alpha),
            unpremultiply(blue, alpha),
        ]);
    }
    Some(Icon { width: i32::from(width), height: i32::from(height), argb })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_premultiplied_argb_window_becomes_straight_network_order_argb() {
        let data = [0x40, 0x20, 0x10, 0x80, 0, 0, 0, 0];
        let icon = from_zpixmap(&data, 2, 1, 32).expect("enough bytes");
        assert_eq!(icon.argb, [0x80, 0x20, 0x40, 0x80, 0, 0, 0, 0]);
        assert_eq!((icon.width, icon.height), (2, 1));
    }

    #[test]
    fn a_window_without_alpha_is_opaque() {
        let data = [1, 2, 3, 0];
        let icon = from_zpixmap(&data, 1, 1, 24).expect("enough bytes");
        assert_eq!(icon.argb, [0xff, 3, 2, 1]);
        assert!(!icon.is_blank());
    }

    #[test]
    fn a_short_image_is_refused_and_a_clear_one_is_blank() {
        assert_eq!(from_zpixmap(&[0; 7], 2, 1, 32), None);
        let clear = from_zpixmap(&[0; 16], 2, 2, 32).expect("enough bytes");
        assert!(clear.is_blank(), "nothing drawn yet");
    }

    #[test]
    fn unpremultiplying_never_overflows() {
        assert_eq!(unpremultiply(200, 100), 255);
        assert_eq!(unpremultiply(50, 0), 0);
        assert_eq!(unpremultiply(255, 255), 255);
    }
}
