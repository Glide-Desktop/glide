use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::UNIX_EPOCH;

use glide_theme::plates;

const VERSION: u32 = 1;

const SIDE: usize = 48;

const CHANGED: u32 = 24;

const OPAQUE: u8 = 200;

const CONTRAST: f32 = 0.16;

const MUDDY: f32 = 0.2;

static BUSY: Mutex<Vec<String>> = Mutex::new(Vec::new());

static REBUILT: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Unchanged,
    Rebuilt { darkened: usize, kept: usize },
}

pub fn ensure(theme: &str, roots: Vec<PathBuf>) {
    if !plates::wants(theme) {
        return;
    }
    let Some(dest) = glide_config::data_home().map(|data| data.join("icons")) else {
        return;
    };
    {
        let mut busy = BUSY.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if busy.iter().any(|name| name == theme) {
            return;
        }
        busy.push(theme.to_string());
    }
    let name = theme.to_string();
    let spawned = std::thread::Builder::new().name("glide-icon-plates".into()).spawn(move || {
        let source = roots.iter().map(|root| root.join(&name)).find(|dir| dir.join("index.theme").is_file());
        let built = match source {
            Some(source) => fs::create_dir_all(&dest).and_then(|()| build(&source, &name, &dest)),
            None => Ok(Outcome::Unchanged),
        };
        match built {
            Ok(Outcome::Rebuilt { darkened, kept }) => {
                tracing::info!(theme = %name, darkened, kept, "app icons on dark plates are ready");
                REBUILT.store(true, Ordering::Release);
            }
            Ok(Outcome::Unchanged) => {}
            Err(err) => tracing::warn!(theme = %name, %err, "could not put the app icons on dark plates"),
        }
        forget(&name);
    });
    if let Err(err) = spawned {
        tracing::warn!(%err, "could not start the icon plate thread");
        forget(theme);
    }
}

fn forget(theme: &str) {
    BUSY.lock().unwrap_or_else(std::sync::PoisonError::into_inner).retain(|name| name != theme);
}

#[must_use]
pub fn take_rebuilt() -> bool {
    REBUILT.swap(false, Ordering::AcqRel)
}

pub fn build(source: &Path, theme: &str, dest_root: &Path) -> io::Result<Outcome> {
    let apps = fs::canonicalize(source.join("apps").join("scalable"))?;
    let stamp = stamp_for(&apps)?;
    let dest = dest_root.join(plates::derived(theme));
    if fs::read_to_string(dest.join(plates::STAMP)).is_ok_and(|current| current == stamp) {
        return Ok(Outcome::Unchanged);
    }
    let partial = dest_root.join(format!(".{}.partial", plates::derived(theme)));
    if partial.exists() {
        fs::remove_dir_all(&partial)?;
    }
    let out = partial.join("apps").join("scalable");
    fs::create_dir_all(&out)?;
    let mut darkened = HashSet::new();
    let mut kept = 0;
    let mut links = Vec::new();
    for entry in fs::read_dir(&apps)?.flatten() {
        let name = entry.file_name();
        let Some(file) =
            name.to_str().filter(|file| Path::new(file).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("svg")))
        else {
            continue;
        };
        if entry.file_type()?.is_symlink() {
            links.push(file.to_string());
            continue;
        }
        let Some((light, dark)) =
            fs::read_to_string(entry.path()).ok().and_then(|svg| plates::darken(&svg).map(|dark| (svg, dark)))
        else {
            continue;
        };
        if !legible(light.as_bytes(), dark.as_bytes()) {
            kept += 1;
            continue;
        }
        fs::write(out.join(file), dark)?;
        darkened.insert(file.to_string());
    }
    for link in links {
        let Ok(target) = fs::canonicalize(apps.join(&link)) else {
            continue;
        };
        let Some(file) = target.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if target.parent() == Some(apps.as_path()) && darkened.contains(file) {
            std::os::unix::fs::symlink(file, out.join(&link))?;
        }
    }
    fs::write(partial.join("index.theme"), plates::index_theme(theme))?;
    fs::write(partial.join(plates::STAMP), &stamp)?;
    if dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    fs::rename(&partial, &dest)?;
    Ok(Outcome::Rebuilt { darkened: darkened.len(), kept })
}

fn stamp_for(apps: &Path) -> io::Result<String> {
    let modified = fs::metadata(apps)?.modified()?;
    let seconds = modified.duration_since(UNIX_EPOCH).map(|age| age.as_secs()).unwrap_or_default();
    let count = fs::read_dir(apps)?.count();
    Ok(format!("{VERSION} {} {} {seconds} {count}\n", plates::PLATE, apps.display()))
}

fn distance(px: &[u8], plate: [u8; 3]) -> f32 {
    let squares: f32 = px.iter().zip(plate).map(|(&a, b)| (f32::from(a) - f32::from(b)).powi(2)).sum();
    squares.sqrt() / (255.0 * 3f32.sqrt())
}

#[must_use]
pub fn legible(light: &[u8], dark: &[u8]) -> bool {
    let side = i32::try_from(SIDE).unwrap_or(48);
    let (Some(before), Some(after)) = (glide_paint::render_svg(light, side), glide_paint::render_svg(dark, side))
    else {
        return false;
    };
    if before.len() != SIDE * SIDE * 4 || after.len() != before.len() {
        return false;
    }
    let pixel = |buf: &[u8], at: usize| -> [u8; 4] { [buf[at * 4], buf[at * 4 + 1], buf[at * 4 + 2], buf[at * 4 + 3]] };
    let changed: Vec<bool> = (0..SIDE * SIDE)
        .map(|at| {
            let (a, b) = (pixel(&before, at), pixel(&after, at));
            a.iter().zip(b).map(|(&x, y)| u32::from(x.abs_diff(y))).sum::<u32>() > CHANGED
        })
        .collect();
    let (mut edges, mut muddy) = (0u32, 0u32);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let at = y * SIDE + x;
            let px = pixel(&after, at);
            if changed[at] || px[3] < OPAQUE {
                continue;
            }
            let near = [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)];
            if !near.iter().any(|&(nx, ny)| nx < SIDE && ny < SIDE && changed[ny * SIDE + nx]) {
                continue;
            }
            edges += 1;
            if distance(&px[..3], plates::PLATE_RGB) < CONTRAST {
                muddy += 1;
            }
        }
    }
    f64::from(muddy) < f64::from(MUDDY) * f64::from(edges.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="56" height="56" x="4" y="4" fill="#f2f2f2" rx="13" ry="13"/><circle cx="32" cy="32" r="16" fill="#e00"/></svg>"##;

    fn icon(glyph: &str) -> String {
        ICON.replace("#e00", glyph)
    }

    #[test]
    fn a_glyph_that_stands_out_from_the_dark_plate_is_legible() {
        for glyph in ["#e00", "#fff", "#1eadf4", "gray"] {
            let light = icon(glyph);
            let dark = plates::darken(&light).expect("darkened");
            assert!(legible(light.as_bytes(), dark.as_bytes()), "{glyph}");
        }
    }

    #[test]
    fn a_glyph_close_to_the_dark_plate_is_not() {
        for glyph in ["#4d4d4d", "#333", "#5a5a5a"] {
            let light = icon(glyph);
            let dark = plates::darken(&light).expect("darkened");
            assert!(!legible(light.as_bytes(), dark.as_bytes()), "{glyph}");
        }
    }

    #[test]
    fn a_dark_detail_inside_a_light_glyph_does_not_count() {
        let light =
            ICON.replace("</svg>", r##"<circle cx="32" cy="32" r="6" fill="#222"/></svg>"##).replace("#e00", "#ddd");
        let dark = plates::darken(&light).expect("darkened");
        assert!(legible(light.as_bytes(), dark.as_bytes()));
    }

    #[test]
    #[ignore = "builds from an installed theme: GLIDE_PLATES_SOURCE=<theme dir> GLIDE_PLATES_PREVIEW=<out dir>"]
    fn preview() {
        let source = PathBuf::from(std::env::var_os("GLIDE_PLATES_SOURCE").expect("GLIDE_PLATES_SOURCE"));
        let out = PathBuf::from(std::env::var_os("GLIDE_PLATES_PREVIEW").expect("GLIDE_PLATES_PREVIEW"));
        let theme = source.file_name().and_then(|name| name.to_str()).expect("theme name").to_string();
        fs::create_dir_all(&out).expect("out dir");
        let built = build(&source, &theme, &out).expect("built");
        println!("{built:?}");
    }

    #[test]
    fn building_writes_darkened_icons_their_aliases_and_a_stamp_last() {
        let root = std::env::temp_dir().join(format!("glide-icon-plates-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let light = root.join("Colloid-Light").join("apps").join("scalable");
        fs::create_dir_all(&light).expect("source");
        fs::create_dir_all(root.join("Colloid-Dark").join("apps")).expect("source");
        std::os::unix::fs::symlink("../../Colloid-Light/apps/scalable", root.join("Colloid-Dark/apps/scalable"))
            .expect("link");
        fs::write(light.join("org.gnome.Settings.svg"), ICON).expect("icon");
        fs::write(light.join("blue.svg"), ICON.replace("#f2f2f2", "#1eadf4")).expect("icon");
        fs::write(light.join("muddy.svg"), icon("#444")).expect("icon");
        std::os::unix::fs::symlink("org.gnome.Settings.svg", light.join("preferences-system.svg")).expect("alias");
        std::os::unix::fs::symlink("blue.svg", light.join("blue-alias.svg")).expect("alias");
        std::os::unix::fs::symlink("missing.svg", light.join("dangling.svg")).expect("alias");

        let built = build(&root.join("Colloid-Dark"), "Colloid-Dark", &root).expect("built");
        assert_eq!(built, Outcome::Rebuilt { darkened: 1, kept: 1 });
        let out = root.join("Glide-Colloid-Dark").join("apps").join("scalable");
        assert!(fs::read_to_string(out.join("org.gnome.Settings.svg")).expect("dark").contains(plates::PLATE));
        assert_eq!(
            fs::read_link(out.join("preferences-system.svg")).expect("alias"),
            Path::new("org.gnome.Settings.svg")
        );
        for absent in ["blue.svg", "blue-alias.svg", "dangling.svg", "muddy.svg"] {
            assert!(!out.join(absent).exists(), "{absent}");
        }
        let roots = vec![root.clone()];
        assert_eq!(glide_theme::plates::prefer("Colloid-Dark".into(), &roots), "Glide-Colloid-Dark");
        assert_eq!(build(&root.join("Colloid-Dark"), "Colloid-Dark", &root).expect("again"), Outcome::Unchanged);
        assert!(!root.join(".Glide-Colloid-Dark.partial").exists());
        let _ = fs::remove_dir_all(&root);
    }
}
