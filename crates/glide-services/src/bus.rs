use std::time::Duration;

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
        register_until_accepted(&connection, service, register, std::thread::sleep);
        for message in messages {
            if message.as_ref().is_ok_and(|message| appeared(message, service)) {
                register_until_accepted(&connection, service, register, std::thread::sleep);
            }
        }
    })?;
    Ok(())
}

const FIRST_RETRY: Duration = Duration::from_secs(1);
const LAST_RETRY: Duration = Duration::from_mins(1);

fn already_registered(err: &zbus::Error) -> bool {
    let text = err.to_string().to_lowercase();
    text.contains("already registered") || text.contains("already exists")
}

fn register_until_accepted<C>(
    connection: &C, service: &str, register: fn(&C) -> zbus::Result<()>, sleep: impl Fn(Duration),
) {
    let mut delay = FIRST_RETRY;
    let mut refusals = 0_u32;
    loop {
        match register(connection) {
            Ok(()) => return,
            Err(err) if already_registered(&err) => {
                tracing::info!(service, %err, "an agent is already registered for this session; not retrying");
                return;
            }
            Err(err) => {
                refusals += 1;
                if refusals == 1 {
                    tracing::warn!(service, %err, "could not register the agent; retrying with backoff");
                } else {
                    tracing::debug!(service, %err, refusals, "the agent is still not registered");
                }
            }
        }
        sleep(delay);
        delay = (delay * 2).min(LAST_RETRY);
    }
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

    struct Flaky {
        refusals: std::cell::Cell<u32>,
        error: &'static str,
    }

    fn flaky(connection: &Flaky) -> zbus::Result<()> {
        let left = connection.refusals.get();
        if left == 0 {
            return Ok(());
        }
        connection.refusals.set(left - 1);
        Err(zbus::Error::Failure(connection.error.into()))
    }

    #[test]
    fn a_refused_registration_is_retried_with_a_growing_capped_delay() {
        let connection = Flaky { refusals: std::cell::Cell::new(9), error: "no session" };
        let slept = std::cell::RefCell::new(Vec::new());
        register_until_accepted(&connection, "org.example.Service", flaky, |delay| slept.borrow_mut().push(delay));
        let slept = slept.into_inner();
        assert_eq!(slept.len(), 9);
        assert_eq!(slept[..3], [Duration::from_secs(1), Duration::from_secs(2), Duration::from_secs(4)]);
        assert_eq!(slept.last(), Some(&LAST_RETRY));
    }

    #[test]
    fn an_agent_the_service_still_knows_counts_as_registered() {
        let connection =
            Flaky { refusals: std::cell::Cell::new(5), error: "An agent is already registered for this subject" };
        let slept = std::cell::Cell::new(0);
        register_until_accepted(&connection, "org.example.Service", flaky, |_| slept.set(slept.get() + 1));
        assert_eq!(slept.get(), 0);
    }
}
