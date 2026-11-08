use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use glide_netbird::{self as nb, ConfigPatch};
use iced::{Element, Task};

use super::{Ctx, Pane};
use crate::services::config::ConfigStore;
use crate::services::netbird::{self as service, BundleJob, Failure, Snapshot};
use crate::services::network::Secret;
use crate::services::run::{Generation, blocking};

mod view;

mod tr {
    crate::i18n::catalogue! {
        Title => "NetBird", "NetBird";
        NotInstalled => "NetBird is not installed", "NetBird не встановлено";
        NotInstalledHint => "Install the netbird package to manage your mesh network here", "Встановіть пакет netbird, щоб керувати мережею тут";
        ServiceDown => "NetBird service is not running", "Служба NetBird не запущена";
        ServiceDownHint => "This page reconnects automatically once the service is running. Start it with systemctl start netbird.", "Сторінка під'єднається сама, щойно служба запуститься. Запустіть її командою systemctl start netbird.";
        Outdated => "The NetBird service is older than this page expects. Update the netbird package.", "Служба NetBird старіша, ніж очікує ця сторінка. Оновіть пакет netbird.";
        Connected => "Connected", "Підключено";
        Connecting => "Connecting…", "Підключення…";
        Disconnecting => "Disconnecting…", "Відключення…";
        Disconnected => "Disconnected", "Відключено";
        LoginRequired => "Login required", "Потрібен вхід";
        LoginFailed => "Login failed", "Не вдалося увійти";
        SessionExpired => "Session expired", "Сесія завершилася";
        SignIn => "Sign in", "Увійти";
        SigningIn => "Waiting for the browser…", "Очікування браузера…";
        Tabs => "Sections", "Розділи";
        TabOverview => "Overview", "Огляд";
        TabPeers => "Peers", "Вузли";
        TabResources => "Resources", "Ресурси";
        TabSettings => "Settings", "Налаштування";
        TabTroubleshoot => "Troubleshoot", "Діагностика";
        TabAbout => "About", "Про NetBird";
        Connection => "Connection", "З'єднання";
        Management => "Management", "Керування";
        Signal => "Signal", "Сигнальний сервер";
        Relays => "Relays", "Ретранслятори";
        RelaysLine => "{n} of {total} available", "{n} з {total} доступні";
        Reachable => "Reachable", "Доступний";
        Unreachable => "Unreachable", "Недоступний";
        NetbirdIp => "NetBird IP", "IP у NetBird";
        NetbirdIpv6 => "NetBird IPv6", "IPv6 у NetBird";
        Domain => "Domain", "Домен";
        Interface => "Interface", "Інтерфейс";
        Kernel => "Kernel WireGuard", "WireGuard ядра";
        Userspace => "Userspace WireGuard", "WireGuard у просторі користувача";
        Port => "Port", "Порт";
        Session => "Session", "Сесія";
        Account => "Account", "Обліковий запис";
        SessionLeft => "Expires in {time}", "Спливає через {time}";
        Renew => "Renew", "Подовжити";
        Renewing => "Waiting for the browser…", "Очікування браузера…";
        LessThanMinute => "less than a minute", "менш ніж хвилину";
        Minutes => "{n} min", "{n} хв";
        Hours => "{n} h", "{n} год";
        Days => "{n} d", "{n} дн.";
        DaemonVersion => "Service version", "Версія служби";
        Dns => "DNS", "DNS";
        DnsLine => "Name server groups: {n}", "Груп серверів імен: {n}";
        SshServer => "SSH server", "SSH-сервер";
        SshSessions => "{n} active sessions", "Активних сесій: {n}";
        Forwarding => "Port forwarding", "Переадресація портів";
        ForwardingLine => "{n} rules", "Правил: {n}";
        On => "On", "Увімкнено";
        Off => "Off", "Вимкнено";
        ExitNode => "Exit node", "Вихідний вузол";
        ExitNodeHint => "Send all internet traffic through another peer", "Спрямовувати весь інтернет-трафік через інший вузол";
        ExitNone => "None", "Немає";
        ExitNoneHint => "Direct connection without an exit node", "Пряме з'єднання без вихідного вузла";
        ExitNoneAvailable => "No exit nodes have been shared with this peer", "З цим вузлом не поділилися жодним вихідним вузлом";
        Profiles => "Profiles", "Профілі";
        ProfilesHint => "Keep separate NetBird identities side by side, for example work and personal accounts, or different management servers", "Тримайте окремі облікові записи NetBird поруч, наприклад робочий і особистий або різні сервери керування";
        Active => "Active", "Активний";
        NoEmail => "No account yet", "Обліковий запис ще не підключено";
        Switch => "Switch", "Перемкнути";
        Switching => "Switching…", "Перемикання…";
        Rename => "Rename", "Перейменувати";
        Save => "Save", "Зберегти";
        Cancel => "Cancel", "Скасувати";
        Delete => "Delete", "Видалити";
        Deregister => "Deregister", "Від'єднати";
        DeleteConfirm => "Delete this profile? Its settings are removed from this computer.", "Видалити цей профіль? Його налаштування буде видалено з цього комп'ютера.";
        DeregisterConfirm => "Deregister this profile? The device is removed from its NetBird network and must sign in again.", "Від'єднати цей профіль? Пристрій буде видалено з мережі NetBird, і знадобиться новий вхід.";
        AddProfile => "Add profile", "Додати профіль";
        ProfileName => "Profile name", "Назва профілю";
        ProfileNamePlaceholder => "e.g. Work", "наприклад, Робота";
        Cloud => "NetBird Cloud", "NetBird Cloud";
        SelfHosted => "Self-hosted", "Власний сервер";
        ManagementUrl => "Management server URL", "Адреса сервера керування";
        ManagementUrlPlaceholder => "https://netbird.selfhosted.com:443", "https://netbird.selfhosted.com:443";
        UrlInvalid => "Enter a valid URL, e.g. https://netbird.selfhosted.com:443", "Введіть правильну адресу, наприклад https://netbird.selfhosted.com:443";
        Events => "Recent events", "Останні події";
        CatAuthentication => "Authentication", "Автентифікація";
        CatConnectivity => "Connectivity", "З'єднання";
        CatSystem => "System", "Система";
        NoEvents => "Nothing has happened yet", "Поки нічого не сталося";
        NotConnected => "Disconnected", "Відключено";
        NotConnectedHint => "Connect to NetBird first to see your peers, network resources and exit nodes.", "Спершу підключіться до NetBird, щоб побачити вузли, мережеві ресурси та вихідні вузли.";
        PeersSearch => "Search by name or IP", "Пошук за назвою або IP";
        All => "All", "Усі";
        Online => "Online", "На зв'язку";
        Offline => "Offline", "Поза мережею";
        PeersCount => "{n} of {total} connected", "{n} з {total} на зв'язку";
        NoPeers => "No peers available", "Немає доступних вузлів";
        NoPeersHint => "You either don't have any peers or have no access to any of them.", "У вас немає вузлів або доступу до жодного з них.";
        NoMatches => "Nothing matches", "Нічого не знайдено";
        NoMatchesHint => "Try a different search term or another filter.", "Спробуйте інший запит або фільтр.";
        PeerConnecting => "Connecting", "Підключення";
        PeerDisconnected => "Disconnected", "Не на зв'язку";
        P2p => "P2P", "Напряму (P2P)";
        Relayed => "Relayed", "Через ретранслятор";
        PublicKey => "Public key", "Публічний ключ";
        Connection2 => "Connection", "З'єднання";
        LocalIce => "Local ICE", "Локальний ICE";
        RemoteIce => "Remote ICE", "Віддалений ICE";
        Relay => "Relay", "Ретранслятор";
        Latency => "Latency", "Затримка";
        LastHandshake => "Last handshake", "Останнє рукостискання";
        StatusSince => "Last connection update", "Останнє оновлення з'єднання";
        Transfer => "Transfer", "Передано";
        TransferLine => "{rx} received · {tx} sent", "{rx} отримано · {tx} надіслано";
        Rosenpass => "Quantum resistance", "Квантова стійкість";
        Never => "Never", "Ніколи";
        JustNow => "Just now", "Щойно";
        Ago => "{time} ago", "{time} тому";
        PeerResources => "Resources", "Ресурси";
        ResourcesSearch => "Search by network or domain", "Пошук за мережею або доменом";
        ResourcesActive => "Active", "Активні";
        Overlapping => "Overlapping", "Перекриваються";
        ResourcesCount => "{n} of {total} active", "{n} з {total} активні";
        EnableAll => "Enable all", "Увімкнути всі";
        DisableAll => "Disable all", "Вимкнути всі";
        NoResources => "No resources available", "Немає доступних ресурсів";
        NoResourcesHint => "You either don't have any network resources or have no access to any of them.", "У вас немає мережевих ресурсів або доступу до жодного з них.";
        ResolvedIps => "Resolved IPs", "Отримані IP";
        General => "General", "Загальне";
        ManagementServer => "Management server", "Сервер керування";
        ManagementHint => "Connect to NetBird Cloud or your own server. Changing it reconnects the client.", "Під'єднання до NetBird Cloud або власного сервера. Зміна перепід'єднує клієнт.";
        ConnectOnStartup => "Connect on startup", "Підключатися під час запуску";
        ConnectOnStartupHint => "Establish the connection when the service starts", "Встановлювати з'єднання, коли запускається служба";
        Notifications => "Desktop notifications", "Сповіщення на робочому столі";
        NotificationsHint => "Show notifications for updates and connection events", "Показувати сповіщення про оновлення та події з'єднання";
        Network => "Network", "Мережа";
        ClientRoutes => "Enable client routes", "Маршрути від інших вузлів";
        ClientRoutesHint => "Accept routes from other peers to reach their networks", "Приймати маршрути від інших вузлів, щоб діставатися їхніх мереж";
        ServerRoutes => "Enable server routes", "Маршрути цього вузла";
        ServerRoutesHint => "Advertise this host's local routes to other peers", "Оголошувати локальні маршрути цього комп'ютера іншим вузлам";
        DnsSwitch => "Enable DNS", "Увімкнути DNS";
        DnsHint => "Apply NetBird-managed DNS settings to the host resolver", "Застосовувати DNS-налаштування NetBird до системного розпізнавача";
        Ipv6 => "Enable IPv6", "Увімкнути IPv6";
        Ipv6Hint => "Use IPv6 addressing for the NetBird overlay network", "Використовувати IPv6-адреси в мережі NetBird";
        Monitor => "Reconnect on network change", "Перепід'єднуватися при зміні мережі";
        MonitorHint => "Reconnect after Wi-Fi switching, cable changes or resume from sleep", "Перепід'єднуватися після зміни Wi-Fi, кабелю чи виходу зі сну";
        Lazy => "Lazy connections", "Ліниві з'єднання";
        LazyHint => "Open tunnels to peers only when traffic needs them", "Відкривати тунелі до вузлів лише тоді, коли потрібен трафік";
        Security => "Security", "Безпека";
        BlockInbound => "Block inbound traffic", "Блокувати вхідний трафік";
        BlockInboundHint => "Reject unsolicited connections from peers to this device and the networks it routes", "Відхиляти непрохані з'єднання від вузлів до цього пристрою та мереж, які він маршрутизує";
        BlockLan => "Block LAN access", "Блокувати доступ до локальної мережі";
        BlockLanHint => "Keep peers away from your local network when this device routes their traffic", "Не пускати вузли до локальної мережі, коли цей пристрій маршрутизує їхній трафік";
        Quantum => "Enable quantum resistance", "Увімкнути квантову стійкість";
        QuantumHint => "Secure tunnels with Rosenpass post-quantum key exchange", "Захищати тунелі постквантовим обміном ключами Rosenpass";
        Permissive => "Permissive mode", "Поблажливий режим";
        PermissiveHint => "Allow connections to peers without quantum-resistance support", "Дозволяти з'єднання з вузлами без квантової стійкості";
        Ssh => "SSH", "SSH";
        SshServerSwitch => "Enable SSH server", "Увімкнути SSH-сервер";
        SshServerHint => "Run the NetBird SSH server so other peers can connect to this host", "Запустити SSH-сервер NetBird, щоб інші вузли могли під'єднатися до цього комп'ютера";
        SshRoot => "Allow root login", "Дозволити вхід як root";
        SshRootHint => "Let peers sign in as the root user", "Дозволити вузлам входити як root";
        Sftp => "Allow SFTP", "Дозволити SFTP";
        SftpHint => "Transfer files with native SFTP or SCP clients", "Передавати файли звичайними клієнтами SFTP або SCP";
        LocalForward => "Local port forwarding", "Локальна переадресація портів";
        LocalForwardHint => "Let connecting peers tunnel local ports to services reachable from this host", "Дозволити вузлам тунелювати локальні порти до служб, доступних з цього комп'ютера";
        RemoteForward => "Remote port forwarding", "Віддалена переадресація портів";
        RemoteForwardHint => "Let connecting peers expose ports on this host back to their machine", "Дозволити вузлам відкривати порти цього комп'ютера на своїй машині";
        Jwt => "Enable JWT authentication", "Увімкнути автентифікацію JWT";
        JwtHint => "Verify each SSH session against your identity provider", "Перевіряти кожну SSH-сесію у вашого постачальника ідентичності";
        JwtTtl => "JWT cache TTL", "Час кешування JWT";
        JwtTtlHint => "Seconds to reuse a token for outgoing SSH connections; 0 asks every time", "Скільки секунд використовувати токен для вихідних SSH-з'єднань; 0 — питати щоразу";
        Seconds => "s", "с";
        Advanced => "Advanced", "Додатково";
        InterfaceName => "Interface name", "Назва інтерфейсу";
        InterfaceInvalid => "Use 1–15 letters, digits, dots, hyphens or underscores", "Використовуйте 1–15 літер, цифр, крапок, дефісів або підкреслень";
        WireguardPort => "WireGuard port", "Порт WireGuard";
        PortHint => "0 picks a random free port", "0 — випадковий вільний порт";
        PortInvalid => "Enter a port between 0 and 65535", "Введіть порт від 0 до 65535";
        Mtu => "MTU", "MTU";
        MtuInvalid => "Enter an MTU between 576 and 8192", "Введіть MTU від 576 до 8192";
        Psk => "Pre-shared key", "Попередньо спільний ключ";
        PskHint => "Optional WireGuard key for extra symmetric encryption. Not a setup key: only peers with the same key can talk to this one.", "Необов'язковий ключ WireGuard для додаткового симетричного шифрування. Це не ключ налаштування: спілкуватися можна буде лише з вузлами з тим самим ключем.";
        PskSet => "A key is set", "Ключ задано";
        PskPlaceholder => "Paste a new key", "Вставте новий ключ";
        PskClear => "Remove key", "Прибрати ключ";
        Managed => "Managed by your organisation", "Керує ваша організація";
        PrivilegeHint => "This needs administrator rights. You can also run:", "Для цього потрібні права адміністратора. Можна також виконати:";
        Copy => "Copy", "Копіювати";
        Declined => "The change was not made: authorisation was cancelled", "Зміну не внесено: авторизацію скасовано";
        Bundle => "Debug bundle", "Пакет діагностики";
        BundleHint => "Collect logs, system details and the connection state to share with NetBird support", "Зібрати журнали, відомості про систему та стан з'єднання для підтримки NetBird";
        Anonymize => "Anonymize sensitive information", "Приховувати чутливі дані";
        AnonymizeHint => "Default keeps internal addresses and peer names readable; strict hides those too", "Типово внутрішні адреси та назви вузлів лишаються видимими; суворо — ховаються й вони";
        AnonymizeNone => "None", "Ні";
        AnonymizeDefault => "Default", "Типово";
        AnonymizeStrict => "Strict", "Суворо";
        SystemInfo => "Include system information", "Додати відомості про систему";
        SystemInfoHint => "OS, kernel, network interfaces and routing tables", "ОС, ядро, мережеві інтерфейси та таблиці маршрутизації";
        Upload => "Upload bundle to NetBird servers", "Вивантажити пакет на сервери NetBird";
        UploadHint => "Returns an upload key to share with NetBird support", "Повертає ключ вивантаження для служби підтримки NetBird";
        Trace => "Enable trace logs", "Докладні журнали (trace)";
        TraceHint => "Raises the log level to TRACE and restores it afterwards", "Піднімає рівень журналу до TRACE і повертає його потім";
        CaptureSession => "Capture session", "Сеанс запису";
        CaptureSessionHint => "Reconnects and waits so you can reproduce the issue", "Перепід'єднується й чекає, щоб ви відтворили проблему";
        Duration => "Capture duration", "Тривалість запису";
        DurationHint => "Minutes, from 1 to 30", "Хвилини, від 1 до 30";
        Packets => "Capture network packets", "Записувати мережеві пакети";
        PacketsHint => "Saves a .pcap of network traffic during the capture", "Зберігає .pcap мережевого трафіку під час запису";
        Create => "Create bundle", "Створити пакет";
        Capturing => "Capturing… reproduce the issue now", "Запис… відтворіть проблему зараз";
        Collecting => "Collecting logs and connection state…", "Збирання журналів і стану з'єднання…";
        Saved => "Bundle saved", "Пакет збережено";
        Uploaded => "Debug bundle uploaded", "Пакет діагностики вивантажено";
        UploadKey => "Upload key", "Ключ вивантаження";
        UploadFailed => "Upload failed: {reason} The bundle is still saved locally.", "Вивантаження не вдалося: {reason} Пакет збережено локально.";
        OpenFolder => "Open folder", "Відкрити теку";
        Versions => "Versions", "Версії";
        ServiceVersion => "NetBird service", "Служба NetBird";
        GlideVersion => "Glide integration", "Інтеграція Glide";
        Update => "Update", "Оновлення";
        UpdateAvailable => "NetBird {version} is available", "Доступна версія NetBird {version}";
        UpdateEnforced => "Your administrator requires this update", "Адміністратор вимагає це оновлення";
        UpdateNone => "No update has been announced", "Оновлень не оголошено";
        InstallNow => "Install now", "Встановити";
        Download => "Download", "Завантажити";
        Links => "Help and community", "Довідка та спільнота";
        Documentation => "Documentation", "Документація";
        GitHub => "GitHub", "GitHub";
        Forum => "Forum", "Форум";
        Slack => "Slack", "Slack";
        ReportBug => "Report a problem", "Повідомити про проблему";
        Terms => "Terms of service", "Умови використання";
        Privacy => "Privacy", "Конфіденційність";
        ErrClockSkew => "This device's clock is out of sync with the server. Sync the clock and try again.", "Годинник цього пристрою розходиться із сервером. Синхронізуйте час і спробуйте знову.";
        ErrTokenExpired => "Your sign-in token has expired. Please sign in again.", "Термін дії токена входу минув. Увійдіть знову.";
        ErrSignature => "The token signature is invalid. Please contact your administrator.", "Підпис токена недійсний. Зверніться до адміністратора.";
        ErrSessionExpired => "Your session has expired. Please sign in again.", "Сесія завершилася. Увійдіть знову.";
        ErrSetupKey => "The setup key is missing or invalid.", "Ключ налаштування відсутній або недійсний.";
        ErrPermission => "The request was rejected.", "Запит відхилено.";
        ErrUnreachable => "The NetBird service is not responding.", "Служба NetBird не відповідає.";
        ErrOutdated => "The NetBird service does not support this. Update the netbird package.", "Служба NetBird цього не підтримує. Оновіть пакет netbird.";
        ErrUnknown => "The operation failed.", "Операція не вдалася.";
    }
}

pub(crate) use tr::Key as T;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Overview,
    Peers,
    Resources,
    Settings,
    Troubleshoot,
    About,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PeerFilter {
    #[default]
    All,
    Online,
    Offline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResourceFilter {
    #[default]
    All,
    Active,
    Overlapping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    ConnectOnStartup,
    Notifications,
    ClientRoutes,
    ServerRoutes,
    Dns,
    Ipv6,
    Monitor,
    Lazy,
    BlockInbound,
    BlockLan,
    Quantum,
    Permissive,
    SshServer,
    SshRoot,
    Sftp,
    LocalForward,
    RemoteForward,
    Jwt,
}

impl Setting {
    #[must_use]
    pub const fn value(self, config: &nb::Config) -> bool {
        match self {
            Self::ConnectOnStartup => !config.disable_auto_connect,
            Self::Notifications => !config.disable_notifications,
            Self::ClientRoutes => !config.disable_client_routes,
            Self::ServerRoutes => !config.disable_server_routes,
            Self::Dns => !config.disable_dns,
            Self::Ipv6 => !config.disable_ipv6,
            Self::Monitor => config.network_monitor,
            Self::Lazy => config.lazy_connection_enabled,
            Self::BlockInbound => config.block_inbound,
            Self::BlockLan => config.block_lan_access,
            Self::Quantum => config.rosenpass_enabled,
            Self::Permissive => config.rosenpass_permissive,
            Self::SshServer => config.server_ssh_allowed,
            Self::SshRoot => config.enable_ssh_root,
            Self::Sftp => config.enable_ssh_sftp,
            Self::LocalForward => config.enable_ssh_local_port_forwarding,
            Self::RemoteForward => config.enable_ssh_remote_port_forwarding,
            Self::Jwt => !config.disable_ssh_auth,
        }
    }

    #[must_use]
    pub fn patch(self, on: bool) -> ConfigPatch {
        let mut patch = ConfigPatch::default();
        let field = match self {
            Self::ConnectOnStartup => &mut patch.disable_auto_connect,
            Self::Notifications => &mut patch.disable_notifications,
            Self::ClientRoutes => &mut patch.disable_client_routes,
            Self::ServerRoutes => &mut patch.disable_server_routes,
            Self::Dns => &mut patch.disable_dns,
            Self::Ipv6 => &mut patch.disable_ipv6,
            Self::Monitor => &mut patch.network_monitor,
            Self::Lazy => &mut patch.lazy_connection_enabled,
            Self::BlockInbound => &mut patch.block_inbound,
            Self::BlockLan => &mut patch.block_lan_access,
            Self::Quantum => &mut patch.rosenpass_enabled,
            Self::Permissive => &mut patch.rosenpass_permissive,
            Self::SshServer => &mut patch.server_ssh_allowed,
            Self::SshRoot => &mut patch.enable_ssh_root,
            Self::Sftp => &mut patch.enable_ssh_sftp,
            Self::LocalForward => &mut patch.enable_ssh_local_port_forwarding,
            Self::RemoteForward => &mut patch.enable_ssh_remote_port_forwarding,
            Self::Jwt => &mut patch.disable_ssh_auth,
        };
        *field = Some(if self.inverted() { !on } else { on });
        patch
    }

    const fn inverted(self) -> bool {
        matches!(
            self,
            Self::ConnectOnStartup
                | Self::Notifications
                | Self::ClientRoutes
                | Self::ServerRoutes
                | Self::Dns
                | Self::Ipv6
                | Self::Jwt
        )
    }

    #[must_use]
    pub(crate) const fn mdm_key(self) -> Option<&'static str> {
        match self {
            Self::ConnectOnStartup => Some("disableAutoConnect"),
            Self::ClientRoutes => Some("disableClientRoutes"),
            Self::ServerRoutes => Some("disableServerRoutes"),
            Self::BlockInbound => Some("blockInbound"),
            Self::Quantum => Some("rosenpassEnabled"),
            Self::Permissive => Some("rosenpassPermissive"),
            Self::SshServer => Some("allowServerSSH"),
            Self::Lazy => Some("lazyConnection"),
            _ => None,
        }
    }

    #[must_use]
    pub const fn text(self) -> (T, T) {
        match self {
            Self::ConnectOnStartup => (T::ConnectOnStartup, T::ConnectOnStartupHint),
            Self::Notifications => (T::Notifications, T::NotificationsHint),
            Self::ClientRoutes => (T::ClientRoutes, T::ClientRoutesHint),
            Self::ServerRoutes => (T::ServerRoutes, T::ServerRoutesHint),
            Self::Dns => (T::DnsSwitch, T::DnsHint),
            Self::Ipv6 => (T::Ipv6, T::Ipv6Hint),
            Self::Monitor => (T::Monitor, T::MonitorHint),
            Self::Lazy => (T::Lazy, T::LazyHint),
            Self::BlockInbound => (T::BlockInbound, T::BlockInboundHint),
            Self::BlockLan => (T::BlockLan, T::BlockLanHint),
            Self::Quantum => (T::Quantum, T::QuantumHint),
            Self::Permissive => (T::Permissive, T::PermissiveHint),
            Self::SshServer => (T::SshServerSwitch, T::SshServerHint),
            Self::SshRoot => (T::SshRoot, T::SshRootHint),
            Self::Sftp => (T::Sftp, T::SftpHint),
            Self::LocalForward => (T::LocalForward, T::LocalForwardHint),
            Self::RemoteForward => (T::RemoteForward, T::RemoteForwardHint),
            Self::Jwt => (T::Jwt, T::JwtHint),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Busy {
    Connect,
    Disconnect,
    SignIn,
    Extend,
    Switch(String),
    Exit,
    Resource(String),
    Profile,
    Setting,
    Update,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirm {
    Remove(String),
    Deregister(String),
}

#[allow(clippy::struct_excessive_bools)] // One switch per debug bundle option, as the form shows them
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundleForm {
    pub anonymize: nb::Anonymize,
    pub system_info: bool,
    pub upload: bool,
    pub trace: bool,
    pub capture: bool,
    pub packets: bool,
}

impl Default for BundleForm {
    fn default() -> Self {
        Self {
            anonymize: nb::Anonymize::Default,
            system_info: true,
            upload: true,
            trace: false,
            capture: false,
            packets: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(u64, bool, Box<Snapshot>),
    Tab(Tab),
    Connect,
    Disconnect,
    SignIn,
    Extend,
    Done(Busy, Option<Result<(), Failure>>),
    ExitNode(Option<String>),
    Switch(String),
    ToggleAdd,
    NewName(String),
    NewUrl(String),
    NewSelfHosted(bool),
    AddProfile,
    StartRename(String),
    RenameInput(String),
    Rename,
    CancelRename,
    Ask(Confirm),
    CancelConfirm,
    Confirmed(Confirm),
    PeerQuery(String),
    PeerFilter(PeerFilter),
    ExpandPeer(String),
    ResourceQuery(String),
    ResourceFilter(ResourceFilter),
    Resource(String, bool),
    AllResources(bool),
    ExpandResource(String),
    Set(Setting, bool),
    SelfHosted(bool),
    ManagementUrl(String),
    SaveManagement,
    InterfaceInput(String),
    PortInput(String),
    MtuInput(String),
    SaveAdvanced,
    PskInput(Secret),
    TogglePsk,
    SavePsk,
    ClearPsk,
    JwtTtlInput(String),
    SaveJwtTtl,
    CopyText(String),
    BundleChanged(BundleForm),
    DurationInput(String),
    CreateBundle,
    CancelBundle,
    Bundled(Option<Result<nb::BundleResult, Failure>>),
    OpenFolder(String),
    InstallUpdate,
    OpenLink(&'static str),
}

#[allow(clippy::struct_excessive_bools)] // Independent page facts: load state and which inline forms are open
#[derive(Debug, Default)]
pub struct Page {
    snapshot: Snapshot,
    installed: bool,
    loaded: bool,
    loading: bool,
    generation: Generation,
    tab: Tab,
    busy: Option<Busy>,
    error: Option<String>,
    privilege: Option<(String, String)>,
    adding: bool,
    new_name: String,
    new_url: String,
    new_self_hosted: bool,
    renaming: Option<String>,
    rename_to: String,
    confirm: Option<Confirm>,
    peer_query: String,
    peer_filter: PeerFilter,
    expanded_peer: Option<String>,
    resource_query: String,
    resource_filter: ResourceFilter,
    expanded_resource: Option<String>,
    form: Option<String>,
    self_hosted: bool,
    management_url: String,
    interface: String,
    port: String,
    mtu: String,
    psk: Secret,
    psk_shown: bool,
    jwt_ttl: String,
    bundle: BundleForm,
    duration: String,
    bundling: Option<Arc<AtomicBool>>,
    bundle_result: Option<nb::BundleResult>,
}

const CLOUD_URL: &str = "https://api.netbird.io:443";

#[must_use]
pub(crate) fn is_cloud(url: &str) -> bool {
    url.is_empty() || url.trim_end_matches('/').trim_end_matches(":443") == CLOUD_URL.trim_end_matches(":443")
}

#[must_use]
pub(crate) fn valid_url(url: &str) -> bool {
    let url = url.trim();
    let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) else {
        return false;
    };
    let host = rest.split(['/', ':']).next().unwrap_or_default();
    !host.is_empty() && !rest.contains(char::is_whitespace)
}

#[must_use]
pub(crate) fn valid_interface(name: &str) -> bool {
    (1..=15).contains(&name.len()) && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

#[must_use]
pub(crate) fn parse_port(text: &str) -> Option<i64> {
    text.trim().parse::<u16>().ok().map(i64::from)
}

#[must_use]
pub(crate) fn parse_mtu(text: &str) -> Option<i64> {
    text.trim().parse::<i64>().ok().filter(|mtu| (576..=8192).contains(mtu))
}

#[must_use]
pub(crate) fn parse_minutes(text: &str) -> Option<u64> {
    text.trim().parse::<u64>().ok().filter(|minutes| (1..=30).contains(minutes))
}

#[must_use]
pub(crate) fn failure_text(lang: crate::i18n::Lang, failure: &Failure) -> Option<String> {
    let error = match failure {
        Failure::Declined => return Some(T::Declined.text(lang).to_string()),
        Failure::Privilege { summary, .. } => return Some(summary.clone()),
        Failure::Daemon(nb::Error::Cancelled) => return None,
        Failure::Daemon(error) => error,
    };
    let key = match error.reason() {
        nb::Reason::JwtClockSkew => T::ErrClockSkew,
        nb::Reason::JwtExpired => T::ErrTokenExpired,
        nb::Reason::JwtSignatureInvalid => T::ErrSignature,
        nb::Reason::SessionExpired => T::ErrSessionExpired,
        nb::Reason::InvalidSetupKey => T::ErrSetupKey,
        nb::Reason::PermissionDenied | nb::Reason::PrivilegeRequired => T::ErrPermission,
        nb::Reason::DaemonUnreachable => T::ErrUnreachable,
        nb::Reason::Unimplemented => T::ErrOutdated,
        nb::Reason::Unknown => {
            let detail = error.to_string();
            return Some(if detail.is_empty() { T::ErrUnknown.text(lang).to_string() } else { detail });
        }
    };
    Some(key.text(lang).to_string())
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] =
        &["netbird", "mesh", "wireguard", "peers", "exit node", "vpn", "вузли", "мережа", "тунель"];
    const POLLS: bool = true;

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        self.installed = nb::installed();
        self.refresh(true)
    }

    fn update(&mut self, msg: Msg, c: Ctx, _config: &mut ConfigStore) -> Task<Msg> {
        Page::update(self, msg, c)
    }

    fn view<'a>(&'a self, c: Ctx, _config: &'a ConfigStore) -> Element<'a, Msg> {
        Page::view(self, c)
    }

    fn tick(&mut self, _c: Ctx) -> Task<Msg> {
        if self.busy.is_some() || self.bundling.is_some() { Task::none() } else { self.refresh(false) }
    }

    fn sidebar_status(&self, c: Ctx) -> Option<String> {
        self.loaded.then(|| c.t(view::state_label(self.snapshot.status.state)).to_string())
    }
}

impl Page {
    fn refresh(&mut self, full: bool) -> Task<Msg> {
        if self.loading || !self.installed {
            return Task::none();
        }
        self.loading = true;
        let generation = self.generation.current();
        Task::perform(blocking(move || service::snapshot(full)), move |snapshot| {
            Msg::Loaded(generation, full, Box::new(snapshot.unwrap_or_default()))
        })
    }

    fn take_snapshot(&mut self, full: bool, mut snapshot: Snapshot) {
        let same_daemon = snapshot.status.state != nb::State::Unavailable;
        if !full && same_daemon {
            snapshot.compatible = self.snapshot.compatible;
            if snapshot.status.state.is_connected() {
                snapshot.forwarding = std::mem::take(&mut self.snapshot.forwarding);
            }
        }
        self.snapshot = snapshot;
    }

    fn start(&mut self, busy: Busy, job: impl FnOnce() -> Result<(), Failure> + Send + 'static) -> Task<Msg> {
        self.busy = Some(busy.clone());
        self.error = None;
        self.privilege = None;
        self.generation.bump();
        self.loading = false;
        Task::perform(blocking(job), move |result| Msg::Done(busy, result))
    }

    fn save(&mut self, patch: ConfigPatch) -> Task<Msg> {
        self.start(Busy::Setting, move || service::set_config(patch))
    }

    fn fill_form(&mut self) {
        let Some(config) = &self.snapshot.config else {
            return;
        };
        let key = format!("{}:{}", self.snapshot.active.id, config.management_url);
        if self.form.as_deref() == Some(key.as_str()) {
            return;
        }
        self.form = Some(key);
        self.self_hosted = !is_cloud(&config.management_url);
        self.management_url = if self.self_hosted { config.management_url.clone() } else { String::new() };
        self.interface.clone_from(&config.interface_name);
        self.port = config.wireguard_port.to_string();
        self.mtu = config.mtu.to_string();
        self.jwt_ttl = config.ssh_jwt_cache_ttl.to_string();
        self.psk = Secret::default();
    }

    #[must_use]
    pub const fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    #[must_use]
    pub const fn tab(&self) -> Tab {
        self.tab
    }

    pub(crate) fn show_tab(&mut self, name: &str) {
        self.tab = match name.trim().to_ascii_lowercase().as_str() {
            "peers" => Tab::Peers,
            "resources" | "networks" => Tab::Resources,
            "settings" => Tab::Settings,
            "troubleshoot" | "debug" => Tab::Troubleshoot,
            "about" | "update" => Tab::About,
            _ => Tab::Overview,
        };
    }

    fn done(&mut self, lang: crate::i18n::Lang, busy: &Busy, result: Option<Result<(), Failure>>) -> Task<Msg> {
        if self.busy.as_ref() == Some(busy) {
            self.busy = None;
        }
        match result {
            Some(Err(Failure::Privilege { summary, command })) => self.privilege = Some((summary, command)),
            Some(Err(failure)) => self.error = failure_text(lang, &failure),
            Some(Ok(())) => {
                if *busy == Busy::Profile {
                    self.adding = false;
                    self.new_name.clear();
                    self.new_url.clear();
                    self.renaming = None;
                }
                if *busy == Busy::Setting {
                    self.form = None;
                }
            }
            None => {}
        }
        self.generation.bump();
        self.loading = false;
        self.refresh(false)
    }

    #[allow(clippy::too_many_lines)] // One arm per message keeps the panel's state machine in one place
    pub fn update(&mut self, msg: Msg, c: Ctx) -> Task<Msg> {
        match msg {
            Msg::Loaded(generation, ..) if !self.generation.is_current(generation) => {}
            Msg::Loaded(_, full, snapshot) => {
                self.take_snapshot(full, *snapshot);
                self.loading = false;
                self.loaded = true;
                self.fill_form();
            }
            Msg::Tab(tab) => {
                self.tab = tab;
                return self.refresh(tab == Tab::Overview);
            }
            Msg::Connect => return self.start(Busy::Connect, service::up),
            Msg::Disconnect => return self.start(Busy::Disconnect, service::down),
            Msg::SignIn => return self.start(Busy::SignIn, service::sign_in),
            Msg::Extend => return self.start(Busy::Extend, service::extend),
            Msg::Done(busy, result) => return self.done(c.lang, &busy, result),
            Msg::ExitNode(target) => {
                let exits = nb::exit_nodes(&self.snapshot.networks);
                let current: Vec<String> =
                    exits.iter().filter(|exit| exit.selected).map(|exit| exit.id.clone()).collect();
                return self.start(Busy::Exit, move || match target {
                    Some(id) => service::select(vec![id], true),
                    None => service::select(current, false),
                });
            }
            Msg::Switch(id) => return self.start(Busy::Switch(id.clone()), move || service::switch_profile(&id)),
            Msg::ToggleAdd => self.adding = !self.adding,
            Msg::NewName(name) => self.new_name = name,
            Msg::NewUrl(url) => self.new_url = url,
            Msg::NewSelfHosted(on) => self.new_self_hosted = on,
            Msg::AddProfile => {
                let name = self.new_name.trim().to_string();
                let url = self.new_self_hosted.then(|| self.new_url.trim().to_string());
                if name.is_empty() || url.as_deref().is_some_and(|url| !valid_url(url)) {
                    return Task::none();
                }
                return self.start(Busy::Profile, move || service::add_profile(&name, url.as_deref()));
            }
            Msg::StartRename(id) => {
                self.rename_to =
                    self.snapshot.profiles.iter().find(|p| p.id == id).map(|p| p.name.clone()).unwrap_or_default();
                self.renaming = Some(id);
            }
            Msg::RenameInput(name) => self.rename_to = name,
            Msg::Rename => {
                let (Some(id), name) = (self.renaming.clone(), self.rename_to.trim().to_string()) else {
                    return Task::none();
                };
                if name.is_empty() {
                    return Task::none();
                }
                return self.start(Busy::Profile, move || service::rename_profile(&id, &name));
            }
            Msg::CancelRename => self.renaming = None,
            Msg::Ask(confirm) => self.confirm = Some(confirm),
            Msg::CancelConfirm => self.confirm = None,
            Msg::Confirmed(confirm) => {
                self.confirm = None;
                return self.start(Busy::Profile, move || match confirm {
                    Confirm::Remove(id) => service::remove_profile(&id),
                    Confirm::Deregister(id) => service::deregister(&id),
                });
            }
            Msg::PeerQuery(query) => self.peer_query = query,
            Msg::PeerFilter(filter) => self.peer_filter = filter,
            Msg::ExpandPeer(key) => {
                self.expanded_peer = if self.expanded_peer.as_deref() == Some(key.as_str()) { None } else { Some(key) };
            }
            Msg::ResourceQuery(query) => self.resource_query = query,
            Msg::ResourceFilter(filter) => self.resource_filter = filter,
            Msg::Resource(id, on) => {
                if let Some(network) = self.snapshot.networks.iter_mut().find(|network| network.id == id) {
                    network.selected = on;
                }
                return self.start(Busy::Resource(id.clone()), move || service::select(vec![id], on));
            }
            Msg::AllResources(on) => {
                let ids: Vec<String> = view::filtered_resources(self).into_iter().map(|network| network.id).collect();
                let everything = ids.len() == nb::resources(&self.snapshot.networks).len();
                return self.start(Busy::Exit, move || {
                    if everything { service::select_all(on) } else { service::select(ids, on) }
                });
            }
            Msg::ExpandResource(id) => {
                self.expanded_resource =
                    if self.expanded_resource.as_deref() == Some(id.as_str()) { None } else { Some(id) };
            }
            Msg::Set(setting, on) => return self.save(setting.patch(on)),
            Msg::SelfHosted(on) => {
                self.self_hosted = on;
                if !on {
                    return self
                        .save(ConfigPatch { management_url: Some(CLOUD_URL.to_string()), ..ConfigPatch::default() });
                }
            }
            Msg::ManagementUrl(url) => self.management_url = url,
            Msg::SaveManagement => {
                let url = self.management_url.trim().to_string();
                if !valid_url(&url) {
                    return Task::none();
                }
                return self.save(ConfigPatch { management_url: Some(url), ..ConfigPatch::default() });
            }
            Msg::InterfaceInput(text) => self.interface = text,
            Msg::PortInput(text) => self.port = text,
            Msg::MtuInput(text) => self.mtu = text,
            Msg::SaveAdvanced => {
                let (Some(port), Some(mtu)) = (parse_port(&self.port), parse_mtu(&self.mtu)) else {
                    return Task::none();
                };
                if !valid_interface(self.interface.trim()) {
                    return Task::none();
                }
                return self.save(ConfigPatch {
                    interface_name: Some(self.interface.trim().to_string()),
                    wireguard_port: Some(port),
                    mtu: Some(mtu),
                    ..ConfigPatch::default()
                });
            }
            Msg::PskInput(text) => self.psk = text,
            Msg::TogglePsk => self.psk_shown = !self.psk_shown,
            Msg::SavePsk => {
                let Secret(key) = std::mem::take(&mut self.psk);
                if key.trim().is_empty() {
                    return Task::none();
                }
                return self
                    .save(ConfigPatch { pre_shared_key: Some(key.trim().to_string()), ..ConfigPatch::default() });
            }
            Msg::ClearPsk => {
                self.psk = Secret::default();
                return self.save(ConfigPatch { pre_shared_key: Some(String::new()), ..ConfigPatch::default() });
            }
            Msg::JwtTtlInput(text) => self.jwt_ttl = text,
            Msg::SaveJwtTtl => {
                let Ok(ttl) = self.jwt_ttl.trim().parse::<i32>() else {
                    return Task::none();
                };
                if ttl < 0 {
                    return Task::none();
                }
                return self.save(ConfigPatch { ssh_jwt_cache_ttl: Some(ttl), ..ConfigPatch::default() });
            }
            Msg::CopyText(text) => return iced::clipboard::write(text),
            Msg::BundleChanged(form) => self.bundle = form,
            Msg::DurationInput(text) => self.duration = text,
            Msg::CreateBundle => return self.create_bundle(),
            Msg::CancelBundle => {
                if let Some(cancel) = &self.bundling {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            Msg::Bundled(result) => {
                self.bundling = None;
                match result {
                    Some(Ok(result)) => self.bundle_result = Some(result),
                    Some(Err(failure)) => self.error = failure_text(c.lang, &failure),
                    None => {}
                }
            }
            Msg::OpenFolder(path) => service::open_folder(&path),
            Msg::InstallUpdate => return self.start(Busy::Update, service::trigger_update),
            Msg::OpenLink(url) => service::open_uri(url),
        }
        Task::none()
    }

    fn create_bundle(&mut self) -> Task<Msg> {
        if self.bundling.is_some() {
            return Task::none();
        }
        let capture = if self.bundle.capture {
            let Some(minutes) = parse_minutes(&self.duration).or(self.duration.trim().is_empty().then_some(1)) else {
                return Task::none();
            };
            Some(Duration::from_secs(minutes * 60))
        } else {
            None
        };
        let form = self.bundle;
        let job = BundleJob {
            options: nb::BundleOptions {
                anonymize: form.anonymize,
                system_info: form.system_info,
                upload: form.upload,
                log_file_count: 1,
            },
            trace: form.trace,
            packets: form.packets && capture.is_some(),
            capture,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.bundling = Some(Arc::clone(&cancel));
        self.bundle_result = None;
        self.error = None;
        let reconnect = capture.is_some() && self.snapshot.status.state.is_up();
        Task::perform(
            blocking(move || {
                if reconnect {
                    service::down()?;
                    service::up()?;
                }
                service::create_bundle(job, &cancel)
            }),
            Msg::Bundled,
        )
    }

    #[must_use]
    pub(crate) fn capturing(&self) -> bool {
        self.bundling.is_some() && self.bundle.capture
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switches_that_read_as_enable_store_the_daemons_disable_flag() {
        let config = nb::Config { disable_dns: true, network_monitor: true, ..nb::Config::default() };
        assert!(!Setting::Dns.value(&config) && Setting::Monitor.value(&config));
        assert_eq!(Setting::Dns.patch(true).disable_dns, Some(false));
        assert_eq!(Setting::Monitor.patch(false).network_monitor, Some(false));
        assert_eq!(Setting::Jwt.patch(false).disable_ssh_auth, Some(true));
        assert_eq!(Setting::SshServer.patch(true).server_ssh_allowed, Some(true));
        let changed = |setting: Setting| setting.patch(true) != ConfigPatch::default();
        for setting in [Setting::ConnectOnStartup, Setting::Lazy, Setting::Sftp, Setting::RemoteForward] {
            assert!(changed(setting), "{setting:?}");
        }
    }

    #[test]
    fn the_periodic_refresh_keeps_what_only_a_full_load_reads() {
        let connected = nb::Status { state: nb::State::Connected, ..nb::Status::default() };
        let rule = nb::ForwardingRule {
            protocol: "tcp".into(),
            destination: nb::Ports::One(80),
            translated_address: "10.0.0.2".into(),
            translated_hostname: String::new(),
            translated_port: nb::Ports::One(8080),
        };
        let mut page = Page::default();
        page.take_snapshot(
            true,
            Snapshot { status: connected.clone(), compatible: false, forwarding: vec![rule], ..Snapshot::default() },
        );
        page.take_snapshot(false, Snapshot { status: connected, compatible: true, ..Snapshot::default() });
        assert!(!page.snapshot.compatible, "the outdated warning outlives the 4 s refresh");
        assert_eq!(page.snapshot.forwarding.len(), 1);
        page.take_snapshot(false, Snapshot { status: nb::Status::unavailable(), ..Snapshot::default() });
        assert!(page.snapshot.forwarding.is_empty(), "a stopped daemon forgets what it reported");
    }

    #[test]
    fn mdm_keys_match_the_names_the_daemon_reports() {
        assert_eq!(Setting::SshServer.mdm_key(), Some("allowServerSSH"));
        assert_eq!(Setting::ConnectOnStartup.mdm_key(), Some("disableAutoConnect"));
        assert_eq!(Setting::Dns.mdm_key(), None);
    }

    #[test]
    fn advanced_values_are_checked_against_the_limits_netbird_ui_uses() {
        assert!(valid_interface("wt0") && valid_interface("netbird-0.a_b"));
        assert!(!valid_interface("") && !valid_interface("a-very-long-interface") && !valid_interface("wt 0"));
        assert_eq!(parse_port("51820"), Some(51820));
        assert_eq!(parse_port("0"), Some(0));
        assert_eq!(parse_port("70000"), None);
        assert_eq!(parse_mtu("1280"), Some(1280));
        assert_eq!(parse_mtu("575"), None);
        assert_eq!(parse_mtu("8193"), None);
        assert_eq!(parse_minutes("30"), Some(30));
        assert_eq!(parse_minutes("0"), None);
        assert_eq!(parse_minutes("31"), None);
    }

    #[test]
    fn management_urls_tell_cloud_from_self_hosted() {
        assert!(is_cloud("https://api.netbird.io:443") && is_cloud("https://api.netbird.io") && is_cloud(""));
        assert!(!is_cloud("https://nb.example.com:443"));
        assert!(valid_url("https://nb.example.com:443") && valid_url("http://10.0.0.1"));
        assert!(!valid_url("nb.example.com") && !valid_url("https://") && !valid_url("https://a b"));
    }

    #[test]
    fn cancelled_operations_say_nothing_and_refusals_say_why() {
        use crate::i18n::Lang;
        assert_eq!(failure_text(Lang::En, &Failure::Daemon(nb::Error::Cancelled)), None);
        assert_eq!(failure_text(Lang::En, &Failure::Declined).as_deref(), Some(T::Declined.en()));
        let offline = failure_text(Lang::Uk, &Failure::Daemon(nb::Error::Unavailable));
        assert_eq!(offline.as_deref(), Some(T::ErrUnreachable.uk()));
        let raw = Failure::Daemon(nb::Error::Daemon { code: nb::Code::Unknown, message: "profile exists".into() });
        assert_eq!(failure_text(Lang::En, &raw).as_deref(), Some("profile exists"));
    }
}
