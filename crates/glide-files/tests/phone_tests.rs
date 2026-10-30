use glide_files::Message;
use glide_files::fs::mtp::Phone;

#[test]
fn a_phone_being_opened_ignores_further_clicks() {
    let empty = std::env::temp_dir().join(format!("glide-no-tools-{}", std::process::id()));
    std::fs::create_dir_all(&empty).expect("empty PATH dir");
    // SAFETY: this binary holds this one test and changes the environment before it starts any thread.
    unsafe { std::env::set_var("PATH", &empty) };
    let (mut app, _) = glide_files::App::new();
    app.phones = vec![Phone {
        id: String::from("1:5"),
        name: String::from("Pixel"),
        serial: String::new(),
        camera: false,
        bus: 1,
        dev: 5,
        mount: None,
    }];
    let open = || Message::OpenPhone(String::from("1:5"));
    assert_eq!(app.update(open()).units(), 1, "the first click starts opening, with no mount tool to run");
    assert_eq!(app.update(open()).units(), 0, "a second click while it opens starts nothing");
    app.busy.insert(String::from("2:7"));
    let _ = app.update(Message::PhoneOpened(String::from("1:5"), Err(String::from("no tool"))));
    assert!(app.busy.contains("2:7"), "another phone that is still opening keeps its guard");
    assert_eq!(app.update(open()).units(), 1, "once that attempt ends the phone can be opened again");
    let _ = std::fs::remove_dir_all(empty);
}
