use std::path::Path;

pub const TERMINAL_BUS: &str = "org.glide.Terminal";
pub const TERMINAL_PATH: &str = "/org/glide/Terminal1";
pub const TERMINAL_INTERFACE: &str = "org.glide.Terminal1";

const FILE_MANAGER_BUS: &str = "org.freedesktop.FileManager1";
const FILES_BUS: &str = "org.glide.Files";
const FILE_MANAGER_PATH: &str = "/org/freedesktop/FileManager1";

#[must_use]
pub fn is_glide_terminal(command: &str) -> bool {
    command
        .split_whitespace()
        .next()
        .and_then(|program| Path::new(program).file_name())
        .is_some_and(|name| name == "glide-terminal")
}

pub fn open_terminal_tab(directory: &Path, argv: &[String], token: &str) -> zbus::Result<()> {
    let connection = zbus::blocking::Connection::session()?;
    let directory = directory.to_string_lossy();
    connection.call_method(
        Some(TERMINAL_BUS),
        TERMINAL_PATH,
        Some(TERMINAL_INTERFACE),
        "OpenTab",
        &(directory.as_ref(), argv, token),
    )?;
    Ok(())
}

pub use glide_config::uri::file_uri;

pub fn show_in_files(path: &Path, token: &str) -> zbus::Result<()> {
    let method = if path.is_dir() { "ShowFolders" } else { "ShowItems" };
    let connection = zbus::blocking::Connection::session()?;
    let body = (vec![file_uri(path)], token);
    connection.call_method(Some(FILES_BUS), FILE_MANAGER_PATH, Some(FILE_MANAGER_BUS), method, &body).or_else(
        |_| connection.call_method(Some(FILE_MANAGER_BUS), FILE_MANAGER_PATH, Some(FILE_MANAGER_BUS), method, &body),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_glide_terminal_counts_as_glide_terminal() {
        assert!(is_glide_terminal("glide-terminal"));
        assert!(is_glide_terminal("/usr/local/bin/glide-terminal --working-directory ~"));
        assert!(!is_glide_terminal("alacritty"));
        assert!(!is_glide_terminal(""));
    }
}
