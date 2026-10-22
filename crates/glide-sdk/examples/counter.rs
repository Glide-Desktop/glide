use glide_sdk::iced::widget::column;
use glide_sdk::prelude::*;

struct Counter {
    count: u32,
}

#[derive(Debug, Clone)]
enum Message {
    Increment,
    Reset,
}

impl App for Counter {
    type Message = Message;

    fn new(_shell: &mut Shell<Message>) -> (Self, Task<Message>) {
        (Self { count: 0 }, Task::none())
    }

    fn title(&self, shell: &Shell<Message>) -> String {
        shell.t("Лічильник", "Counter").into()
    }

    fn update(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Message::Increment => self.count += 1,
            Message::Reset => {
                self.count = 0;
                shell.toast(shell.t("Скинуто", "Reset"));
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, shell: &'a Shell<Message>) -> Element<'a, Message> {
        let paint = shell.paint();
        container(
            column![
                text(self.count.to_string(), TextStyle::T1),
                widget::primary(paint, shell.t("Додати", "Add"), Message::Increment),
            ]
            .spacing(SP3),
        )
        .center(Fill)
        .into()
    }

    fn menus(&self, shell: &Shell<Message>) -> Vec<Menu<Message>> {
        vec![Menu::new(
            shell.t("Правка", "Edit"),
            vec![Item::action(shell.t("Скинути", "Reset"), Message::Reset).shortcut("Ctrl+R")],
        )]
    }
}

fn main() -> glide_sdk::iced::Result {
    glide_sdk::run::<Counter>(Settings::new("org.example.Counter").size(480.0, 360.0))
}
