use futures_channel::mpsc::{UnboundedReceiver, unbounded};

pub use glide_config::service::{Change, Client, changed_keys, load, read, update};
pub use glide_ipc::config::{
    BUS_INTERFACE as INTERFACE, BUS_NAME, BUS_PATH as PATH, CHANGED, SHELL, Values, flatten, nest,
};

fn changes_of(
    connection: &zbus::blocking::Connection, namespace: &str,
) -> zbus::Result<zbus::blocking::MessageIterator> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .path(PATH)?
        .interface(INTERFACE)?
        .member(CHANGED)?
        .arg(0, namespace.to_owned())?
        .build();
    zbus::blocking::MessageIterator::for_match_rule(rule, connection, None)
}

fn follow(
    connection: zbus::blocking::Connection, namespace: &str, sink: &(impl Fn(Values) -> bool + Send),
) -> zbus::Result<()> {
    let changes = changes_of(&connection, namespace)?;
    let client = Client::on(connection);
    if !sink(client.get_all(namespace)?) {
        return Ok(());
    }
    for message in changes {
        message?;
        if !sink(client.get_all(namespace)?) {
            return Ok(());
        }
    }
    Err(zbus::Error::Failure("the session bus connection closed".into()))
}

pub fn watch_with(namespace: &str, sink: impl Fn(Values) -> bool + Send + 'static) {
    let namespace = namespace.to_owned();
    let spawned = std::thread::Builder::new().name(format!("glide-config-{namespace}")).spawn(move || {
        let mut backoff = crate::RETRY;
        loop {
            match zbus::blocking::Connection::session().and_then(|connection| follow(connection, &namespace, &sink)) {
                Ok(()) => return,
                Err(err) => tracing::debug!("stopped following the {namespace} settings: {err}"),
            }
            std::thread::sleep(backoff);
            backoff = (backoff * 2).min(crate::MAX_RETRY);
        }
    });
    if let Err(err) = spawned {
        tracing::warn!("cannot follow the settings: {err}");
    }
}

#[must_use]
pub fn watch(namespace: &str) -> UnboundedReceiver<Values> {
    let (sender, receiver) = unbounded();
    watch_with(namespace, move |values| sender.unbounded_send(values).is_ok());
    receiver
}
