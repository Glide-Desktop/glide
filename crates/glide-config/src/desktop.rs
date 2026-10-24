pub fn entry(text: &str) -> impl Iterator<Item = (&str, &str)> {
    let mut in_entry = false;
    text.lines().filter_map(move |line| {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            return None;
        }
        let (key, value) = line.split_once('=').filter(|_| in_entry)?;
        Some((key.trim(), value.trim()))
    })
}

#[must_use]
pub fn entry_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    entry(text).find_map(|(name, value)| (name == key).then_some(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_desktop_entry_group_is_read_with_keys_and_values_trimmed() {
        let text = "[Desktop Action new]\nName=Wrong\n[Desktop Entry]\n Name = Right \nExec=app %U\nbare line\n\
                    [Other]\nIcon=no\n";
        assert_eq!(entry(text).collect::<Vec<_>>(), [("Name", "Right"), ("Exec", "app %U")]);
        assert_eq!(entry_value(text, "Name"), Some("Right"));
        assert_eq!(entry_value(text, "Icon"), None);
    }
}
