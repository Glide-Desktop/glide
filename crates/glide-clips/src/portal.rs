use std::collections::HashMap;

use zbus::zvariant::{OwnedObjectPath, Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";

pub fn open_with(uri: &str) -> zbus::Result<()> {
    let connection = zbus::blocking::Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(&connection, PORTAL, PATH, "org.freedesktop.portal.OpenURI")?;
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("ask", Value::from(true));
    let _: OwnedObjectPath = proxy.call("OpenURI", &("", uri, options))?;
    Ok(())
}
