use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use zbus::zvariant::OwnedValue;

const INTERFACE: &str = "org.gnome.Shell.SearchProvider2";
const QUERY_TIMEOUT: Duration = Duration::from_millis(600);
const METHOD_TIMEOUT: Duration = Duration::from_millis(800);
const PER_PROVIDER: usize = 3;
pub const MIN_QUERY: usize = 2;

pub use glide_config::search_providers::{Provider, discover};

static GENERATION: AtomicU64 = AtomicU64::new(0);

#[must_use]
pub fn next_generation() -> u64 {
    GENERATION.fetch_add(1, Ordering::Relaxed) + 1
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

fn connect(builder: zbus::blocking::connection::Builder<'_>) -> zbus::Result<zbus::blocking::Connection> {
    builder.method_timeout(METHOD_TIMEOUT).build()
}

fn activate(connection: zbus::blocking::Connection, provider: Provider, id: String, terms: Vec<String>) {
    let _ = std::thread::Builder::new().name("glide-search-activate".into()).spawn(move || {
        let _ = connection.call_method(
            Some(provider.bus.as_str()),
            provider.path.as_str(),
            Some(INTERFACE),
            "ActivateResult",
            &(id, terms, 0_u32),
        );
    });
}

fn spawn(
    connection: zbus::blocking::Connection, find: impl Fn() -> Vec<Provider> + Send + 'static,
    sink: impl Fn(u64, Vec<Found>) + Send + 'static,
) -> Option<Handle> {
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
                            providers = Arc::new(find());
                            discovered = Some(Instant::now());
                        }
                        sink(generation, run_query(&connection, &providers, &terms));
                    }
                    Job::Activate { provider, id, terms } => activate(connection.clone(), provider, id, terms),
                }
            }
        })
        .ok()?;
    Some(Handle { jobs })
}

pub fn start(sink: impl Fn(u64, Vec<Found>) + Send + 'static) -> Option<Handle> {
    let connection = connect(zbus::blocking::connection::Builder::session().ok()?).ok()?;
    spawn(connection, discover, sink)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, Stdio};

    #[test]
    fn terms_are_lowercase_words() {
        assert_eq!(terms("  Hello  World "), ["hello", "world"]);
    }

    #[test]
    fn generations_are_unique_and_never_the_idle_zero() {
        let first = next_generation();
        let second = next_generation();
        assert!(first >= 1, "a launcher that never asked holds generation 0");
        assert!(second > first);
    }

    struct PrivateBus {
        child: Child,
        address: String,
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn private_bus() -> Option<PrivateBus> {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .inspect_err(|err| eprintln!("no dbus-daemon ({err}); skipping"))
            .ok()?;
        let mut address = String::new();
        BufReader::new(child.stdout.take()?).read_line(&mut address).ok()?;
        let bus = PrivateBus { child, address: address.trim().to_string() };
        (!bus.address.is_empty()).then_some(bus)
    }

    struct Slow(Duration);

    #[allow(clippy::unused_self)] // zbus requires `&self` on interface methods even when they are stateless.
    #[zbus::interface(name = "org.gnome.Shell.SearchProvider2")]
    impl Slow {
        fn get_initial_result_set(&self, terms: Vec<String>) -> Vec<String> {
            std::thread::sleep(self.0);
            vec![terms.into_iter().collect()]
        }

        fn get_result_metas(&self, ids: Vec<String>) -> Vec<HashMap<String, OwnedValue>> {
            ids.into_iter()
                .map(|id| {
                    let text = |text: &str| OwnedValue::try_from(zbus::zvariant::Value::from(text)).expect("a string");
                    HashMap::from([("id".to_string(), text(&id)), ("name".to_string(), text("Result"))])
                })
                .collect()
        }

        fn activate_result(&self, id: String, terms: Vec<String>, timestamp: u32) {
            let _ = (id, terms, timestamp);
            std::thread::sleep(self.0);
        }
    }

    const BUS_PATH: &str = "/org/glide/test/provider";

    struct Served {
        _bus: PrivateBus,
        _service: zbus::blocking::Connection,
        client: zbus::blocking::Connection,
        provider: Provider,
        address: String,
    }

    fn serve(delay: Duration) -> Option<Served> {
        let bus = private_bus()?;
        let name = format!("org.glide.test.Provider{}", std::process::id());
        let service = zbus::blocking::connection::Builder::address(bus.address.as_str())
            .ok()?
            .name(name.as_str())
            .ok()?
            .serve_at(BUS_PATH, Slow(delay))
            .ok()?
            .build()
            .ok()?;
        let client = connect(zbus::blocking::connection::Builder::address(bus.address.as_str()).ok()?).ok()?;
        let provider = Provider { desktop_id: "test.desktop".into(), bus: name, path: BUS_PATH.into() };
        let address = bus.address.clone();
        Some(Served { _bus: bus, _service: service, client, provider, address })
    }

    #[test]
    fn a_provider_that_answers_in_time_still_yields_its_results() {
        let Some(served) = serve(Duration::ZERO) else {
            return;
        };
        let found = ask(&served.client, &served.provider, &terms("res")).expect("the provider answers");
        let names: Vec<(&str, &str)> = found.iter().map(|found| (found.id.as_str(), found.name.as_str())).collect();
        assert_eq!(names, [("res", "Result")]);
    }

    #[test]
    fn a_provider_that_never_answers_fails_the_call_instead_of_parking_its_thread() {
        let Some(served) = serve(Duration::from_secs(5)) else {
            return;
        };
        let began = Instant::now();
        let answer = ask(&served.client, &served.provider, &terms("res"));
        assert!(answer.is_err(), "a call that outlives the method timeout is an error");
        assert!(
            began.elapsed() < Duration::from_secs(3),
            "the call must be abandoned near the method timeout, took {:?}",
            began.elapsed()
        );
    }

    #[test]
    fn a_hung_activation_does_not_stop_the_query_worker() {
        let Some(served) = serve(Duration::from_secs(5)) else {
            return;
        };
        let connection = connect(
            zbus::blocking::connection::Builder::address(served.address.as_str()).expect("the private bus address"),
        )
        .expect("a client connection");
        let provider = served.provider;
        let (answers, received) = mpsc::channel();
        let handle = spawn(connection, Vec::new, move |generation, found| {
            let _ = answers.send((generation, found));
        })
        .expect("the worker starts");

        let found = Found {
            desktop_id: provider.desktop_id.clone(),
            provider,
            id: "one".into(),
            name: "Result".into(),
            description: String::new(),
        };
        let began = Instant::now();
        handle.activate(&found, terms("res"));
        handle.query(7, terms("res"));
        let (generation, _) = received
            .recv_timeout(Duration::from_secs(3))
            .expect("the query is answered while ActivateResult is still hanging");
        assert_eq!(generation, 7);
        assert!(
            began.elapsed() < METHOD_TIMEOUT / 2,
            "the worker must not wait for ActivateResult, not even until its timeout, took {:?}",
            began.elapsed()
        );
    }
}
