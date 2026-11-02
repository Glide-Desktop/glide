use std::sync::Arc;

use glide_phone::client::PhoneDaemonProxyBlocking;
use glide_phone::config::PhoneConfig;
use glide_phone::daemon::start_dbus_service;
use glide_phone::discovery::bind_discovery_socket;
use glide_phone::service::PhoneService;
use glide_phone::ui::{PhoneApp, set_global_manager};

fn is_daemon_running() -> bool {
    let Ok(conn) = zbus::blocking::Connection::session() else {
        return false;
    };
    let Ok(dbus) = zbus::blocking::fdo::DBusProxy::new(&conn) else {
        return false;
    };
    let Ok(name) = "org.glide.Phone".try_into() else {
        return false;
    };
    dbus.name_has_owner(name).unwrap_or(false)
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();

    if let Some(pos) = args.iter().position(|arg| arg == "--ring") {
        let dev_id = args.get(pos + 1).cloned().unwrap_or_default();
        if let Ok(conn) = zbus::blocking::Connection::session()
            && let Ok(proxy) = PhoneDaemonProxyBlocking::new(&conn)
        {
            let _ = proxy.ring_phone(&dev_id);
        }
        return Ok(());
    }

    if args.iter().any(|arg| arg == "--stop") {
        if let Ok(conn) = zbus::blocking::Connection::session()
            && let Ok(proxy) = PhoneDaemonProxyBlocking::new(&conn)
        {
            let _ = proxy.stop_alarm("");
        }
        return Ok(());
    }

    let is_daemon = args.iter().any(|arg| arg == "--daemon" || arg == "-d");

    if is_daemon && is_daemon_running() {
        return Ok(());
    }

    if !is_daemon && is_daemon_running() {
        tracing::info!("connecting to running glide-phone daemon via D-Bus");
        let settings = glide_sdk::Settings::new("glide-phone")
            .size(720.0, 640.0)
            .min_size(520.0, 440.0)
            .single_instance("org.glide.PhoneGui")
            .chrome(glide_sdk::Chrome::Server);

        let _ = glide_sdk::run::<PhoneApp>(settings);
        return Ok(());
    }

    let config_path = PhoneConfig::default_config_path();
    let (config, cert) = PhoneConfig::load_or_generate(&config_path)?;

    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;

    let (service, listener) = rt.block_on(PhoneService::bind(config, cert))?;
    let service = Arc::new(service);

    set_global_manager(service.manager());

    let _dbus_conn = rt.block_on(start_dbus_service(service.manager()))?;

    let service_clone = service.clone();
    rt.spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    let s = service_clone.clone();
                    tokio::spawn(async move {
                        if let Err(err) = s.handle_incoming_connection(stream, peer_addr).await {
                            tracing::warn!(%peer_addr, %err, "phone incoming connection failed");
                        }
                    });
                }
                Err(err) => {
                    tracing::error!(%err, "phone TCP listener failed");
                    break;
                }
            }
        }
    });

    rt.spawn(async move {
        match bind_discovery_socket().await {
            Ok(udp_socket) => {
                if let Err(err) = service.run_discovery(Arc::new(udp_socket), Vec::new()).await {
                    tracing::error!(%err, "phone discovery stopped");
                }
            }
            Err(err) => tracing::error!(%err, "phone discovery socket failed"),
        }
    });

    if is_daemon {
        tracing::info!("running glide-phone as background daemon");
        rt.block_on(async {
            let _ = tokio::signal::ctrl_c().await;
        });
        return Ok(());
    }

    let settings = glide_sdk::Settings::new("glide-phone")
        .size(720.0, 640.0)
        .min_size(520.0, 440.0)
        .single_instance("org.glide.PhoneGui")
        .chrome(glide_sdk::Chrome::Server);

    let _ = glide_sdk::run::<PhoneApp>(settings);
    Ok(())
}
