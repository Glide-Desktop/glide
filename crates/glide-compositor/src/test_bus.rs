use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
};

pub struct Daemon {
    child: Child,
    pub address: String,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Daemon {
    pub fn builder(&self) -> zbus::blocking::connection::Builder<'_> {
        zbus::blocking::connection::Builder::address(self.address.as_str()).expect("a private bus address")
    }

    pub fn client(&self) -> zbus::blocking::Connection {
        self.builder().build().expect("a client on the private bus")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Member {
    Method { name: String, inputs: Vec<(String, String)>, outputs: Vec<String> },
    Property { name: String, kind: String, access: String },
    Signal { name: String, args: Vec<String> },
}

fn attribute(node: roxmltree::Node<'_, '_>, name: &str) -> String {
    node.attribute(name).unwrap_or_default().to_string()
}

fn args<'a>(node: roxmltree::Node<'a, 'a>) -> impl Iterator<Item = roxmltree::Node<'a, 'a>> {
    node.children().filter(|child| child.has_tag_name("arg"))
}

#[must_use]
pub fn members(xml: &str, interface: &str) -> std::collections::BTreeSet<Member> {
    let options = roxmltree::ParsingOptions { allow_dtd: true, ..roxmltree::ParsingOptions::default() };
    let document = roxmltree::Document::parse_with_options(xml, options).expect("introspection XML parses");
    let Some(node) =
        document.descendants().find(|node| node.has_tag_name("interface") && node.attribute("name") == Some(interface))
    else {
        panic!("{interface} is not in the XML");
    };
    node.children()
        .filter(roxmltree::Node::is_element)
        .filter_map(|child| match child.tag_name().name() {
            "method" => {
                let direction = |arg: &roxmltree::Node<'_, '_>| arg.attribute("direction").unwrap_or("in").to_string();
                Some(Member::Method {
                    name: attribute(child, "name"),
                    inputs: args(child)
                        .filter(|arg| direction(arg) == "in")
                        .map(|arg| (attribute(arg, "name"), attribute(arg, "type")))
                        .collect(),
                    outputs: args(child)
                        .filter(|arg| direction(arg) == "out")
                        .map(|arg| attribute(arg, "type"))
                        .collect(),
                })
            }
            "property" => Some(Member::Property {
                name: attribute(child, "name"),
                kind: attribute(child, "type"),
                access: attribute(child, "access"),
            }),
            "signal" => Some(Member::Signal {
                name: attribute(child, "name"),
                args: args(child).map(|arg| attribute(arg, "type")).collect(),
            }),
            _ => None,
        })
        .collect()
}

pub fn introspect(bus: &Daemon, destination: &str, path: &str) -> String {
    let reply = bus
        .client()
        .call_method(Some(destination), path, Some("org.freedesktop.DBus.Introspectable"), "Introspect", &())
        .expect("introspection");
    reply.body().deserialize::<String>().expect("introspection XML")
}

#[must_use]
pub fn private_bus() -> Option<Daemon> {
    let mut child = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .inspect_err(|err| eprintln!("no dbus-daemon ({err}); skipping"))
        .ok()?;
    let mut address = String::new();
    BufReader::new(child.stdout.take()?).read_line(&mut address).ok()?;
    let daemon = Daemon { child, address: address.trim().to_string() };
    (!daemon.address.is_empty()).then_some(daemon)
}
