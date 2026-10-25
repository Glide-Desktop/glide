use std::time::Duration;

use glide_ipc::keychain::{BUS_INTERFACE, BUS_NAME, BUS_PATH, CHANGED, Request, Response};
use iced::futures::{SinkExt, Stream, StreamExt};
use tokio::sync::OnceCell;

static CONNECTION: OnceCell<zbus::Connection> = OnceCell::const_new();

async fn connection() -> zbus::Result<&'static zbus::Connection> {
    CONNECTION.get_or_try_init(zbus::Connection::session).await
}

pub async fn call(request: Request) -> Result<Response, String> {
    let connection = connection().await.map_err(|err| err.to_string())?;
    let proxy = zbus::Proxy::new(connection, BUS_NAME, BUS_PATH, BUS_INTERFACE).await.map_err(|err| err.to_string())?;
    let line = serde_json::to_string(&request).map_err(|err| err.to_string())?;
    let reply: String = proxy.call("Call", &(line,)).await.map_err(|err| err.to_string())?;
    serde_json::from_str(&reply).map_err(|err| err.to_string())
}

pub fn changes() -> impl Stream<Item = ()> {
    iced::stream::channel(8, async |mut output| {
        loop {
            if let Ok(connection) = connection().await {
                let rule = zbus::MatchRule::builder()
                    .msg_type(zbus::message::Type::Signal)
                    .interface(BUS_INTERFACE)
                    .and_then(|rule| rule.member(CHANGED))
                    .map(zbus::match_rule::Builder::build);
                if let Ok(rule) = rule
                    && let Ok(mut stream) = zbus::MessageStream::for_match_rule(rule, connection, None).await
                {
                    while let Some(message) = stream.next().await {
                        if message.is_ok() && output.send(()).await.is_err() {
                            return;
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    })
}
