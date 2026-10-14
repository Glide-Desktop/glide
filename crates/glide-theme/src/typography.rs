#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub size_px: f32,
    pub line_height_px: f32,
    pub weight: u16,
    pub tracking_em: f32,
    pub uppercase: bool,
    pub tabular: bool,
}

impl TextStyle {
    pub const DISPLAY: Self = Self {
        size_px: 36.0,
        line_height_px: 42.0,
        weight: 680,
        tracking_em: -0.030,
        uppercase: false,
        tabular: false,
    };
    pub const T1: Self = Self {
        size_px: 26.0,
        line_height_px: 33.0,
        weight: 650,
        tracking_em: -0.024,
        uppercase: false,
        tabular: false,
    };
    pub const T2: Self = Self {
        size_px: 19.0,
        line_height_px: 26.0,
        weight: 630,
        tracking_em: -0.017,
        uppercase: false,
        tabular: false,
    };
    pub const T3: Self = Self {
        size_px: 16.0,
        line_height_px: 22.0,
        weight: 610,
        tracking_em: -0.012,
        uppercase: false,
        tabular: false,
    };
    pub const HEAD: Self = Self {
        size_px: 14.0,
        line_height_px: 20.0,
        weight: 630,
        tracking_em: -0.007,
        uppercase: false,
        tabular: false,
    };
    pub const BODY: Self = Self {
        size_px: 14.0,
        line_height_px: 21.0,
        weight: 400,
        tracking_em: -0.006,
        uppercase: false,
        tabular: false,
    };
    pub const CALL: Self =
        Self { size_px: 13.0, line_height_px: 19.0, weight: 400, tracking_em: 0.0, uppercase: false, tabular: false };
    pub const CAP: Self =
        Self { size_px: 12.0, line_height_px: 16.0, weight: 450, tracking_em: 0.0, uppercase: false, tabular: false };
    pub const MICRO: Self =
        Self { size_px: 11.0, line_height_px: 14.0, weight: 640, tracking_em: 0.075, uppercase: true, tabular: false };
    pub const NUM: Self =
        Self { size_px: 13.0, line_height_px: 15.6, weight: 400, tracking_em: -0.020, uppercase: false, tabular: true };

    pub const WIN_TITLE: Self = Self {
        size_px: 13.5,
        line_height_px: 20.0,
        weight: 640,
        tracking_em: -0.012,
        uppercase: false,
        tabular: false,
    };
    pub const OV_LABEL: Self =
        Self { size_px: 12.0, line_height_px: 16.0, weight: 570, tracking_em: 0.0, uppercase: false, tabular: false };
    pub const LAUNCHER_CAPTION: Self =
        Self { size_px: 13.0, line_height_px: 18.0, weight: 480, tracking_em: 0.0, uppercase: false, tabular: false };
    pub const TOPBAR: Self =
        Self { size_px: 12.5, line_height_px: 16.0, weight: 520, tracking_em: 0.0, uppercase: false, tabular: false };
    pub const LAUNCHER_CHIP: Self =
        Self { size_px: 12.0, line_height_px: 16.0, weight: 560, tracking_em: 0.0, uppercase: false, tabular: false };
    pub const MENU_ITEM: Self = Self {
        size_px: 13.5,
        line_height_px: 20.0,
        weight: 400,
        tracking_em: -0.008,
        uppercase: false,
        tabular: false,
    };
    pub const FALLBACK_INITIAL: Self =
        Self { size_px: 34.0, line_height_px: 40.0, weight: 560, tracking_em: 0.0, uppercase: false, tabular: false };

    pub const ALL: &'static [Self] = &[
        Self::DISPLAY,
        Self::T1,
        Self::T2,
        Self::T3,
        Self::HEAD,
        Self::BODY,
        Self::CALL,
        Self::CAP,
        Self::MICRO,
        Self::NUM,
        Self::MENU_ITEM,
        Self::WIN_TITLE,
        Self::OV_LABEL,
        Self::LAUNCHER_CAPTION,
        Self::LAUNCHER_CHIP,
        Self::TOPBAR,
        Self::FALLBACK_INITIAL,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_micro_is_uppercase() {
        for style in TextStyle::ALL {
            assert_eq!(style.uppercase, *style == TextStyle::MICRO, "{style:?}");
            assert_eq!(style.tabular, *style == TextStyle::NUM, "{style:?}");
            assert!(style.line_height_px >= style.size_px, "{style:?}");
        }
    }

    #[test]
    fn larger_document_roles_track_tighter() {
        let scale = [TextStyle::HEAD, TextStyle::T3, TextStyle::T2, TextStyle::T1, TextStyle::DISPLAY];
        assert!(
            scale
                .windows(2)
                .all(|pair| pair[0].tracking_em >= pair[1].tracking_em && pair[0].size_px <= pair[1].size_px)
        );
    }
}
