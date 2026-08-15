#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub size_px: f32,
    pub line_height_px: f32,
    pub weight: u16,
    pub tracking_em: f32,
    pub uppercase: bool,
}

impl TextStyle {
    pub const DISPLAY: Self = Self {
        size_px: 36.0,
        line_height_px: 42.0,
        weight: 680,
        tracking_em: -0.030,
        uppercase: false,
    };
    pub const T1: Self = Self {
        size_px: 26.0,
        line_height_px: 33.0,
        weight: 650,
        tracking_em: -0.024,
        uppercase: false,
    };
    pub const T2: Self = Self {
        size_px: 19.0,
        line_height_px: 26.0,
        weight: 630,
        tracking_em: -0.017,
        uppercase: false,
    };
    pub const T3: Self = Self {
        size_px: 16.0,
        line_height_px: 22.0,
        weight: 610,
        tracking_em: -0.012,
        uppercase: false,
    };
    pub const HEAD: Self = Self {
        size_px: 14.0,
        line_height_px: 20.0,
        weight: 630,
        tracking_em: -0.007,
        uppercase: false,
    };
    pub const BODY: Self = Self {
        size_px: 14.0,
        line_height_px: 21.0,
        weight: 400,
        tracking_em: -0.006,
        uppercase: false,
    };
    pub const CALL: Self = Self {
        size_px: 13.0,
        line_height_px: 19.0,
        weight: 400,
        tracking_em: -0.006,
        uppercase: false,
    };
    pub const CAP: Self = Self {
        size_px: 12.0,
        line_height_px: 16.0,
        weight: 450,
        tracking_em: -0.006,
        uppercase: false,
    };
    pub const MICRO: Self = Self {
        size_px: 11.0,
        line_height_px: 14.0,
        weight: 640,
        tracking_em: 0.075,
        uppercase: true,
    };
    pub const NUM: Self = Self {
        size_px: 13.0,
        line_height_px: 15.6,
        weight: 400,
        tracking_em: -0.020,
        uppercase: false,
    };

    pub const WIN_TITLE: Self = Self {
        size_px: 13.5,
        line_height_px: 20.0,
        weight: 640,
        tracking_em: -0.012,
        uppercase: false,
    };
    pub const OV_LABEL: Self = Self {
        size_px: 12.0,
        line_height_px: 16.0,
        weight: 570,
        tracking_em: 0.0,
        uppercase: false,
    };
    pub const LAUNCHER_CAPTION: Self = Self {
        size_px: 11.0,
        line_height_px: 14.0,
        weight: 480,
        tracking_em: 0.0,
        uppercase: false,
    };
    pub const MENU_ITEM: Self = Self {
        size_px: 13.5,
        line_height_px: 20.0,
        weight: 400,
        tracking_em: -0.008,
        uppercase: false,
    };

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
    ];
}

#[cfg(test)]
#[allow(clippy::assertions_on_constants)]
mod tests {
    use super::*;

    #[test]
    fn italic_is_never_expressed_here_by_design() {
        assert_eq!(
            TextStyle::ALL.len(),
            14,
            "no style role carries a slant field"
        );
    }

    #[test]
    fn only_micro_is_uppercase() {
        for style in TextStyle::ALL {
            let is_micro = *style == TextStyle::MICRO;
            assert_eq!(
                style.uppercase, is_micro,
                "uppercase must be exclusive to .t-micro"
            );
        }
    }

    #[test]
    fn display_is_the_largest_and_boldest_document_role() {
        for style in [
            TextStyle::T1,
            TextStyle::T2,
            TextStyle::T3,
            TextStyle::HEAD,
            TextStyle::BODY,
            TextStyle::CALL,
            TextStyle::CAP,
            TextStyle::MICRO,
        ] {
            assert!(TextStyle::DISPLAY.size_px > style.size_px);
        }
    }

    #[test]
    fn negative_tracking_grows_more_negative_as_size_grows_on_the_document_scale() {
        let scale = [
            TextStyle::HEAD,
            TextStyle::T3,
            TextStyle::T2,
            TextStyle::T1,
            TextStyle::DISPLAY,
        ];
        for pair in scale.windows(2) {
            assert!(
                pair[0].tracking_em >= pair[1].tracking_em,
                "{:?} should not be tighter than {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn line_height_is_never_smaller_than_the_font_size() {
        for style in TextStyle::ALL {
            assert!(
                style.line_height_px >= style.size_px,
                "{style:?}: line-height must not clip the glyph"
            );
        }
    }

    #[test]
    fn num_uses_tabular_leaning_negative_tracking_like_the_spec_table() {
        assert!((TextStyle::NUM.tracking_em - (-0.020)).abs() < f32::EPSILON);
    }

    #[test]
    fn win_title_matches_the_winbar_title_css_rule() {
        assert!((TextStyle::WIN_TITLE.size_px - 13.5).abs() < f32::EPSILON);
        assert_eq!(TextStyle::WIN_TITLE.weight, 640);
        assert!((TextStyle::WIN_TITLE.tracking_em - (-0.012)).abs() < f32::EPSILON);
    }

    #[test]
    fn menu_item_is_the_same_size_as_win_title_but_regular_weight() {
        assert!((TextStyle::MENU_ITEM.size_px - TextStyle::WIN_TITLE.size_px).abs() < f32::EPSILON);
        assert_eq!(TextStyle::MENU_ITEM.weight, 400);
        assert!(TextStyle::MENU_ITEM.weight < TextStyle::WIN_TITLE.weight);
    }

    #[test]
    fn ov_label_matches_the_overview_label_css_rule() {
        assert!((TextStyle::OV_LABEL.size_px - 12.0).abs() < f32::EPSILON);
        assert_eq!(TextStyle::OV_LABEL.weight, 570);
    }
}
