use std::sync::atomic::{AtomicU64, Ordering};

static REVISION: AtomicU64 = AtomicU64::new(0);

pub const TABULAR: iced::Font =
    iced::Font { family: iced::font::Family::Name(glide_paint::TABULAR_FAMILY), ..iced::Font::DEFAULT };

pub fn use_ui_family(requested: Option<&str>) -> String {
    use iced::advanced::graphics::text::cosmic_text::fontdb::{Family, Query, Style, Weight};
    let mut system =
        iced::advanced::graphics::text::font_system().write().unwrap_or_else(std::sync::PoisonError::into_inner);
    let db = system.raw().db_mut();
    let family = glide_theme::font::ui_family(
        requested,
        db.faces().flat_map(|face| face.families.iter().map(|(name, _)| name.as_str())),
    );
    db.set_sans_serif_family(family.clone());
    let mut weights: Vec<_> = glide_theme::TextStyle::ALL.iter().map(|style| Weight(style.weight)).collect();
    weights.extend([Weight::NORMAL, Weight::MEDIUM, Weight::SEMIBOLD, Weight::BOLD]);
    weights.sort_unstable();
    weights.dedup();
    for weight in weights {
        let exact = db.faces().any(|face| {
            face.weight == weight
                && face.style == Style::Normal
                && face.families.iter().any(|(name, _)| *name == family)
        });
        let query = Query { families: &[Family::Name(&family)], weight, ..Query::default() };
        let nearest = db.query(&query).and_then(|id| db.face(id)).cloned();
        if let Some(mut face) = nearest.filter(|_| !exact) {
            face.weight = weight;
            db.push_face_info(face);
        }
    }
    REVISION.fetch_add(1, Ordering::Release);
    family
}

#[must_use]
pub fn installed_monospace(name: &str) -> Option<iced::Font> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let mut system =
        iced::advanced::graphics::text::font_system().write().unwrap_or_else(std::sync::PoisonError::into_inner);
    let found =
        system.raw().db().faces().any(|face| face.monospaced && face.families.iter().any(|(family, _)| family == name));
    found.then(|| iced::Font::with_name(Box::leak(name.to_string().into_boxed_str())))
}

#[must_use]
pub fn revision() -> u64 {
    REVISION.load(Ordering::Acquire)
}
