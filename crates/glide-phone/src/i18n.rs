pub use glide_sdk::Lang;

glide_sdk::catalogue! {
    PairRequest => "Pairing request from {device}", "Запит на з'єднання від {device}";
    PairRequestHint => "Open Phone Link to confirm the connection.", "Відкрийте «Підключення телефону», щоб підтвердити.";
    Ping => "Ping from {device}", "Пінг від {device}";
    PingBody => "Ping!", "Пінг!";
    Finding => "{device} is looking for this computer", "{device} шукає цей комп'ютер";
    FindingHint => "The alarm is ringing. Click to stop it.", "Лунає сигнал. Натисніть, щоб зупинити.";
    StopAlarm => "Stop Alarm", "Зупинити сигнал";
    LowBattery => "{device}: low battery", "{device}: низький заряд";
    BatteryAt => "Battery at {percent}%", "Заряд {percent}%";
    FileReceived => "File received from {device}", "Отримано файл від {device}";
    LinkFrom => "Link from {device}", "Посилання від {device}";
    TextCopied => "Text from {device} copied", "Текст від {device} скопійовано";
    TextFrom => "Text from {device}", "Текст від {device}";
    Reply => "Reply", "Відповісти";
    ReplyHint => "Type a reply…", "Введіть відповідь…";
}

#[must_use]
pub fn lang() -> Lang {
    let text = glide_sdk::look::shell_path().and_then(|path| std::fs::read_to_string(path).ok()).unwrap_or_default();
    glide_sdk::look::Look::parse(&text, false).lang()
}

#[must_use]
pub fn t(key: Key) -> &'static str {
    key.text(lang())
}

#[must_use]
pub fn naming(key: Key, device: &str) -> String {
    t(key).replace("{device}", device)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_name_fills_its_place_in_either_language() {
        for lang in [Lang::En, Lang::Uk] {
            for key in [Key::PairRequest, Key::Finding, Key::LowBattery, Key::FileReceived, Key::TextCopied] {
                assert!(key.text(lang).contains("{device}"), "{key:?} in {lang:?}");
            }
            assert!(Key::BatteryAt.text(lang).contains("{percent}"));
        }
        assert!(naming(Key::LinkFrom, "Pixel 9").contains("Pixel 9"));
    }
}
