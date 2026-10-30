use glide_ipc::keychain::Hidden;
use iced::Task;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scanned {
    pub key: String,
    pub issuer: String,
    pub account: String,
}

fn decoded(text: &str) -> String {
    String::from_utf8_lossy(&glide_config::uri::decode(&text.replace('+', " "))).into_owned()
}

#[must_use]
pub fn parse(uri: &str) -> Option<Scanned> {
    let rest = uri.trim();
    let rest = rest.get(..15).filter(|scheme| scheme.eq_ignore_ascii_case("otpauth://totp/")).map(|_| &rest[15..])?;
    let (label, query) = rest.split_once('?').unwrap_or((rest, ""));
    let mut secret = None;
    let mut issuer = String::new();
    let mut standard = true;
    for pair in query.split('&') {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = decoded(value);
        match name.to_ascii_lowercase().as_str() {
            "secret" => secret = Some(value),
            "issuer" => issuer = value,
            "algorithm" => standard &= value.eq_ignore_ascii_case("sha1"),
            "digits" => standard &= value == "6",
            "period" => standard &= value == "30",
            _ => {}
        }
    }
    let secret: String =
        secret?.chars().filter(|ch| !ch.is_whitespace() && *ch != '-' && *ch != '=').collect::<String>().to_uppercase();
    if secret.is_empty() || !secret.chars().all(|ch| matches!(ch, 'A'..='Z' | '2'..='7')) {
        return None;
    }
    let label = String::from_utf8_lossy(&glide_config::uri::decode(label)).into_owned();
    let (prefix, account) = label.split_once(':').map_or(("", label.as_str()), |(issuer, account)| (issuer, account));
    if issuer.is_empty() {
        issuer = prefix.trim().to_string();
    }
    let key = if standard { secret } else { uri.trim().to_string() };
    Some(Scanned { key, issuer, account: account.trim().to_string() })
}

#[must_use]
pub fn first_totp(codes: &[Hidden]) -> Option<Scanned> {
    codes.iter().find_map(|code| parse(code.expose()))
}

pub fn screen<M: Send + 'static>(done: impl Fn(Result<Vec<Hidden>, String>) -> M + Send + 'static) -> Task<M> {
    Task::perform(
        async {
            let (sender, receiver) = tokio::sync::oneshot::channel();
            let spawned = std::thread::Builder::new().name("glide-passwords-scan".into()).spawn(move || {
                let _ = sender.send(glide_sdk::system::scan_screen_qr());
            });
            match spawned {
                Ok(_) => receiver.await.unwrap_or_else(|_| Err("the scan stopped".into())),
                Err(err) => Err(err.to_string()),
            }
        },
        done,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_standard_code_keeps_only_its_secret() {
        let scanned =
            parse("otpauth://totp/GitHub:artem%40example.com?secret=jbsw%20y3dp-ehpk3pxp&issuer=GitHub").expect("totp");
        assert_eq!(
            scanned,
            Scanned { key: "JBSWY3DPEHPK3PXP".into(), issuer: "GitHub".into(), account: "artem@example.com".into() }
        );
    }

    #[test]
    fn the_label_names_the_issuer_when_the_query_does_not() {
        let scanned = parse("OTPAUTH://TOTP/Example%3Ame?secret=GEZDGNBVGY3TQOJQ").expect("totp");
        assert_eq!((scanned.issuer.as_str(), scanned.account.as_str()), ("Example", "me"));
    }

    #[test]
    fn a_plus_in_the_label_stays_a_plus() {
        let scanned = parse("otpauth://totp/C++:me?secret=GEZDGNBVGY3TQOJQ").expect("totp");
        assert_eq!((scanned.issuer.as_str(), scanned.account.as_str()), ("C++", "me"));
    }

    #[test]
    fn other_digits_or_hashes_keep_the_whole_uri() {
        let uri = "otpauth://totp/x?secret=GEZDGNBVGY3TQOJQ&digits=8&algorithm=SHA256";
        assert_eq!(parse(uri).expect("totp").key, uri);
    }

    #[test]
    fn anything_else_is_not_a_code() {
        assert!(parse("https://example.com").is_none());
        assert!(parse("otpauth://hotp/x?secret=GEZDGNBV&counter=1").is_none());
        assert!(parse("otpauth://totp/x?issuer=nobody").is_none());
        assert!(parse("otpauth://totp/x?secret=not*base32").is_none());
        let codes = [Hidden::new("https://example.com".into()), Hidden::new("otpauth://totp/a?secret=MFRGG".into())];
        assert_eq!(first_totp(&codes).expect("second").key, "MFRGG");
    }
}
