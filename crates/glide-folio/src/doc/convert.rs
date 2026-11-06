use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::{Format, OpenError};

#[must_use]
pub fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("glide/folio")
}

#[must_use]
pub fn cache_key(path: &Path) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    if let Ok(meta) = std::fs::metadata(path) {
        meta.len().hash(&mut hasher);
        meta.modified().ok().hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

#[must_use]
pub fn find(names: &[&str]) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    names
        .iter()
        .find_map(|name| std::env::split_paths(&paths).map(|dir| dir.join(name)).find(|candidate| candidate.is_file()))
}

#[must_use]
pub fn tool_for(format: Format) -> Option<(&'static [&'static str], &'static str)> {
    Some(match format {
        Format::PostScript => (&["gs"], "ghostscript"),
        Format::Djvu => (&["ddjvu"], "djvulibre"),
        Format::Xps => (&["xpstopdf"], "libgxps"),
        Format::Office => (&["soffice", "libreoffice"], "libreoffice-fresh"),
        _ => return None,
    })
}

pub fn to_pdf(path: &Path, format: Format) -> Result<PathBuf, OpenError> {
    let (names, package) = tool_for(format).ok_or(OpenError::Unsupported)?;
    let out_dir = cache_dir().join("converted");
    let out = out_dir.join(format!("{}.pdf", cache_key(path)));
    if out.is_file() {
        return Ok(out);
    }
    let tool =
        find(names).ok_or_else(|| OpenError::Missing { tool: names[0].to_string(), package: package.to_string() })?;
    std::fs::create_dir_all(&out_dir).map_err(|e| OpenError::Io(e.to_string()))?;
    let partial = out.with_extension("part.pdf");
    let mut command = Command::new(&tool);
    match format {
        Format::PostScript => {
            command
                .args(["-q", "-dNOPAUSE", "-dBATCH", "-dSAFER", "-sDEVICE=pdfwrite"])
                .arg(format!("-sOutputFile={}", partial.display()))
                .arg(path);
        }
        Format::Djvu => {
            command.arg("-format=pdf").arg(path).arg(&partial);
        }
        Format::Xps => {
            command.arg(path).arg(&partial);
        }
        _ => {
            let work = out_dir.join(cache_key(path));
            std::fs::create_dir_all(&work).map_err(|e| OpenError::Io(e.to_string()))?;
            let ok = Command::new(&tool)
                .args(["--headless", "--norestore", "--convert-to", "pdf", "--outdir"])
                .arg(&work)
                .arg(path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            let produced = std::fs::read_dir(&work).ok().and_then(|entries| {
                entries.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "pdf"))
            });
            let result = match (ok, produced) {
                (true, Some(produced)) => {
                    std::fs::rename(produced, &out).map(|()| out.clone()).map_err(|e| OpenError::Io(e.to_string()))
                }
                _ => Err(OpenError::Damaged),
            };
            let _ = std::fs::remove_dir_all(&work);
            return result;
        }
    }
    let ok = command.stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
    if !ok || !partial.is_file() {
        let _ = std::fs::remove_file(&partial);
        return Err(OpenError::Damaged);
    }
    std::fs::rename(&partial, &out).map_err(|e| OpenError::Io(e.to_string()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cache_key_follows_the_file() {
        let dir = std::env::temp_dir().join(format!("glide-folio-key-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.ps");
        std::fs::write(&file, b"%!PS").unwrap();
        let first = cache_key(&file);
        assert_eq!(first, cache_key(&file));
        std::fs::write(&file, b"%!PS-Adobe-3.0").unwrap();
        assert_ne!(first, cache_key(&file));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn every_converted_format_names_its_package() {
        for format in [Format::PostScript, Format::Djvu, Format::Xps, Format::Office] {
            assert!(tool_for(format).is_some_and(|(tools, package)| !tools.is_empty() && !package.is_empty()));
        }
        assert!(tool_for(Format::Pdf).is_none());
    }
}
