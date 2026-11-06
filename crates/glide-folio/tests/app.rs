use std::path::{Path, PathBuf};

use glide_folio::App;
use glide_folio::app::{FindMsg, Lib, Message, NoteMsg, Org, Read, Screen, Slot};
use glide_folio::reader::Tool;
use glide_folio::worker::Session;
use glide_sdk::{Harness, Lang, Look};
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};

fn pdf(path: &Path, pages: usize) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
    let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font } });
    let mut kids = Vec::new();
    for n in 0..pages {
        let line =
            if n == 1 { "Workspaces switch with three fingers".to_string() } else { format!("Chapter {}", n + 1) };
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 18.into()]),
                Operation::new("Td", vec![72.into(), 700.into()]),
                Operation::new("Tj", vec![Object::string_literal(line)]),
                Operation::new("ET", vec![]),
            ],
        };
        let stream = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => stream, "Resources" => resources,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        kids.push(page.into());
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => pages as i64 }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    doc.save(path).unwrap();
}

fn isolate() -> PathBuf {
    let root = std::env::temp_dir().join(format!("glide-folio-app-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in ["state", "config", "cache", "data", "docs"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    // SAFETY: this binary holds a single test, and the variables are set before the harness or any worker thread reads the environment.
    unsafe {
        std::env::set_var("XDG_STATE_HOME", root.join("state"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_CACHE_HOME", root.join("cache"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    root
}

fn open(app: &mut Harness<App>, path: &Path) {
    app.message(Message::OpenPath(path.to_path_buf()));
    let generation = app.app().generation;
    let sessions = Ok((Session::local(path, None).unwrap(), Session::local(path, None).unwrap()));
    app.message(Message::Opened(generation, path.to_path_buf(), Slot::new(sessions)));
}

#[test]
fn reading_a_document_end_to_end() {
    let root = isolate();
    let path = root.join("docs/Manual.pdf");
    pdf(&path, 4);
    glide_sdk::theme::motion::set_scale(0.0);
    let look = Look { language: Some(Lang::Uk), motion_scale: 0.0, ..Look::default() };
    let mut app = Harness::<App>::new(glide_folio::APP_ID, look, (1280.0, 800.0).into());

    open(&mut app, &path);
    assert!(matches!(app.app().screen, Screen::Reading), "the document opens into the reader");
    assert_eq!(app.app().doc.as_ref().unwrap().count(), 4);

    let wide = app.app().doc.as_ref().unwrap().viewport.width;
    app.message(Message::Read(Read::ToggleSide));
    let narrow = app.app().doc.as_ref().unwrap().viewport.width;
    assert!((wide - narrow).abs() > 200.0, "toggling the sidebar resizes the page area: {wide} → {narrow}");
    let widest = 612.0 * app.app().doc.as_ref().unwrap().zoom();
    assert!(
        glide_folio::view::content_width(narrow, widest) >= narrow,
        "the page column spans the visible area, so the pages sit in its centre"
    );
    app.message(Message::Read(Read::ToggleSide));

    app.message(Message::Find(FindMsg::Open));
    app.message(Message::Find(FindMsg::Query("fingers".into())));
    let generation = app.app().doc.as_ref().unwrap().find.as_ref().unwrap().generation;
    let mut session = Session::local(&path, None).unwrap();
    let pages = (0..4).map(|page| (page, session.text(page))).collect();
    app.message(Message::Find(FindMsg::Chunk(generation, 4, pages)));
    let doc = app.app().doc.as_ref().unwrap();
    assert_eq!(doc.find.as_ref().unwrap().hits.len(), 1, "the word is found once");
    assert_eq!(doc.page(), 1, "the first hit takes the reader to its page");
    app.message(Message::Find(FindMsg::Close));

    app.message(Message::Note(NoteMsg::Tool(Some(Tool::Highlight))));
    app.message(Message::Note(NoteMsg::Selected(1, vec![0, 1])));
    let doc = app.app().doc.as_ref().unwrap();
    assert_eq!(doc.notes.len(), 1, "dragging over text with the highlighter adds a note");
    assert_eq!(doc.notes[0].quote, "Workspaces switch");
    assert!(doc.dirty());
    app.message(Message::Note(NoteMsg::Undo));
    assert!(!app.app().doc.as_ref().unwrap().dirty(), "undo takes the highlight back");

    app.message(Message::Note(NoteMsg::Bookmark));
    assert_eq!(app.app().doc.as_ref().unwrap().memory.bookmarks, vec![("Сторінка 2".to_string(), 1)]);

    app.message(Message::Read(Read::Step(1)));
    assert_eq!(app.app().doc.as_ref().unwrap().page(), 2);
    app.message(Message::Read(Read::PageInput("4".into())));
    app.message(Message::Read(Read::PageSubmit));
    assert_eq!(app.app().doc.as_ref().unwrap().page(), 3);
    app.message(Message::Read(Read::Back));
    assert_eq!(app.app().doc.as_ref().unwrap().page(), 2, "back returns from a typed jump");

    app.message(Message::Org(Org::Start));
    app.message(Message::Org(Org::Pick(0)));
    app.message(Message::Org(Org::Rotate(90)));
    app.message(Message::Org(Org::Remove));
    let org = app.app().doc.as_ref().unwrap().organizer.as_ref().unwrap();
    assert_eq!(org.kept(), 3);
    assert_eq!(org.pages[0].rotate, 90);
    app.message(Message::Org(Org::Exit));

    app.message(Message::Read(Read::Library));
    assert!(matches!(app.app().screen, Screen::Library));
    let memories = app.app().memories.clone();
    let memory = memories.get(&glide_folio::store::key(&path)).expect("the position was remembered");
    assert_eq!(memory.page, 2);
    assert_eq!(memory.pages, 4);
    for _ in 0..50 {
        if glide_folio::store::load_memories().get(&glide_folio::store::key(&path)).is_some_and(|m| m.page == 2) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(
        glide_folio::store::load_memories().get(&glide_folio::store::key(&path)).map(|m| m.page),
        Some(2),
        "the newest position reaches the disk"
    );

    let entries = glide_folio::store::entries(&glide_folio::store::load_prefs(), &memories);
    app.message(Message::Library(Lib::Scanned(entries)));
    assert_eq!(app.app().continuing().len(), 1, "a half-read document is offered to continue");
    let _ = std::fs::remove_dir_all(root);
}
