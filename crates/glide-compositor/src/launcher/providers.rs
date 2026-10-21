use std::collections::HashMap;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use zbus::zvariant::OwnedValue;

const INTERFACE: &str = "org.gnome.Shell.SearchProvider2";
const QUERY_TIMEOUT: Duration = Duration::from_millis(600);
const PER_PROVIDER: usize = 3;
pub const MIN_QUERY: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    pub desktop_id: String,
    pub bus: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub provider: Provider,
    pub desktop_id: String,
    pub id: String,
    pub name: String,
    pub description: String,
}

#[must_use]
pub fn parse_ini(text: &str) -> Option<Provider> {
    let mut in_section = false;
    let mut keys: HashMap<&str, &str> = HashMap::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_section = line == "[Shell Search Provider]";
        } else if in_section && let Some((key, value)) = line.split_once('=') {
            keys.insert(key.trim(), value.trim());
        }
    }
    if keys.get("Version").is_some_and(|version| *version != "2")
        || keys.get("DefaultDisabled").is_some_and(|value| *value == "true")
    {
        return None;
    }
    let desktop_id = keys.get("DesktopId")?.trim_end_matches(".desktop").to_string();
    Some(Provider {
        desktop_id,
        bus: (*keys.get("BusName")?).to_string(),
        path: (*keys.get("ObjectPath")?).to_string(),
    })
}

#[must_use]
pub fn discover() -> Vec<Provider> {
    let mut found: Vec<Provider> = Vec::new();
    for dir in glide_config::data_dirs() {
        let Ok(entries) = std::fs::read_dir(dir.join("gnome-shell/search-providers")) else {
            continue;
        };
        let mut files: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
        files.sort();
        for file in files.iter().filter(|file| file.extension().is_some_and(|ext| ext == "ini")) {
            let Some(provider) = std::fs::read_to_string(file).ok().as_deref().and_then(parse_ini) else {
                continue;
            };
            if !found.iter().any(|known| known.bus == provider.bus && known.path == provider.path) {
                found.push(provider);
            }
        }
    }
    found
}

#[must_use]
pub fn terms(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

fn text(meta: &HashMap<String, OwnedValue>, key: &str) -> String {
    meta.get(key).and_then(|value| String::try_from(value.try_clone().ok()?).ok()).unwrap_or_default()
}

fn ask(connection: &zbus::blocking::Connection, provider: &Provider, terms: &[String]) -> zbus::Result<Vec<Found>> {
    let reply = connection.call_method(
        Some(provider.bus.as_str()),
        provider.path.as_str(),
        Some(INTERFACE),
        "GetInitialResultSet",
        &(terms,),
    )?;
    let ids: Vec<String> = reply.body().deserialize()?;
    let ids: Vec<String> = ids.into_iter().take(PER_PROVIDER).collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let reply = connection.call_method(
        Some(provider.bus.as_str()),
        provider.path.as_str(),
        Some(INTERFACE),
        "GetResultMetas",
        &(ids,),
    )?;
    let metas: Vec<HashMap<String, OwnedValue>> = reply.body().deserialize()?;
    Ok(metas
        .iter()
        .map(|meta| Found {
            provider: provider.clone(),
            desktop_id: provider.desktop_id.clone(),
            id: text(meta, "id"),
            name: text(meta, "name"),
            description: text(meta, "description"),
        })
        .filter(|found| !found.id.is_empty() && !found.name.is_empty())
        .collect())
}

enum Job {
    Query { generation: u64, terms: Vec<String> },
    Activate { provider: Provider, id: String, terms: Vec<String> },
}

const REDISCOVER: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct Handle {
    jobs: mpsc::Sender<Job>,
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("providers::Handle")
    }
}

impl Handle {
    pub fn query(&self, generation: u64, terms: Vec<String>) {
        let _ = self.jobs.send(Job::Query { generation, terms });
    }

    pub fn activate(&self, found: &Found, terms: Vec<String>) {
        let _ = self.jobs.send(Job::Activate { provider: found.provider.clone(), id: found.id.clone(), terms });
    }
}

fn run_query(connection: &zbus::blocking::Connection, providers: &Arc<Vec<Provider>>, terms: &[String]) -> Vec<Found> {
    let (sender, receiver) = mpsc::channel();
    for index in 0..providers.len() {
        let (connection, providers, terms, sender) =
            (connection.clone(), Arc::clone(providers), terms.to_vec(), sender.clone());
        let _ = std::thread::Builder::new().name("glide-search-provider".into()).spawn(move || {
            let found = ask(&connection, &providers[index], &terms).unwrap_or_default();
            let _ = sender.send((index, found));
        });
    }
    drop(sender);
    let deadline = Instant::now() + QUERY_TIMEOUT;
    let mut answers: Vec<(usize, Vec<Found>)> = Vec::new();
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match receiver.recv_timeout(left) {
            Ok(answer) => answers.push(answer),
            Err(_) => break,
        }
    }
    answers.sort_by_key(|(index, _)| *index);
    answers.into_iter().flat_map(|(_, found)| found).collect()
}

pub fn start(sink: impl Fn(u64, Vec<Found>) + Send + 'static) -> Option<Handle> {
    let connection = zbus::blocking::Connection::session().ok()?;
    let (jobs, inbox) = mpsc::channel::<Job>();
    std::thread::Builder::new()
        .name("glide-search-providers".into())
        .spawn(move || {
            let mut providers = Arc::new(Vec::new());
            let mut discovered: Option<Instant> = None;
            while let Ok(mut job) = inbox.recv() {
                while let Job::Query { .. } = job {
                    match inbox.try_recv() {
                        Ok(next) => job = next,
                        Err(_) => break,
                    }
                }
                match job {
                    Job::Query { generation, terms } => {
                        if discovered.is_none_or(|at| at.elapsed() >= REDISCOVER) {
                            providers = Arc::new(discover());
                            discovered = Some(Instant::now());
                        }
                        sink(generation, run_query(&connection, &providers, &terms));
                    }
                    Job::Activate { provider, id, terms } => {
                        let _ = connection.call_method(
                            Some(provider.bus.as_str()),
                            provider.path.as_str(),
                            Some(INTERFACE),
                            "ActivateResult",
                            &(id, terms, 0_u32),
                        );
                    }
                }
            }
        })
        .ok()?;
    Some(Handle { jobs })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_provider_file_names_its_bus_path_and_app() {
        let ini = "[Shell Search Provider]\nDesktopId=org.gnome.Nautilus.desktop\nBusName=org.gnome.Nautilus\nObjectPath=/org/gnome/Nautilus/SearchProvider\nVersion=2\n";
        assert_eq!(
            parse_ini(ini),
            Some(Provider {
                desktop_id: "org.gnome.Nautilus".into(),
                bus: "org.gnome.Nautilus".into(),
                path: "/org/gnome/Nautilus/SearchProvider".into(),
            })
        );
        assert_eq!(parse_ini(&format!("{ini}DefaultDisabled=true\n")), None);
        assert_eq!(parse_ini("[Shell Search Provider]\nBusName=a\n"), None);
    }

    #[test]
    fn terms_are_lowercase_words() {
        assert_eq!(terms("  Hello  World "), ["hello", "world"]);
    }
}
