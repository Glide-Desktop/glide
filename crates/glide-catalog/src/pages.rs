use std::borrow::Cow;

use glide_sdk::anim::{self, Speed};
use glide_sdk::iced::widget::{Column, Row, Space, column, container, mouse_area, row, text_input};
use glide_sdk::iced::{Alignment, Color, Element, Fill, Padding, Vector};
use glide_sdk::prelude::*;
use glide_sdk::system::{Osd, QuickSettings, Urgency};
use glide_sdk::theme::{Accent, R1, R2, R3, R4, TextStyle};
use glide_sdk::widget::{self as ui, Check, Severity, Size, SwitchSize, Validity, mock};

use crate::{
    Catalog, Fruit, Look, Message, Page, Preset, fruit_option, fruit_options, timeout_option, timeout_options,
};

const WIDTH: f32 = 760.0;

fn page<'a>(
    shell: &'a Shell<Message>, page: Page, intro: &'a str, blocks: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    let paint = shell.paint();
    let head = column![text(page.title(shell.lang()), TextStyle::T1), text(intro, TextStyle::BODY).color(paint.fg_2())]
        .spacing(SP1);
    let content = Column::with_children(std::iter::once(head.into()).chain(blocks)).spacing(SP5).max_width(WIDTH);
    ui::scrollable(paint, container(content).padding(Padding::from([SP5, SP6])).center_x(Fill)).into()
}

fn demo<'a>(
    shell: &'a Shell<Message>, title: &'a str, content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    ui::section(shell.paint(), title, ui::card(shell.paint(), content))
}

fn wrap(items: Vec<Element<'_, Message>>) -> Element<'_, Message> {
    Row::with_children(items).spacing(SP2).align_y(Alignment::Center).wrap().vertical_spacing(SP2).into()
}

fn caption<'a>(shell: &'a Shell<Message>, label: &'a str) -> Element<'a, Message> {
    text(label, TextStyle::CAP).color(shell.paint().fg_3()).into()
}

#[must_use]
pub fn view<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let lang = shell.lang();
    let entries: Vec<(Option<Icon>, &str)> =
        Page::ALL.iter().map(|page| (Some(page.icon()), page.title(lang))).collect();
    let selected = Page::ALL.iter().position(|page| *page == catalog.page);
    let list =
        ui::scrollable(paint, container(ui::sidebar_list(paint, &entries, selected, Message::Page)).padding(SP2));
    let sidebar = ui::pane(paint.sidebar_bg(), 0.0, list).width(232);
    let body = match catalog.page {
        Page::Overview => overview(catalog, shell),
        Page::Buttons => buttons(catalog, shell),
        Page::Inputs => inputs(catalog, shell),
        Page::Selection => selection(catalog, shell),
        Page::Feedback => feedback(catalog, shell),
        Page::Layout => layout(catalog, shell),
        Page::Cards => cards(catalog, shell),
        Page::Insight => insight(catalog, shell),
        Page::Secrets => secrets(catalog, shell),
        Page::Overlays => overlays(shell),
        Page::Type => typography(shell),
        Page::Motion => motion(catalog, shell),
        Page::Notifications => notifications(catalog, shell),
        Page::System => system(catalog, shell),
    };
    row![sidebar, ui::vertical_separator(paint), container(body).width(Fill).height(Fill)].into()
}

fn overview<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let look = shell.look();
    let looks = [
        (Look::System, shell.t("Як у системі", "System")),
        (Look::Light, shell.t("Світла", "Light")),
        (Look::Dark, shell.t("Темна", "Dark")),
    ];
    let mut accents: Vec<Element<'a, Message>> =
        vec![ui::chip(paint, shell.t("Системний", "System"), catalog.accent.is_none(), Some(Message::Accent(None)))];
    for accent in Accent::ALL {
        let on = catalog.accent == Some(*accent);
        accents.push(ui::chip(paint, accent.key(), on, Some(Message::Accent(Some(*accent)))));
    }
    let facts = ui::list(
        paint,
        vec![
            ui::list_row(
                paint,
                shell.t("Тема оболонки", "Shell appearance"),
                None,
                text(look.appearance.key(), TextStyle::CALL),
            ),
            ui::list_row(paint, shell.t("Акцент", "Accent"), None, text(look.accent.key(), TextStyle::CALL)),
            ui::list_row(
                paint,
                shell.t("Масштаб анімацій", "Animation scale"),
                Some(shell.t("0 вимикає рух повністю", "0 turns all motion off")),
                text(format!("{:.2}", look.motion_scale), TextStyle::NUM),
            ),
            ui::list_row(
                paint,
                shell.t("Меню у верхній панелі", "Menu in the top bar"),
                None,
                text(
                    if shell.menu_in_top_bar() { shell.t("так", "yes") } else { shell.t("ні", "no") },
                    TextStyle::CALL,
                ),
            ),
        ],
    );
    page(
        shell,
        Page::Overview,
        shell.t(
            "Кожен компонент glide-sdk наживо: спробуйте наведення, натискання й клавіатуру. Тема і акцент нижче змінюють лише це вікно.",
            "Every glide-sdk component, live: try hover, press and the keyboard. The look below changes this window only.",
        ),
        vec![
            demo(shell, shell.t("Попередній перегляд", "Preview"), column![
                ui::segmented(paint, &looks, catalog.look, Message::Look),
                wrap(accents),
            ]
            .spacing(SP3)),
            ui::section(paint, shell.t("Оболонка", "Shell"), facts),
        ],
    )
}

fn buttons<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let tones = [
        (Tone::Accent, shell.t("Основна", "Primary")),
        (Tone::Tonal, shell.t("Тональна", "Tonal")),
        (Tone::Neutral, shell.t("Звичайна", "Neutral")),
        (Tone::Outline, shell.t("Контур", "Outline")),
        (Tone::Ghost, shell.t("Прозора", "Ghost")),
        (Tone::Danger, shell.t("Небезпечна", "Danger")),
    ];
    let row_of = |size: Size| -> Element<'a, Message> {
        wrap(
            tones
                .iter()
                .map(|&(tone, label)| ui::sized_button(paint, label, tone, size, Some(Message::Said(label))))
                .collect(),
        )
    };
    let disabled = wrap(tones.iter().map(|&(tone, label)| ui::button(paint, label, tone, None::<Message>)).collect());
    let (add, search, sidebar) =
        (shell.t("Додати", "Add"), shell.t("Пошук", "Search"), shell.t("Бічна панель", "Sidebar"));
    let icons = wrap(vec![
        ui::icon_button(paint, Icon::Plus, add, Some(Message::Said(add)), false),
        ui::icon_button(paint, Icon::Search, search, Some(Message::Said(search)), false),
        ui::icon_button(paint, Icon::Sidebar, sidebar, Some(Message::Said(sidebar)), true),
        ui::icon_button(paint, Icon::Refresh, shell.t("Вимкнено", "Disabled"), None, false),
    ]);
    let (day, week, month) = (shell.t("День", "Day"), shell.t("Тиждень", "Week"), shell.t("Місяць", "Month"));
    let linked = ui::linked(
        paint,
        &[(day, Some(Message::Said(day))), (week, Some(Message::Said(week))), (month, Some(Message::Said(month)))],
    );
    let actions =
        [shell.t("Дублювати", "Duplicate"), shell.t("Перейменувати", "Rename"), shell.t("Архівувати", "Archive")];
    let menu = ui::menu_button(
        paint,
        shell.t("Дії", "Actions"),
        actions.iter().map(|&label| (label.to_string(), Message::Said(label))).collect(),
        200.0,
    );
    page(
        shell,
        Page::Buttons,
        shell.t(
            "Шість тонів і три розміри зі специфікації; натискання стискає кнопку до 0,975.",
            "Six tones and three sizes from the spec; a press squeezes the button to 0.975.",
        ),
        vec![
            demo(shell, shell.t("Середні — 34 px", "Medium — 34 px"), row_of(Size::Medium)),
            demo(shell, shell.t("Малі — 28 px", "Small — 28 px"), row_of(Size::Small)),
            demo(shell, shell.t("Великі — 42 px", "Large — 42 px"), row_of(Size::Large)),
            demo(shell, shell.t("Вимкнені", "Disabled"), disabled),
            demo(
                shell,
                shell.t("Іконки, група, меню", "Icons, group, menu"),
                column![icons, linked, menu].spacing(SP3),
            ),
            demo(shell, shell.t("Кнопки-перемикачі", "Toggle buttons"), toggle_buttons(catalog, shell)),
        ],
    )
}

fn toggle_buttons<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let labels = [shell.t("Жирний", "Bold"), shell.t("Курсив", "Italic"), shell.t("Підкреслений", "Underline")];
    wrap(
        labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                ui::toggle_button(paint, *label, catalog.styles[index], Some(move |on| Message::Style(index, on)))
            })
            .collect(),
    )
}

fn inputs<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let lang = shell.lang();
    let email_ok = catalog.email.contains('@') && catalog.email.contains('.');
    let validity = if catalog.email.is_empty() {
        Validity::Neutral
    } else if email_ok {
        Validity::Valid
    } else {
        Validity::Invalid
    };
    let fields = column![
        caption(shell, shell.t("Звичайне поле", "Plain field")),
        ui::text_field(paint, shell.t("Ваше ім'я", "Your name"), &catalog.name, Message::Name),
        caption(shell, shell.t("З перевіркою — введіть адресу", "Validated — type an address")),
        ui::validated_field(paint, text_input("name@example.com", &catalog.email).on_input(Message::Email), validity),
        caption(shell, shell.t("Пошук", "Search")),
        ui::search_field(
            paint,
            shell.t("Шукати компоненти", "Search components"),
            &catalog.search,
            Message::Search,
            Message::ClearSearch
        ),
        caption(shell, shell.t("Пароль", "Password")),
        ui::password_field(
            paint,
            shell.t("Пароль", "Password"),
            &catalog.password,
            catalog.revealed,
            Message::Password,
            Message::Reveal
        ),
    ]
    .spacing(SP2);
    let editor = ui::text_area(paint, &catalog.notes)
        .on_action(Message::Edit)
        .height(110)
        .padding(SP3)
        .size(TextStyle::BODY.size_px);
    let previous = catalog.quantity.checked_sub(1);
    let next = (catalog.quantity < 9).then_some(catalog.quantity + 1);
    let pickers = ui::list(
        paint,
        vec![
            ui::list_row(
                paint,
                shell.t("Кількість", "Quantity"),
                None,
                ui::stepper(paint, catalog.quantity, previous, next, Message::Quantity),
            ),
            ui::list_row(
                paint,
                shell.t("Фрукт", "Fruit"),
                Some(shell.t("Випадний список із клавіатурою", "Dropdown with keyboard support")),
                container(ui::dropdown(
                    paint,
                    fruit_options(lang),
                    catalog.fruit.map(|fruit| fruit_option(fruit, lang)),
                    shell.t("Оберіть…", "Choose…"),
                    |option| Message::Fruit(option.value),
                ))
                .width(200),
            ),
        ],
    );
    page(
        shell,
        Page::Inputs,
        shell.t(
            "Поля отримують кільце 3 px у фокусі, підсвітку при наведенні й кольори стану.",
            "Fields get the 3 px focus ring, a hover tint and validity colours.",
        ),
        vec![
            demo(shell, shell.t("Текстові поля", "Text fields"), fields),
            demo(shell, shell.t("Багаторядкове", "Multi-line"), editor),
            ui::section(paint, shell.t("Значення", "Values"), pickers),
        ],
    )
}

fn selection<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let fruits = [
        (Fruit::Apple, shell.t("Яблуко", "Apple")),
        (Fruit::Pear, shell.t("Груша", "Pear")),
        (Fruit::Plum, shell.t("Слива", "Plum")),
    ];
    let toggles = ui::list(
        paint,
        vec![
            ui::list_row(
                paint,
                "Wi-Fi",
                Some(shell.t("Перемикач 44×26, E_OVER", "Switch 44×26, E_OVER")),
                ui::switch(paint, catalog.wifi, Some(Message::Wifi)),
            ),
            ui::list_row(
                paint,
                shell.t("Компактний", "Compact"),
                Some(shell.t("Малий 28×16 для панелей", "Small 28×16 for bars")),
                ui::sized_switch(paint, catalog.compact, SwitchSize::Bar, Some(Message::Compact)),
            ),
            ui::list_row(
                paint,
                shell.t("Вимкнений", "Disabled"),
                None,
                ui::switch(paint, true, None::<fn(bool) -> Message>),
            ),
        ],
    );
    let checks = column![
        ui::checkbox(
            paint,
            shell.t("Клацніть, щоб пройти три стани", "Click to walk the three states"),
            catalog.check,
            Some(Message::Check)
        ),
        ui::checkbox(paint, shell.t("Вимкнений", "Disabled"), Check::On, None),
        ui::radio_group(paint, &fruits, catalog.radio, Message::Radio),
    ]
    .spacing(SP3);
    let chips = wrap(
        [
            shell.t("Фото", "Photos"),
            shell.t("Відео", "Videos"),
            shell.t("Документи", "Documents"),
            shell.t("Музика", "Music"),
        ]
        .iter()
        .enumerate()
        .map(|(index, label)| ui::chip(paint, *label, catalog.chips[index], Some(Message::Chip(index))))
        .collect(),
    );
    page(
        shell,
        Page::Selection,
        shell.t(
            "Підкладки ковзають між варіантами, позначки «вистрибують» з переліт-кривою.",
            "Thumbs slide between options; marks pop with the overshoot curve.",
        ),
        vec![
            ui::section(paint, shell.t("Перемикачі", "Switches"), toggles),
            demo(shell, shell.t("Позначки й радіо", "Checks and radios"), checks),
            demo(
                shell,
                shell.t("Сегменти й вкладки", "Segments and tabs"),
                column![
                    ui::segmented(paint, &fruits, catalog.segment, Message::Segment),
                    ui::tabs(paint, &fruits, catalog.tab, Message::Tab),
                ]
                .spacing(SP4),
            ),
            demo(shell, shell.t("Чіпи", "Chips"), chips),
            demo(shell, shell.t("Повзунки", "Sliders"), sliders(catalog, shell)),
            demo(shell, shell.t("Навігація й кольори", "Navigation and colours"), navigation(catalog, shell)),
        ],
    )
}

fn sliders<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    column![
        row![
            text(shell.t("Гучність", "Volume"), TextStyle::CALL).width(110),
            ui::slider(paint, 0.0..=100.0, catalog.volume, Message::Volume),
            text(format!("{:.0}", catalog.volume), TextStyle::NUM).width(32)
        ]
        .spacing(SP3)
        .align_y(Alignment::Center),
        row![
            text(shell.t("Крок 25", "Step 25"), TextStyle::CALL).width(110),
            ui::stepped_slider(paint, 0.0..=100.0, catalog.stepped, 25.0, Some(Message::Stepped), None),
            text(format!("{:.0}", catalog.stepped), TextStyle::NUM).width(32)
        ]
        .spacing(SP3)
        .align_y(Alignment::Center),
        row![
            text(shell.t("Масштаб", "Zoom"), TextStyle::CALL).width(110),
            ui::zoom_control(
                paint,
                (catalog.stepped / 25.0).round() as usize,
                5,
                |level| Message::Stepped(level as f32 * 25.0),
                (shell.t("Зменшити", "Zoom out"), shell.t("Збільшити", "Zoom in")),
            ),
        ]
        .spacing(SP3)
        .align_y(Alignment::Center),
        row![
            ui::sized_button(paint, "0", Tone::Neutral, Size::Small, Some(Message::Volume(0.0))),
            ui::sized_button(paint, "50", Tone::Neutral, Size::Small, Some(Message::Volume(50.0))),
            ui::sized_button(paint, "100", Tone::Neutral, Size::Small, Some(Message::Volume(100.0))),
            caption(shell, shell.t("програмна зміна ковзає t2 E_OUT", "a programmatic change glides t2 E_OUT")),
        ]
        .spacing(SP2)
        .align_y(Alignment::Center),
    ]
    .spacing(SP3)
    .into()
}

fn navigation<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let trail = [shell.t("Домівка", "Home"), shell.t("Документи", "Documents"), shell.t("Звіти", "Reports"), "2026"];
    let depth = catalog.depth.min(trail.len() - 1);
    let crumbs = row![
        ui::breadcrumbs(paint, &trail[..=depth], Message::Depth),
        Space::new().width(Fill),
        ui::sized_button(
            paint,
            shell.t("Глибше", "Deeper"),
            Tone::Neutral,
            Size::Small,
            (depth + 1 < trail.len()).then_some(Message::Depth(depth + 1))
        ),
    ]
    .align_y(Alignment::Center);
    let colors: Vec<(Accent, glide_sdk::iced::Color)> = Accent::ALL
        .iter()
        .map(|accent| {
            let tokens = glide_sdk::theme::Tokens::new(paint.tokens.appearance, *accent);
            (*accent, glide_sdk::theme::to_iced(tokens.acc_bg))
        })
        .collect();
    let accent = catalog.accent.unwrap_or(shell.look().accent);
    column![
        caption(shell, shell.t("Хлібні крихти", "Breadcrumbs")),
        crumbs,
        caption(
            shell,
            shell.t("Крапки сторінок — активна витягується в пігулку", "Page dots — the active one stretches")
        ),
        ui::page_dots(paint, 5, catalog.dot, Message::Dot),
        caption(shell, shell.t("Палітра змінює акцент каталогу", "Swatches change the catalogue's accent")),
        ui::swatches(paint, &colors, Some(accent), |accent| Message::Accent(Some(accent))),
    ]
    .spacing(SP3)
    .into()
}

fn feedback<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let progress = column![
        ui::progress(paint, Some(catalog.loading)),
        ui::slider(paint, 0.0..=1.0, catalog.loading, Message::Loading),
        caption(shell, shell.t("Невизначений", "Indeterminate")),
        ui::progress(paint, None),
        row![
            ui::spinner(paint, false),
            ui::spinner(paint, true),
            caption(shell, shell.t("Спінери 22 і 34 px", "Spinners 22 and 34 px"))
        ]
        .spacing(SP4)
        .align_y(Alignment::Center),
    ]
    .spacing(SP3);
    let badges = wrap(vec![
        ui::badge(paint, shell.t("Нейтральний", "Neutral"), Severity::Neutral),
        ui::badge(paint, shell.t("Акцент", "Accent"), Severity::Accent),
        ui::badge(paint, shell.t("Готово", "Ready"), Severity::Ok),
        ui::badge(paint, shell.t("Увага", "Warning"), Severity::Warn),
        ui::badge(paint, shell.t("Помилка", "Error"), Severity::Danger),
        ui::count_badge(paint, catalog.count),
        ui::sized_button(paint, "−", Tone::Neutral, Size::Small, Some(Message::Count(catalog.count.saturating_sub(1)))),
        ui::sized_button(paint, "+", Tone::Neutral, Size::Small, Some(Message::Count(catalog.count + 1))),
    ]);
    let banners = column![
        ui::banner(
            paint,
            Severity::Warn,
            shell.t("Файл змінено на диску", "The file changed on disk"),
            shell.t("Перезавантажити його?", "Reload it?"),
            vec![ui::compact_button(
                paint,
                shell.t("Перезавантажити", "Reload"),
                Tone::Neutral,
                Some(Message::Said(shell.t("Перезавантажити", "Reload")))
            ),]
        ),
        ui::banner(paint, Severity::Ok, shell.t("Резервну копію створено", "Backup complete"), "", vec![]),
        ui::banner(
            paint,
            Severity::Danger,
            shell.t("Немає з'єднання", "No connection"),
            shell.t("Повторна спроба за 10 с", "Retrying in 10 s"),
            vec![]
        ),
    ]
    .spacing(SP2);
    let people = wrap(vec![
        ui::avatar(paint, "Артем", 40.0),
        ui::avatar(paint, "Olena", 32.0),
        ui::avatar(paint, "Ira", 24.0),
        ui::keycap(paint, "Ctrl"),
        ui::keycap(paint, "Shift"),
        ui::keycap(paint, "K"),
    ]);
    let empty = ui::empty_state(
        paint,
        Icon::Folder,
        shell.t("Тут порожньо", "Nothing here yet"),
        shell.t("Перетягніть файли сюди або створіть новий.", "Drop files here or create a new one."),
        Some(ui::primary(paint, shell.t("Створити", "Create"), Message::Said(shell.t("Створити", "Create")))),
    );
    page(
        shell,
        Page::Feedback,
        shell.t(
            "Прогрес плавно доганяє значення; лічильник з'являється з масштабу 0,6.",
            "Progress glides to its value; the counter pops in from 0.6.",
        ),
        vec![
            demo(shell, shell.t("Прогрес", "Progress"), progress),
            demo(shell, shell.t("Бейджі", "Badges"), badges),
            ui::section(paint, shell.t("Банери", "Banners"), banners),
            demo(shell, shell.t("Аватари й клавіші", "Avatars and keys"), people),
            demo(
                shell,
                shell.t("Картки задач — пауза за кнопкою", "Task cards — the button pauses"),
                tasks(catalog, shell),
            ),
            ui::section(paint, shell.t("Інфосмуги", "Info bars"), info_bars(shell)),
            demo(shell, shell.t("Бульбашка й картка тосту", "Bubble and toast card"), bubbles(shell)),
            demo(
                shell,
                shell.t("Заготовка під час завантаження", "Loading skeleton"),
                row![
                    ui::skeleton(paint, 40, 40.0),
                    column![ui::skeleton(paint, Fill, 14.0), ui::skeleton(paint, 220, 12.0)].spacing(SP2).width(Fill),
                ]
                .spacing(SP3)
                .align_y(Alignment::Center),
            ),
            ui::section(paint, shell.t("Порожній стан", "Empty state"), ui::card(paint, container(empty).height(280))),
        ],
    )
}

fn tasks<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let task = |paused: bool| ui::Task {
        title: shell.t("Копіювання", "Copying").to_string(),
        current: Some("holiday-in-the-carpathians.jpg".to_string()),
        fraction: Some(catalog.loading),
        paused,
        detail: format!("12/40 · {:.0}%", catalog.loading * 100.0),
        actions: vec![
            ui::icon_button(paint, if paused { Icon::Refresh } else { Icon::More }, "", Some(Message::Flip), false),
            ui::icon_button(paint, Icon::Close, "", Some(Message::Said(shell.t("Зупинити", "Stop"))), false),
        ],
        footer: None,
    };
    let all = ui::full_button(paint, shell.t("Усі операції", "All operations"), Tone::Tonal, Some(Message::Flip));
    let summary = ui::Task {
        title: shell.t("7 операцій", "7 operations").to_string(),
        current: None,
        actions: Vec::new(),
        footer: Some(all),
        ..task(false)
    };
    let queued = ui::Task { fraction: None, current: None, ..task(false) };
    column![
        wrap(vec![
            container(ui::task_card(paint, Icon::File, task(catalog.flipped))).width(260).into(),
            container(ui::task_card(paint, Icon::File, ui::Task { fraction: None, ..task(false) })).width(260).into(),
            container(ui::task_card(paint, Icon::Copy, summary)).width(220).into(),
        ]),
        ui::task_row(paint, ui::icon(Icon::Copy, 16.0, paint.fg_2()), task(false), 420.0),
        ui::task_row(paint, text("1", TextStyle::NUM).color(paint.fg_3()), queued, 420.0),
    ]
    .spacing(SP3)
    .into()
}

fn info_bars(shell: &Shell<Message>) -> Element<'_, Message> {
    let paint = shell.paint();
    let (empty_label, keep_label) = (shell.t("Очистити", "Empty"), shell.t("Відкинути", "Keep mine"));
    let empty = ui::compact_button(paint, empty_label, Tone::Neutral, Some(Message::Said(empty_label)));
    let keep = ui::compact_button(paint, keep_label, Tone::Ghost, Some(Message::Said(keep_label)));
    column![
        ui::info_bar(
            paint,
            None,
            shell.t("Смітник", "Trash"),
            shell.t("Об'єкти можна відновити або стерти.", "Items can be restored or erased."),
            vec![empty],
        ),
        ui::info_bar(
            paint,
            Some(Severity::Warn),
            shell.t("Файл змінено поза редактором.", "The file changed outside the editor."),
            "",
            vec![keep],
        ),
    ]
    .spacing(SP2)
    .into()
}

fn bubbles(shell: &Shell<Message>) -> Element<'_, Message> {
    let paint = shell.paint();
    wrap(vec![
        ui::bubble(paint, text(shell.t("Статична підказка", "A static tip"), TextStyle::CAP)),
        glide_sdk::overlay::toast_card(
            paint,
            shell.t("Переміщено до смітника", "Moved to the Trash"),
            None,
            Some((shell.t("Скасувати", "Undo"), Message::Said(shell.t("Скасувати", "Undo")))),
            Some((Message::Said(shell.t("Закрити", "Close")), shell.t("Закрити", "Close"))),
            420.0,
        ),
    ])
}

fn layout<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let cards = row![
        ui::action_card(
            paint,
            column![
                text(shell.t("Фото", "Photos"), TextStyle::HEAD),
                caption(shell, shell.t("1 248 файлів", "1,248 files"))
            ]
            .spacing(SP1),
            Message::Said(shell.t("Фото", "Photos"))
        ),
        ui::action_card(
            paint,
            column![
                text(shell.t("Документи", "Documents"), TextStyle::HEAD),
                caption(shell, shell.t("312 файлів", "312 files"))
            ]
            .spacing(SP1),
            Message::Said(shell.t("Документи", "Documents"))
        ),
        ui::action_card(
            paint,
            column![
                text(shell.t("Музика", "Music"), TextStyle::HEAD),
                caption(shell, shell.t("86 файлів", "86 files"))
            ]
            .spacing(SP1),
            Message::Said(shell.t("Музика", "Music"))
        ),
    ]
    .spacing(SP3);
    let rows = ui::list(
        paint,
        vec![
            ui::action_row(
                paint,
                shell.t("Мережа", "Network"),
                Some(shell.t("Підключено до «Дім»", "Connected to “Home”")),
                Message::Said(shell.t("Мережа", "Network")),
            ),
            ui::action_row(paint, "Bluetooth", Some(shell.t("2 пристрої", "2 devices")), Message::Said("Bluetooth")),
            ui::switch_row(
                paint,
                shell.t("Режим польоту", "Airplane mode"),
                Some(shell.t("Рядок-перемикач: клацніть будь-де", "A switch row: click anywhere on it")),
                !catalog.wifi,
                Some(|on: bool| Message::Wifi(!on)),
            ),
        ],
    );
    let first = ui::expander(
        paint,
        shell.t("Що нового у версії 0.2", "What's new in 0.2"),
        catalog.expanded[0],
        Message::Expand(0),
        column![
            text(shell.t("• Анімовані перемикачі та повзунки", "• Animated switches and sliders"), TextStyle::CALL),
            text(shell.t("• Випадні списки з клавіатурою", "• Dropdowns with keyboard support"), TextStyle::CALL),
            text(shell.t("• Тестування сповіщень", "• A notification test bench"), TextStyle::CALL),
        ]
        .spacing(SP1),
    );
    let second = ui::expander(
        paint,
        shell.t("Додаткові параметри", "Advanced options"),
        catalog.expanded[1],
        Message::Expand(1),
        ui::list_row(
            paint,
            shell.t("Режим розробника", "Developer mode"),
            None,
            ui::switch(paint, catalog.compact, Some(Message::Compact)),
        ),
    );
    page(
        shell,
        Page::Layout,
        shell.t(
            "Картки з дією піднімаються на 3 px; блоки розгортаються за t3 E_OUT.",
            "Action cards lift 3 px; expanders unfold over t3 E_OUT.",
        ),
        vec![
            ui::section(paint, shell.t("Картки з дією", "Action cards"), cards),
            ui::section(paint, shell.t("Список", "Boxed list"), rows),
            ui::section(paint, shell.t("Розгортні блоки", "Expanders"), column![first, second].spacing(SP2)),
            demo(shell, shell.t("Пункти бічної панелі", "Sidebar items"), sidebar_items(catalog, shell)),
            demo(
                shell,
                shell.t("Крихти, таблиця, поле на місці", "Crumbs, table, inline field"),
                tables(catalog, shell),
            ),
        ],
    )
}

fn cards<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let accent = glide_sdk::theme::to_iced(paint.tokens.ramp[3]);
    let green = paint.ok();
    let tiles = ui::grid(
        vec![
            ui::stat_tile(paint, Icon::File, accent, "12", shell.t("потоків", "threads")),
            ui::stat_tile(paint, Icon::Folder, green, "28%", shell.t("пам'яті", "memory")),
            ui::stat_tile(paint, Icon::Refresh, paint.warn(), shell.t("5 год", "5 h"), shell.t("працює", "running")),
        ],
        3,
        SP3,
    );
    let usage = ui::usage_card(
        paint,
        shell.t("Диск", "Disk"),
        shell.t("Вільно 56 ГБ", "56 GB free"),
        vec![
            (0.6, accent, shell.t("Програми", "Apps").to_string(), shell.t("300 ГБ", "300 GB").to_string()),
            (0.2, green, shell.t("Медіа", "Media").to_string(), String::new()),
        ],
    );
    let picks = row![
        ui::preview_card(
            paint,
            ui::icon_tile(Icon::Sidebar, accent, 64.0),
            shell.t("Світла", "Light"),
            !catalog.flipped,
            Some(Message::Flip)
        ),
        ui::preview_card(
            paint,
            ui::icon_tile(Icon::Sidebar, paint.fg_2(), 64.0),
            shell.t("Темна", "Dark"),
            catalog.flipped,
            Some(Message::Flip)
        ),
        container(ui::choice_card(
            paint,
            ui::icon_tile(Icon::Bell, accent, 40.0),
            shell.t("Праворуч угорі", "Top right"),
            Some(shell.t("Банери", "Banners").to_string()),
            !catalog.flipped,
            Message::Flip,
        ))
        .width(200),
    ]
    .spacing(SP3)
    .align_y(Alignment::Center);
    let hero = ui::hero_card(
        paint,
        Icon::Bell,
        accent,
        shell.t("Не турбувати", "Do Not Disturb"),
        shell.t("Банери не з'являються", "Banners stay hidden"),
        Some(ui::switch(paint, catalog.compact, Some(Message::Compact))),
        false,
    );
    let header = ui::page_header(
        paint,
        ui::icon_tile(Icon::Folder, green, 44.0),
        Some(shell.t("Система", "System")),
        shell.t("Сховище", "Storage"),
        Some(shell.t("Заголовок сторінки", "A page header")),
        None,
    );
    page(
        shell,
        Page::Cards,
        shell.t(
            "Групи, рядки, картки й плитки, якими користується Settings.",
            "Groups, rows, cards and tiles, as Settings uses them.",
        ),
        vec![
            header,
            card_group(catalog, shell),
            ui::section(paint, shell.t("Плитки й використання", "Tiles and usage"), column![tiles, usage].spacing(SP3)),
            demo(shell, shell.t("Прев'ю й вибір", "Previews and choices"), picks),
            ui::section(paint, shell.t("Головна картка", "Hero card"), hero),
            demo(shell, shell.t("Дрібні частини", "Small pieces"), small_pieces(catalog, shell)),
        ],
    )
}

fn card_group<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let accent = glide_sdk::theme::to_iced(paint.tokens.ramp[3]);
    ui::Group::new(paint)
        .title(shell.t("Група", "Group"))
        .description(shell.t("Заголовок, опис і рядки", "A heading, a description and rows"))
        .suffix(ui::link(paint, shell.t("Скинути", "Reset"), Some(Message::Said(shell.t("Скинути", "Reset")))))
        .push(
            ui::ListRow::new(paint, shell.t("Режим польоту", "Airplane mode"))
                .subtitle(shell.t("Рядок з префіксом і перемикачем", "A row with a prefix and a switch"))
                .prefix(ui::icon_tile(Icon::Bell, accent, 28.0))
                .suffix(ui::switch(paint, catalog.wifi, Some(Message::Wifi))),
        )
        .push(
            ui::ListRow::new(paint, shell.t("Недоступний рядок", "A dimmed row"))
                .dimmed(true)
                .suffix(ui::status_badge(paint, shell.t("Вимкнено", "Off"), Severity::Neutral)),
        )
        .push(
            ui::ListRow::new(paint, shell.t("Вузьке вікно", "Narrow window"))
                .subtitle(shell.t("Керування переходить під текст", "The control moves under the label"))
                .stacked(catalog.compact)
                .suffix(ui::inert(
                    paint,
                    !catalog.compact,
                    ui::segmented(paint, &[(0, Cow::Borrowed("A")), (1, Cow::Borrowed("B"))], 0, |_| Message::Flip),
                )),
        )
        .push(
            ui::ListRow::new(paint, shell.t("Деталі", "Details"))
                .suffix(ui::disclosure(paint, Some(String::from("3"))))
                .on_press(Message::Said(shell.t("Деталі", "Details"))),
        )
        .build()
}

fn disk_issue<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    ui::Issue::new(paint, shell.t("Мало місця на диску", "Low on disk space"))
        .summary(shell.t("Вільно 4 % — звільніть місце", "4 % free — make some room"))
        .details(
            [String::from("/dev/nvme0n1p2 · ext4"), shell.t("12,4 ГБ / 476 ГБ", "12.4 GB / 476 GB").to_string()],
            catalog.issue_open,
        )
        .toggle(Message::IssueOpen, shell.t("Детальніше", "Details"), shell.t("Сховати", "Hide"))
        .action(ui::button(
            paint,
            shell.t("Відкрити", "Open"),
            Tone::Neutral,
            Some(Message::Said(shell.t("Відкрити", "Open"))),
        ))
        .into()
}

fn insight<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let ramp = |step: usize| glide_sdk::theme::to_iced(paint.tokens.ramp[step]);
    let issue = disk_issue(catalog, shell);
    let glances = row![
        ui::Glance::new(paint, Icon::Folder, ramp(3), shell.t("Сховище", "Storage"))
            .headline(shell.t("212 ГБ вільно", "212 GB free"))
            .meter(0.55, paint.acc_bg())
            .detail(shell.t("з 476 ГБ", "of 476 GB"))
            .on_press(Message::Said(shell.t("Сховище", "Storage"))),
        ui::Glance::new(paint, Icon::Bell, ramp(4), "Wi-Fi")
            .headline("Home")
            .aside(ui::signal_bars(paint, 3, 4))
            .detail(shell.t("Підключено", "Connected")),
    ]
    .spacing(SP3);
    let facts = ui::fact_grid(
        paint,
        vec![
            (shell.t("Процесор", "Processor").into(), "AMD Ryzen 7 7840U".into()),
            (shell.t("Пам'ять", "Memory").into(), shell.t("32 ГБ · 17,6 ГБ вільно", "32 GB · 17.6 GB free").into()),
            (shell.t("Графіка", "Graphics").into(), "AMD Radeon 780M".into()),
        ],
        2,
    );
    let stages = ui::stage_track(
        paint,
        &[
            ui::Stage::new(0.2, shell.t("гасне", "dims"), paint.acc_bg()),
            ui::Stage::new(0.5, shell.t("блокується", "locks"), paint.warn()),
            ui::Stage::new(0.85, shell.t("сон", "sleeps"), paint.ok()),
        ],
    );
    let plans = ui::choice_tiles(
        paint,
        vec![
            (Fruit::Apple, shell.t("Заощадження", "Saver").into(), shell.t("Довше працює", "Lasts longer").into()),
            (Fruit::Pear, shell.t("Збалансовано", "Balanced").into(), shell.t("Як зазвичай", "As usual").into()),
            (Fruit::Plum, shell.t("Швидкодія", "Performance").into(), shell.t("Найшвидше", "Fastest").into()),
        ],
        catalog.plan,
        Message::Plan,
    );
    let chips = ui::status_chips(
        paint,
        vec![(paint.ok(), "GTK 4".into()), (paint.warn(), "GTK 3".into()), (paint.fg_4(), "Kvantum".into())],
    );
    let header = row![
        ui::section_heading(paint, shell.t("Шрифти й іконки", "Fonts and icons")),
        Space::new().width(Fill),
        ui::reset_action(
            paint,
            shell.t("1 змінено", "1 changed"),
            shell.t("Скинути", "Reset"),
            Some(Message::Said(shell.t("Скинути", "Reset")))
        ),
    ]
    .align_y(Alignment::Center);
    let small = wrap(vec![
        chips,
        ui::info_button(paint, catalog.info_open, Message::InfoOpen),
        ui::applies_chip(paint, shell.t("після повторного входу", "after signing in again"), true),
        ui::applies_chip(paint, shell.t("одразу", "right away"), false),
    ]);
    let pictures = row![
        mock::desktop(paint, 160.0),
        mock::theme_sample(paint, 196.0, 124.0),
        mock::split_theme_sample(
            glide_sdk::Paint::new(glide_sdk::theme::Appearance::Light, Accent::Blue),
            glide_sdk::Paint::new(glide_sdk::theme::Appearance::Dark, Accent::Blue),
            196.0,
            124.0,
        ),
    ]
    .spacing(SP3)
    .align_y(Alignment::Center);
    page(
        shell,
        Page::Insight,
        shell.t(
            "Те, з чого складено «Огляд» Налаштувань: проблеми, стислі картки, факти й етапи.",
            "What the Settings overview is built from: issues, glance cards, facts and stages.",
        ),
        vec![
            demo(shell, shell.t("Проблема", "Issue"), issue),
            ui::section(paint, shell.t("Картки-огляди", "Glance cards"), glances),
            ui::section(paint, shell.t("Факти", "Facts"), facts),
            ui::section(paint, shell.t("Панель інформації", "Information panel"), inspector(shell)),
            demo(shell, shell.t("Етапи", "Stages"), stages),
            ui::section(paint, shell.t("Вибір плиткою", "Choice tiles"), plans),
            ui::section(paint, shell.t("Заголовок зі скиданням", "Heading with reset"), header),
            demo(shell, shell.t("Позначки", "Marks"), small),
            ui::section(paint, shell.t("Ескізи", "Sketches"), pictures),
        ],
    )
}

fn inspector(shell: &Shell<Message>) -> Element<'_, Message> {
    let paint = shell.paint();
    let tags = ui::swatches(paint, &[(0, paint.danger()), (1, paint.ok())], Some(1), |_| Message::Flip);
    let open = ui::full_button(
        paint,
        shell.t("Відкрити", "Open"),
        Tone::Accent,
        Some(Message::Said(shell.t("Відкрити", "Open"))),
    );
    let properties_label = shell.t("Властивості", "Properties");
    let properties = ui::full_button(paint, properties_label, Tone::Outline, Some(Message::Said(properties_label)));
    let panel = ui::Inspector::new(paint, shell.t("Звіт за жовтень.pdf", "October report.pdf"))
        .preview(ui::icon(Icon::File, 96.0, paint.fg_2()))
        .subtitle(shell.t("Документ PDF · 2,4 МБ", "PDF document · 2.4 MB"))
        .figure(shell.t("Розмір", "Size"), shell.t("2,4 МБ", "2.4 MB"))
        .figure(shell.t("Де", "Where"), shell.t("~/Документи", "~/Documents"))
        .section(shell.t("Мітки", "Tags"), tags)
        .action(open)
        .action(properties);
    container(panel).height(560).into()
}

fn small_pieces<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let accent = glide_sdk::theme::to_iced(paint.tokens.ramp[3]);
    let green = paint.ok();
    column![
        ui::profile_card(
            paint,
            ui::avatar(paint, "Olena Koval", 40.0),
            "Olena Koval",
            "olena",
            catalog.flipped,
            Message::Flip
        ),
        ui::noted_banner(
            paint,
            Severity::Accent,
            Icon::Bell,
            "",
            shell.t("Банер зі своїм значком", "A banner with its own glyph"),
            vec![]
        ),
        row![
            ui::busy_label(paint, shell.t("Пошук…", "Searching…")),
            ui::key_combo(paint, "Ctrl+Shift+K", str::to_string),
            ui::swatch(paint, accent, true, Some(Message::Flip), 20.0),
            ui::swatch(paint, green, false, Some(Message::Flip), 20.0),
        ]
        .spacing(SP4)
        .align_y(Alignment::Center),
        ui::sample_well(paint, text("Aa — The quick brown fox", TextStyle::T2)),
        ui::empty_note(
            paint,
            Icon::Search,
            shell.t("Нічого не знайдено", "Nothing found"),
            shell.t("Компактний порожній стан", "A compact empty state")
        ),
    ]
    .spacing(SP3)
    .into()
}

fn prefix_range(label: &str, chars: usize) -> (usize, usize) {
    (0, label.char_indices().nth(chars).map_or(label.len(), |(end, _)| end))
}

fn sidebar_items<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let notifications = shell.t("Сповіщення", "Notifications");
    column![
        ui::sidebar_item(paint, Some(Icon::Folder), shell.t("Домівка", "Home"), !catalog.flipped, Message::Flip),
        ui::sidebar_item(paint, Some(Icon::File), shell.t("Документи", "Documents"), catalog.flipped, Message::Flip),
        Element::from(
            ui::SidebarItem::new(paint, shell.t("Смітник", "Trash"))
                .glyph(Icon::File)
                .count(3)
                .on_press(Some(Message::Said(shell.t("Смітник", "Trash")))),
        ),
        Element::from(
            ui::SidebarItem::new(paint, "KINGSTON")
                .glyph(Icon::File)
                .meter(Some(0.42))
                .on_press(Some(Message::Said(shell.t("Накопичувач", "Drive"))))
                .accessory(ui::icon_button(
                    paint,
                    Icon::Up,
                    "",
                    Some(Message::Said(shell.t("Витягти", "Eject"))),
                    false,
                )),
        ),
        Element::from(
            ui::SidebarItem::new(paint, shell.t("Файлова система", "File System"))
                .glyph(Icon::Server)
                .meter(Some(0.91))
                .on_press(Some(Message::Said(shell.t("Файлова система", "File System")))),
        ),
        Element::from(
            ui::SidebarItem::new(paint, shell.t("Не змонтовано", "Not mounted"))
                .glyph(Icon::File)
                .ink(Some(paint.fg_3()))
                .on_press(Some(Message::Said(shell.t("Змонтувати", "Mount")))),
        ),
        Element::from(
            ui::SidebarItem::new(paint, notifications)
                .leading(ui::icon_tile(Icon::Bell, paint.danger(), 24.0))
                .matched(Some(prefix_range(notifications, 6)))
                .accessory(ui::keycap(paint, "↵"))
                .on_press(Some(Message::Said(notifications))),
        ),
        ui::hinted_search_input(
            paint,
            text_input(shell.t("Пошук", "Search"), &catalog.search).on_input(Message::Search),
            (!catalog.search.is_empty()).then(|| Message::Search(String::new())),
            "",
            Some(ui::key_combo(paint, "Ctrl+F", str::to_string)),
        ),
    ]
    .spacing(2)
    .width(240)
    .into()
}

fn tables<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let trail = [shell.t("Домівка", "Home"), shell.t("Документи", "Documents"), shell.t("Звіти", "Reports")];
    let crumbs = ui::crumb_trail(
        paint,
        trail
            .iter()
            .enumerate()
            .map(|(index, label)| {
                ui::Crumb::new(paint, *label)
                    .glyph(Icon::Folder, if index == 0 { paint.fg_2() } else { paint.acc_bg() })
                    .current(index + 1 == trail.len())
                    .on_press(Some(Message::Said(shell.t("Крихта", "Crumb"))))
                    .into()
            })
            .collect(),
    );
    let header = ui::table_header(
        paint,
        vec![
            ui::HeaderCell::new(paint, shell.t("Назва", "Name"))
                .sorted(Some(!catalog.flipped))
                .on_press(Some(Message::Flip))
                .into(),
            ui::HeaderCell::new(paint, shell.t("Розмір", "Size"))
                .width(120)
                .end(true)
                .on_press(Some(Message::Flip))
                .into(),
            ui::HeaderCell::new(paint, shell.t("Змінено", "Modified")).width(160).lit(catalog.compact).into(),
        ],
    );
    let inline = ui::inline_field(paint, text_input("", &catalog.name).on_input(Message::Name), TextStyle::CALL);
    column![
        caption(shell, shell.t("Крихти зі значками", "Crumbs with glyphs")),
        crumbs,
        caption(shell, shell.t("Заголовок таблиці — клацніть «Назва»", "Table header — click Name")),
        header,
        caption(shell, shell.t("Поле на місці (перейменування)", "Inline field (rename)")),
        container(inline).width(260),
    ]
    .spacing(SP3)
    .into()
}

fn secrets<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let secret = "c7#Vq2!mWx9tLp4Rz8";
    let field = column![
        ui::secret_field(paint, secret)
            .revealed(catalog.secret_revealed)
            .on_reveal(Message::RevealSecret, shell.t("Показати", "Show"), shell.t("Сховати", "Hide"))
            .on_copy(Message::Said(shell.t("Скопійовано", "Copied")), shell.t("Скопіювати", "Copy")),
        ui::password_text(paint, secret, ui::SECRET),
    ]
    .spacing(SP3);
    let meters = Column::with_children(
        [
            (ui::Strength::Weak, shell.t("Слабкий", "Weak")),
            (ui::Strength::Fair, shell.t("Середній", "Fair")),
            (ui::Strength::Strong, shell.t("Надійний", "Strong")),
            (ui::Strength::VeryStrong, shell.t("Дуже надійний", "Very strong")),
        ]
        .into_iter()
        .map(|(strength, label)| ui::strength_line(paint, strength, label)),
    )
    .spacing(SP2);
    let codes = wrap(vec![ui::totp(paint, "482913", 17, 30), ui::totp(paint, "482913", 4, 30)]);
    let chips = wrap(vec![
        ui::vault_chip(paint, "Bitwarden", ui::VaultState::Synced),
        ui::vault_chip(paint, shell.t("Особисте", "Personal"), ui::VaultState::Local),
        ui::vault_chip(paint, shell.t("Bitwarden · дім", "Bitwarden · home"), ui::VaultState::Pending),
        ui::vault_chip(paint, shell.t("Розблоковано", "Unlocked"), ui::VaultState::Unlocked),
        ui::vault_chip(paint, "Bitwarden", ui::VaultState::Current),
    ]);
    let tiles = wrap(vec![
        ui::mark_tile(paint, "GitHub", 22.0, false),
        ui::mark_tile(paint, "GitHub", 32.0, false),
        ui::mark_tile(paint, "GitHub", 40.0, true),
        ui::mark_tile(paint, "GitHub", 56.0, true),
        ui::glyph_tile(paint, Icon::File, 40.0, false),
    ]);
    let rows = column![
        ui::entry_row(
            paint,
            ui::mark_tile(paint, "GitHub", 32.0, true),
            "GitHub",
            "artem-dev",
            vec![ui::dot(paint.ok(), 6.0)],
            catalog.version == 0,
            Message::Version(0),
        ),
        ui::entry_row(
            paint,
            ui::mark_tile(paint, "Telegram", 32.0, false),
            "Telegram Desktop",
            "@artem_k",
            vec![ui::dot(paint.fg_4(), 6.0)],
            catalog.version == 1,
            Message::Version(1),
        ),
    ]
    .spacing(2);
    let code: Element<'_, Message> = ui::code_input(paint, &catalog.code, 6, Message::Code).into();
    let link = ui::link_field(
        paint,
        "https://send.bitwarden.com/#/8sKq2Lw7RzaxX0Tb/Hq9nV2c",
        shell.t("Скопіювати", "Copy"),
        Some(Message::Said(shell.t("Посилання скопійовано", "Link copied"))),
    );
    let qr = ui::qr_code(paint, "https://send.bitwarden.com/#/8sKq2Lw7RzaxX0Tb/Hq9nV2c", 148.0);
    let mut blocks = vec![
        demo(shell, shell.t("Поле секрету", "Secret field"), field),
        demo(shell, shell.t("Сила пароля", "Password strength"), meters),
        demo(shell, shell.t("Код підтвердження", "Verification code"), codes),
        demo(shell, shell.t("Введення коду", "Code entry"), code),
        demo(shell, shell.t("Сховища", "Vaults"), column![chips, tiles].spacing(SP3)),
        demo(shell, shell.t("Рядки записів", "Entry rows"), rows),
    ];
    blocks.extend(choices(catalog, shell));
    blocks.push(demo(shell, shell.t("Посилання", "Link"), column![link, qr].spacing(SP3)));
    page(
        shell,
        Page::Secrets,
        shell.t(
            "Поля секретів, сила пароля, коди TOTP, стани сховищ і посилання для надсилання.",
            "Secret fields, password strength, TOTP codes, vault states and share links.",
        ),
        blocks,
    )
}

fn choices<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Vec<Element<'a, Message>> {
    let paint = shell.paint();
    let field_of = |label: &str, value: &str, differs: bool| ui::DiffField {
        label: label.to_string(),
        value: value.to_string(),
        differs,
        ..ui::DiffField::default()
    };
    let versions = row![
        ui::diff_card(
            paint,
            shell.t("Цей пристрій", "This device"),
            shell.t("сьогодні 14:02, офлайн", "today 14:02, offline"),
            None,
            vec![field_of(shell.t("Пароль", "Password"), "quartz-lantern-91-bloom", true)],
            catalog.version == 0,
            Message::Version(0),
        ),
        ui::diff_card(
            paint,
            shell.t("Сервер", "Server"),
            shell.t("сьогодні 14:10", "today 14:10"),
            Some(shell.t("Новіша", "Newer")),
            vec![field_of(shell.t("Пароль", "Password"), "Hx8$pQ2m!Lw9vRt4", true)],
            catalog.version == 1,
            Message::Version(1),
        ),
    ]
    .spacing(SP3);
    let stats = row![
        ui::stat_choice(
            paint,
            "7",
            paint.danger(),
            shell.t("Слабкі", "Weak"),
            shell.t("Легко підібрати", "Easy to guess"),
            catalog.filter == 0,
            Message::Filter(0),
        ),
        ui::stat_choice(
            paint,
            "12",
            paint.warn(),
            shell.t("Повторені", "Reused"),
            shell.t("5 однакових паролів", "5 shared passwords"),
            catalog.filter == 1,
            Message::Filter(1),
        ),
    ]
    .spacing(SP3);
    vec![
        demo(shell, shell.t("Версії запису", "Item versions"), versions),
        demo(shell, shell.t("Фільтри перевірки", "Check filters"), stats),
    ]
}

fn overlays(shell: &Shell<Message>) -> Element<'_, Message> {
    let paint = shell.paint();
    let triggers = wrap(vec![
        ui::secondary(paint, shell.t("Діалог", "Dialog"), Message::Dialog),
        ui::secondary(paint, shell.t("Тост", "Toast"), Message::Toast),
        ui::secondary(paint, shell.t("Тост із дією", "Toast with action"), Message::ToastUndo),
    ]);
    let hint = text(shell.t("Клацніть правою кнопкою тут", "Right-click here"), TextStyle::CALL).color(paint.fg_2());
    let zone = mock::outlined_surface(paint.fill_1(), paint.line_2(), 1.0, R2, hint).center_x(Fill).center_y(120);
    let area = mouse_area(zone).on_right_press(Message::Context);
    let tips = wrap(vec![
        ui::tooltip(
            paint,
            ui::secondary(paint, shell.t("Наведіть", "Hover me"), Message::Said(shell.t("Наведіть", "Hover me"))),
            shell.t("Підказка з'являється за 550 мс", "A tip appears after 550 ms"),
        ),
        ui::tooltip(
            paint,
            ui::icon_button(paint, Icon::Bell, "", Some(Message::Said(shell.t("Сповіщення", "Notifications"))), false),
            shell.t("Сповіщення", "Notifications"),
        ),
    ]);
    page(
        shell,
        Page::Overlays,
        shell.t(
            "Діалоги, тости, меню й підказки відкриваються і зникають з рухом специфікації.",
            "Dialogs, toasts, menus and tips open and close with the spec's motion.",
        ),
        vec![
            demo(shell, shell.t("Діалоги й тости", "Dialogs and toasts"), triggers),
            demo(shell, shell.t("Контекстне меню", "Context menu"), area),
            demo(shell, shell.t("Підказки", "Tooltips"), tips),
        ],
    )
}

fn typography(shell: &Shell<Message>) -> Element<'_, Message> {
    let paint = shell.paint();
    let styles = [
        ("DISPLAY", TextStyle::DISPLAY),
        ("T1", TextStyle::T1),
        ("T2", TextStyle::T2),
        ("T3", TextStyle::T3),
        ("HEAD", TextStyle::HEAD),
        ("BODY", TextStyle::BODY),
        ("CALL", TextStyle::CALL),
        ("CAP", TextStyle::CAP),
        ("MICRO", TextStyle::MICRO),
        ("NUM", TextStyle::NUM),
    ];
    let sample = shell.t("Швидка руда лисиця", "The quick brown fox");
    let scale = Column::with_children(styles.iter().map(|&(name, style)| {
        row![
            text(name, TextStyle::CAP).color(paint.fg_3()).width(80),
            text(if name == "NUM" { "1 234 567,89" } else { sample }, style),
            Space::new().width(Fill),
            text(format!("{}/{}", style.size_px, style.weight), TextStyle::NUM).color(paint.fg_3()),
        ]
        .spacing(SP3)
        .align_y(Alignment::Center)
        .into()
    }))
    .spacing(SP2);
    let swatches: [(&str, Color); 16] = [
        ("win-bg", paint.win_bg()),
        ("view-bg", paint.view_bg()),
        ("card-bg", paint.card_bg()),
        ("sidebar-bg", paint.sidebar_bg()),
        ("fg-1", paint.fg_1()),
        ("fg-2", paint.fg_2()),
        ("fg-3", paint.fg_3()),
        ("fg-4", paint.fg_4()),
        ("fill-1", paint.fill_1()),
        ("fill-2", paint.fill_2()),
        ("fill-3", paint.fill_3()),
        ("acc-bg", paint.acc_bg()),
        ("acc-soft", paint.acc_soft()),
        ("ok", paint.ok()),
        ("warn", paint.warn()),
        ("danger", paint.danger()),
    ];
    let colours = wrap(
        swatches
            .iter()
            .map(|&(name, color)| {
                column![
                    mock::outlined(color, paint.line_2(), 1.0, 64.0, 44.0, R2),
                    text(name, TextStyle::CAP).color(paint.fg_2()),
                ]
                .spacing(SP1)
                .into()
            })
            .collect(),
    );
    let radii = wrap(
        [("R1", R1), ("R2", R2), ("R3", R3), ("R4", R4)]
            .iter()
            .map(|&(name, radius)| {
                column![
                    mock::block(paint.acc_soft(), 56.0, 56.0, radius),
                    text(name, TextStyle::CAP).color(paint.fg_2())
                ]
                .spacing(SP1)
                .align_x(Alignment::Center)
                .into()
            })
            .collect(),
    );
    page(
        shell,
        Page::Type,
        shell.t(
            "Типографічна шкала, токени кольору й радіуси, що лежать в основі всіх компонентів.",
            "The type scale, colour tokens and radii every component is built from.",
        ),
        vec![
            demo(shell, shell.t("Шкала шрифтів", "Type scale"), scale),
            demo(shell, shell.t("Кольори", "Colours"), colours),
            demo(shell, shell.t("Радіуси", "Radii"), radii),
        ],
    )
}

fn motion<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let speeds = [
        ("STATE · t1 E_IO", Speed::STATE),
        ("PRESS · t1 E_OUT", Speed::PRESS),
        ("COLOR · t2 E_OUT", Speed::COLOR),
        ("OVERSHOOT · t2 E_OVER", Speed::OVERSHOOT),
        ("EXPAND · t3 E_OUT", Speed::EXPAND),
        ("GROW · t3 E_OVER", Speed::GROW),
    ];
    let target = if catalog.flipped { 1.0 } else { 0.0 };
    let lanes = Column::with_children(speeds.iter().map(|&(name, speed)| {
        let dot = anim::animate(target, speed, |_| mock::block(paint.acc_bg(), 28.0, 28.0, R2))
            .offset(|value| Vector::new(300.0 * value, 0.0));
        row![text(name, TextStyle::NUM).color(paint.fg_2()).width(170), container(dot).width(340)]
            .spacing(SP3)
            .align_y(Alignment::Center)
            .into()
    }))
    .spacing(SP3);
    page(
        shell,
        Page::Motion,
        shell.t(
            "Кожна доріжка — одна з кривих специфікації. Масштаб анімацій оболонки й «зменшити рух» діють на всі.",
            "Each lane is one of the spec's curves. The shell's animation scale and reduced motion apply to all of them.",
        ),
        vec![
            demo(shell, shell.t("Криві", "Curves"), column![lanes, ui::primary(paint, shell.t("Запустити", "Play"), Message::Flip)].spacing(SP4)),
            demo(shell, shell.t("Висота", "Height"), ui::expander(paint, shell.t("Розгорнути", "Unfold"), catalog.expanded[1], Message::Expand(1), container(Space::new().height(120)).width(Fill))),
        ],
    )
}

fn compose<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let draft = &catalog.draft;
    column![
        ui::text_field(paint, shell.t("Заголовок", "Summary"), &draft.summary, Message::Summary),
        ui::text_field(paint, shell.t("Текст", "Body"), &draft.body, Message::Body),
        row![
            ui::primary(paint, shell.t("Надіслати", "Send"), Message::Send),
            ui::button(
                paint,
                shell.t("Замінити останнє", "Replace last"),
                Tone::Neutral,
                catalog.last.map(|_| Message::Replace)
            ),
            ui::button(paint, shell.t("Відкликати", "Withdraw"), Tone::Danger, catalog.last.map(|_| Message::Withdraw)),
        ]
        .spacing(SP2),
    ]
    .spacing(SP2)
    .into()
}

fn notifications<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let lang = shell.lang();
    let draft = &catalog.draft;
    let urgencies = [
        (Urgency::Low, shell.t("Низька", "Low")),
        (Urgency::Normal, shell.t("Звичайна", "Normal")),
        (Urgency::Critical, shell.t("Критична", "Critical")),
    ];
    let toggle = |label: &'a str, hint: &'a str, on: bool, message: fn(bool) -> Message| {
        ui::list_row(paint, label, Some(hint), ui::switch(paint, on, Some(message)))
    };
    let mut options = vec![
        ui::list_row(
            paint,
            shell.t("Терміновість", "Urgency"),
            None,
            ui::segmented(paint, &urgencies, draft.urgency, Message::Urgency),
        ),
        toggle(
            shell.t("Кнопки дій", "Action buttons"),
            shell.t("«Відкрити» і «Нагадати пізніше»", "“Open” and “Remind me later”"),
            draft.actions,
            Message::Actions,
        ),
        toggle(
            shell.t("Відповідь у сповіщенні", "Inline reply"),
            shell.t("Поле введення під текстом", "A text field under the body"),
            draft.reply,
            Message::Reply,
        ),
        toggle(
            shell.t("Залишатися в центрі", "Resident"),
            shell.t("Не зникає після дії", "Stays after an action"),
            draft.resident,
            Message::Resident,
        ),
        toggle(
            shell.t("Тимчасове", "Transient"),
            shell.t("Не зберігається в центрі", "Skips the notification centre"),
            draft.transient,
            Message::Transient,
        ),
        toggle(shell.t("Звук", "Sound"), "message-new-instant", draft.sound, Message::Sound),
        toggle(
            shell.t("Прогрес", "Progress"),
            shell.t("Смуга прогресу в сповіщенні", "A progress bar in the notification"),
            draft.progress.is_some(),
            Message::WithProgress,
        ),
    ];
    if let Some(value) = draft.progress {
        options.push(ui::list_row(
            paint,
            shell.t("Значення", "Value"),
            None,
            container(ui::slider(paint, 0.0..=1.0, value, Message::Progress)).width(220),
        ));
    }
    options.push(ui::list_row(
        paint,
        shell.t("Тривалість", "Timeout"),
        None,
        container(ui::dropdown(
            paint,
            timeout_options(lang),
            Some(timeout_option(draft.timeout, lang)),
            "",
            |option| Message::Timeout(option.value),
        ))
        .width(180),
    ));
    page(
        shell,
        Page::Notifications,
        shell.t(
            "Складіть сповіщення, надішліть його в оболонку і спостерігайте, що з ним відбувається.",
            "Compose a notification, send it to the shell and watch what happens to it.",
        ),
        vec![
            demo(shell, shell.t("Готові сценарії", "Presets"), presets(catalog, shell)),
            demo(shell, shell.t("Чернетка", "Draft"), compose(catalog, shell)),
            ui::section(paint, shell.t("Параметри", "Options"), ui::list(paint, options)),
            demo(shell, shell.t("Журнал подій", "Event log"), event_log(catalog, shell)),
        ],
    )
}

fn presets<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let buttons = wrap(vec![
        ui::tonal(paint, shell.t("Повідомлення з відповіддю", "Message with reply"), Message::Preset(Preset::Message)),
        ui::tonal(
            paint,
            shell.t("Завантаження з прогресом", "Download with progress"),
            Message::Preset(Preset::Download),
        ),
        ui::tonal(paint, shell.t("Критичне", "Critical"), Message::Preset(Preset::Critical)),
        ui::tonal(paint, shell.t("Нагадування з діями", "Reminder with actions"), Message::Preset(Preset::Reminder)),
    ]);
    let mut content = column![buttons].spacing(SP3);
    if let Some((_, done)) = catalog.download {
        content = content.push(
            row![ui::spinner(paint, false), container(ui::progress(paint, Some(done))).width(Fill)]
                .spacing(SP3)
                .align_y(Alignment::Center),
        );
    }
    content.into()
}

fn event_log<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let lines: Element<'a, Message> = if catalog.log.is_empty() {
        caption(
            shell,
            shell.t(
                "Події з'являться тут: надсилання, натиснуті дії, відповіді, закриття.",
                "Events show up here: sends, clicked actions, replies, closings.",
            ),
        )
    } else {
        Column::with_children(
            catalog.log.iter().map(|line| text(line.as_str(), TextStyle::NUM).color(paint.fg_2()).into()),
        )
        .spacing(SP1)
        .into()
    };
    column![lines, ui::ghost(paint, shell.t("Очистити", "Clear"), Message::ClearLog)].spacing(SP3).into()
}

fn system<'a>(catalog: &'a Catalog, shell: &'a Shell<Message>) -> Element<'a, Message> {
    let paint = shell.paint();
    let previous = catalog.dock_count.checked_sub(1);
    let dock = ui::list(
        paint,
        vec![
            ui::list_row(
                paint,
                shell.t("Лічильник на доці", "Dock counter"),
                None,
                ui::stepper(paint, catalog.dock_count, previous, Some(catalog.dock_count + 1), Message::DockCount),
            ),
            ui::list_row(
                paint,
                shell.t("Прогрес на доці", "Dock progress"),
                None,
                container(ui::slider(paint, 0.0..=1.0, catalog.dock_progress, Message::DockProgress)).width(220),
            ),
            ui::list_row(
                paint,
                shell.t("Привернути увагу", "Ask for attention"),
                None,
                ui::switch(paint, catalog.dock_urgent, Some(Message::DockUrgent)),
            ),
        ],
    );
    let osd = wrap(vec![
        ui::secondary(paint, shell.t("Гучність 40 %", "Volume 40 %"), Message::Osd(Osd::Volume, 40)),
        ui::secondary(paint, shell.t("Яскравість 80 %", "Brightness 80 %"), Message::Osd(Osd::Brightness, 80)),
        ui::secondary(paint, "Caps Lock", Message::Osd(Osd::CapsLock, 0)),
    ]);
    let quick = wrap(vec![
        ui::secondary(paint, shell.t("Швидкі налаштування", "Quick settings"), Message::Quick(QuickSettings::Main)),
        ui::secondary(paint, "Wi-Fi", Message::Quick(QuickSettings::Wifi)),
        ui::secondary(paint, shell.t("Звук", "Sound"), Message::Quick(QuickSettings::Sound)),
        ui::secondary(paint, shell.t("Відкрити посилання", "Open a link"), Message::OpenLink),
    ]);
    page(
        shell,
        Page::System,
        shell.t(
            "Виклики, якими застосунок говорить з оболонкою Glide.",
            "The calls an app uses to talk to the Glide shell.",
        ),
        vec![
            ui::section(paint, shell.t("Док", "Dock"), dock),
            demo(shell, shell.t("Екранний індикатор", "On-screen indicator"), osd),
            demo(shell, shell.t("Оболонка", "Shell"), quick),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_has_a_title_in_both_languages() {
        for page in Page::ALL {
            assert!(!page.title(Lang::Uk).is_empty());
            assert_ne!(page.title(Lang::Uk), page.title(Lang::En), "{page:?} is translated");
        }
    }

    #[test]
    fn the_sidebar_search_highlight_covers_whole_characters_in_either_script() {
        for label in ["Notifications", "Сповіщення", "Ab"] {
            let (start, end) = prefix_range(label, 6);
            assert_eq!(start, 0);
            assert!(label.is_char_boundary(end), "{label}");
            assert_eq!(label[start..end].chars().count(), label.chars().count().min(6), "{label}");
        }
    }
}
