use std::collections::HashMap;

use glide_paint::TextAlign;
use glide_shell::raster::{Canvas, ShellRenderer};
use glide_theme::{TextStyle, motion};
use glide_ui::Reveal;
use smithay::{
    backend::renderer::element::{
        Kind,
        memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
    },
    utils::{Logical, Point, Rectangle, Scale, Size},
};

use super::{Topbar, TopbarTheme};
use crate::i18n::{Key, t};
use crate::polkit::Face;

pub(super) const TOP: i32 = 44;
pub(super) const RIGHT: i32 = 16;
const TIP_HEIGHT: i32 = 30;
const PAD_X: i32 = glide_theme::spacing::SP3;
const GAP: i32 = 6;
const RADIUS: f32 = glide_theme::shape::R2;
const TITLE: TextStyle = TextStyle { weight: 600, ..TextStyle::CAP };
const BODY: TextStyle = TextStyle::CAP;

#[derive(Debug, Default)]
pub(super) struct FaceTip {
    reveal: Reveal,
    buffers: HashMap<i32, (u64, i32, MemoryRenderBuffer)>,
    shadows: HashMap<(i32, i32, bool), MemoryRenderBuffer>,
}

impl FaceTip {
    pub(super) fn is_animating(&self) -> bool {
        self.reveal.is_animating(motion::t2())
    }
}

fn text_w(painter: &crate::paint::Painter, text: &str, style: &TextStyle) -> i32 {
    painter.text_width_styled(text, style).ceil() as i32
}

#[must_use]
pub(super) fn width(painter: &crate::paint::Painter) -> i32 {
    PAD_X
        + text_w(painter, t(Key::TbFaceTipTitle), &TITLE)
        + GAP
        + text_w(painter, t(Key::TbFaceTipBody), &BODY)
        + PAD_X
}

fn canvas(painter: &crate::paint::Painter, theme: &TopbarTheme, width: i32, scale: i32) -> Option<Canvas<'static>> {
    let mut c = Canvas::popover((width, TIP_HEIGHT), scale, RADIUS, theme.popover_bg, theme.border)?;
    let title = t(Key::TbFaceTipTitle);
    let split = PAD_X + text_w(painter, title, &TITLE);
    c.label(painter, (PAD_X, split + 1), TextAlign::Left, title, theme.label, &TITLE);
    c.label(painter, (split + GAP, width - PAD_X + 1), TextAlign::Left, t(Key::TbFaceTipBody), theme.muted, &BODY);
    Some(c)
}

fn draw(painter: &crate::paint::Painter, theme: &TopbarTheme, width: i32, scale: i32) -> Option<MemoryRenderBuffer> {
    canvas(painter, theme, width, scale).map(|c| c.finish())
}

#[must_use]
pub(super) fn wanted(topbar: &Topbar) -> bool {
    topbar.polkit_shown() && topbar.polkit_face() == Some(Face::Looking)
}

fn push_alpha<R: ShellRenderer>(
    renderer: &mut R, buffer: &MemoryRenderBuffer, position: Point<i32, Logical>, alpha: f32, scale: Scale<f64>,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) {
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        position.to_f64().to_physical(scale),
        buffer,
        Some(alpha.min(1.0)),
        None,
        None,
        Kind::Unspecified,
    ) {
        elements.push(element);
    }
}

pub(super) fn push<R: ShellRenderer>(
    topbar: &mut Topbar, renderer: &mut R, painter: &crate::paint::Painter, output: Rectangle<i32, Logical>,
    (scale, output_scale): (Scale<f64>, i32), elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) {
    let duration = motion::t2();
    if wanted(topbar) {
        topbar.face_tip.reveal.open(duration);
    } else {
        topbar.face_tip.reveal.close(duration);
    }
    topbar.face_tip.reveal.settle(duration);
    let progress = topbar.face_tip.reveal.value(duration);
    let alpha = motion::assist(progress) as f32;
    if alpha <= 0.0 {
        return;
    }
    let output_scale = super::scale_or_one(output_scale);
    let stamp = glide_theme::Revision::new()
        .u64(topbar.theme.revision())
        .u64(crate::paint::painter_generation())
        .str(t(Key::TbFaceTipBody))
        .finish();
    let fresh = topbar.face_tip.buffers.get(&output_scale).is_some_and(|(at, _, _)| *at == stamp);
    if !fresh {
        let width = width(painter);
        if let Some(buffer) = draw(painter, &topbar.theme, width, output_scale) {
            topbar.face_tip.buffers.insert(output_scale, (stamp, width, buffer));
        }
    }
    let Some((_, width, buffer)) = topbar.face_tip.buffers.get(&output_scale) else {
        return;
    };
    let width = *width;
    let rise = ((1.0 - progress) * f64::from(glide_theme::component::POPOVER_RISE)).round() as i32;
    let position = Point::from((output.loc.x + output.size.w - RIGHT - width, output.loc.y + TOP - rise));
    push_alpha(renderer, buffer, position, alpha, scale, elements);

    let dark = topbar.theme.dark;
    let key = (width, output_scale, dark);
    if let std::collections::hash_map::Entry::Vacant(entry) = topbar.face_tip.shadows.entry(key)
        && let Some(shadow) = crate::popup_menu::draw_elevation(
            Size::from((width, TIP_HEIGHT)),
            RADIUS,
            output_scale,
            glide_theme::elevation::Level::El2,
            if dark { glide_theme::Appearance::Dark } else { glide_theme::Appearance::Light },
        )
    {
        entry.insert(shadow);
    }
    if let Some(shadow) = topbar.face_tip.shadows.get(&key) {
        let margin = crate::popup_menu::shadow_margin();
        push_alpha(renderer, shadow, Point::from((position.x - margin, position.y - margin)), alpha, scale, elements);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polkit::{Hint, Motion, Stage, View};

    fn view(face: Option<Face>) -> View {
        View {
            stage: Stage::Ask,
            app: None,
            icon: None,
            message: String::new(),
            user: "artem".into(),
            display: "Артем".into(),
            action: "org.example".into(),
            program: None,
            bullets: 0,
            shown: None,
            details: false,
            shield: false,
            hint: Hint::Keys,
            note: None,
            ready: false,
            face,
        }
    }

    #[test]
    fn the_camera_tip_shows_only_while_the_island_looks_for_a_face() {
        let mut topbar = Topbar::default();
        assert!(!wanted(&topbar));
        topbar.set_polkit(Some(view(None)), Motion::default());
        assert!(!wanted(&topbar), "a password-only prompt has no camera tip");
        topbar.set_polkit(Some(view(Some(Face::Looking))), Motion::default());
        assert_eq!(wanted(&topbar), topbar.polkit_shown(), "the tip follows the face chip while the prompt shows");
        topbar.set_polkit(Some(view(Some(Face::Missed))), Motion::default());
        assert!(!wanted(&topbar), "a miss turns the camera off, so the tip leaves");
        topbar.set_polkit(None, Motion::default());
        assert!(!wanted(&topbar));
    }

    #[test]
    #[ignore = "writes a preview PNG to $GLIDE_FACE_TIP_PREVIEW"]
    fn preview() {
        let Some(dir) = std::env::var_os("GLIDE_FACE_TIP_PREVIEW") else {
            return;
        };
        let painter = crate::paint::Painter::new(14.0, None);
        for appearance in [glide_theme::Appearance::Light, glide_theme::Appearance::Dark] {
            let theme = TopbarTheme::from_tokens(&glide_theme::Tokens::new(appearance, glide_theme::Accent::Blue));
            let width = width(&painter);
            let c = canvas(&painter, &theme, width, 2).expect("tip");
            let image = glide_shell::image::Image::from_bgra((width * 2) as u32, (TIP_HEIGHT * 2) as u32, c.buf)
                .expect("image");
            let file = format!("face-tip-{appearance:?}.png").to_lowercase();
            std::fs::write(std::path::Path::new(&dir).join(file), image.png().expect("png")).expect("write");
        }
    }
}
