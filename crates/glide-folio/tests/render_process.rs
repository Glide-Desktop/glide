use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};

fn pdf(path: &std::path::Path) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
    let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font } });
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 18.into()]),
            Operation::new("Td", vec![72.into(), 700.into()]),
            Operation::new("Tj", vec![Object::string_literal("Rendered elsewhere")]),
            Operation::new("ET", vec![]),
        ],
    };
    let stream = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => stream, "Resources" => resources,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    doc.save(path).unwrap();
}

#[test]
fn the_renderer_runs_as_its_own_process() {
    let dir = std::env::temp_dir().join(format!("glide-folio-process-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("one.pdf");
    pdf(&path);
    let mut child = Command::new(env!("CARGO_BIN_EXE_glide-folio"))
        .arg(glide_folio::worker::FLAG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let open = format!("{{\"Open\":{{\"path\":{},\"password\":null}}}}\n", serde_json::to_string(&path).unwrap());
    input.write_all(open.as_bytes()).unwrap();
    input.write_all(b"{\"Render\":{\"page\":0,\"scale\":0.25}}\n{\"Text\":{\"page\":0}}\n").unwrap();
    input.flush().unwrap();
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert!(line.starts_with("{\"Opened\":{\"Ok\""), "{line}");
    line.clear();
    output.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "{\"Image\":{\"w\":153,\"h\":198}}");
    let mut pixels = vec![0; 153 * 198 * 4];
    output.read_exact(&mut pixels).unwrap();
    assert!(pixels.chunks_exact(4).any(|p| p[0] < 128), "the page has ink");
    line.clear();
    output.read_line(&mut line).unwrap();
    assert!(line.contains("Rendered") && line.contains("elsewhere"), "{line}");
    drop(input);
    assert!(child.wait().unwrap().success(), "the renderer exits when its input closes");
    let _ = std::fs::remove_dir_all(dir);
}
