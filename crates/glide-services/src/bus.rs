fn appeared(message: &zbus::Message, service: &str) -> bool {
    let header = message.header();
    let signal = header.message_type() == zbus::message::Type::Signal
        && header.sender().is_some_and(|sender| sender.as_str() == "org.freedesktop.DBus")
        && header.interface().is_some_and(|interface| interface.as_str() == "org.freedesktop.DBus")
        && header.member().is_some_and(|member| member.as_str() == "NameOwnerChanged");
    signal
        && message
            .body()
            .deserialize::<(String, String, String)>()
            .is_ok_and(|(name, _, owner)| name == service && !owner.is_empty())
}

pub fn keep_registered(
    connection: zbus::blocking::Connection, service: &'static str, thread: &str,
    register: fn(&zbus::blocking::Connection) -> zbus::Result<()>,
) -> anyhow::Result<()> {
    let rule = format!(
        "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',\
         arg0='{service}'"
    );
    zbus::blocking::fdo::DBusProxy::new(&connection)?.add_match_rule(zbus::MatchRule::try_from(rule.as_str())?)?;
    std::thread::Builder::new().name(thread.into()).spawn(move || {
        let messages = zbus::blocking::MessageIterator::from(&connection);
        if let Err(err) = register(&connection) {
            tracing::info!(service, %err, "the service is not reachable yet; the agent waits for it");
        }
        for message in messages {
            if message.as_ref().is_ok_and(|message| appeared(message, service))
                && let Err(err) = register(&connection)
            {
                tracing::warn!(service, %err, "could not register the agent again");
            }
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_bus_can_announce_a_service_owner() {
        let signal = |sender: &str, interface: &str| {
            zbus::Message::signal("/org/freedesktop/DBus", interface, "NameOwnerChanged")
                .unwrap()
                .sender(sender)
                .unwrap()
                .build(&("org.example.Service", "", ":1.7"))
                .unwrap()
        };
        let bus = "org.freedesktop.DBus";
        assert!(appeared(&signal(bus, bus), "org.example.Service"));
        assert!(!appeared(&signal(":1.99", bus), "org.example.Service"));
        assert!(!appeared(&signal(bus, "org.example.Forgery"), "org.example.Service"));
        assert!(!appeared(&signal(bus, bus), "org.example.Other"));
    }
}
