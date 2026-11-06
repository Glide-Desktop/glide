use std::collections::HashMap;
use std::path::Path;

use zbus::zvariant::{Fd, OwnedObjectPath, OwnedValue, Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

fn request(
    connection: &zbus::blocking::Connection, tag: &str,
) -> zbus::Result<(String, zbus::blocking::proxy::SignalIterator<'static>)> {
    let sender =
        connection.unique_name().map(|name| name.trim_start_matches(':').replace('.', "_")).unwrap_or_default();
    let nanos = std::time::SystemTime::UNIX_EPOCH.elapsed().map_or(0, |elapsed| elapsed.subsec_nanos());
    let token = format!("folio_{tag}_{}_{nanos}", std::process::id());
    let path = format!("{PORTAL_PATH}/request/{sender}/{token}");
    let proxy = zbus::blocking::Proxy::new_owned(connection.clone(), PORTAL, path, "org.freedesktop.portal.Request")?;
    let signals = proxy.receive_signal("Response")?;
    Ok((token, signals))
}

fn answer(
    signals: &mut zbus::blocking::proxy::SignalIterator<'static>,
) -> zbus::Result<Option<HashMap<String, OwnedValue>>> {
    let Some(message) = signals.next() else {
        return Ok(None);
    };
    let (code, results): (u32, HashMap<String, OwnedValue>) = message.body().deserialize()?;
    Ok((code == 0).then_some(results))
}

fn run(path: &Path, title: &str) -> zbus::Result<bool> {
    let connection = zbus::blocking::Connection::session()?;
    let print = zbus::blocking::Proxy::new(&connection, PORTAL, PORTAL_PATH, "org.freedesktop.portal.Print")?;
    let (token, mut signals) = request(&connection, "prepare")?;
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    options.insert("modal", Value::from(true));
    let empty: HashMap<&str, Value<'_>> = HashMap::new();
    let _: OwnedObjectPath = print.call("PreparePrint", &("", title, &empty, &empty, options))?;
    let Some(results) = answer(&mut signals)? else {
        return Ok(true);
    };
    let Some(job) = results.get("token").and_then(|value| u32::try_from(value).ok()) else {
        return Ok(false);
    };
    let file = std::fs::File::open(path).map_err(|error| zbus::Error::Failure(error.to_string()))?;
    let (token, mut signals) = request(&connection, "print")?;
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    options.insert("token", Value::from(job));
    let _: OwnedObjectPath = print.call("Print", &("", title, Fd::from(&file), options))?;
    Ok(answer(&mut signals)?.is_some())
}

#[must_use]
pub fn print(path: &Path, title: &str) -> bool {
    run(path, title).unwrap_or(false)
}
