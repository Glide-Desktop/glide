use glide_sdk::Tone;
use glide_sdk::theme::{R2, R3, SP1, SP2, SP3, SP4, TextStyle};
use glide_sdk::widget::{self as ui, CropRect, Size};
use glide_sdk::window::{self, Header};
use iced::widget::{
    Column, Row, Space, column, container, image, mouse_area, opaque, row, scrollable, stack, text_input,
};
use iced::{Alignment, Color, Element, Fill, Length, Padding};

use crate::app::{App, Editor, EditorEvent, Message, SaveForm, Tool};
use crate::edit::{Filter, Output, Ratio, Slider};
use crate::i18n::Key;
use crate::icons::Icon;
use crate::view::HEADER;

pub const PANEL: f32 = 320.0;
const HINT: f32 = 60.0;

fn event(event: EditorEvent) -> Message {
    Message::Editor(event)
}

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let Some(editor) = app.editor.as_ref() else {
        return Space::new().into();
    };
    let paint = app.paint;
    let body = column![
        header(app, editor),
        row![stage(app, editor), ui::vertical_separator(&paint), panel(app, editor)].height(Fill)
    ];
    let base = ui::pane(paint.view_bg(), app.window.corner(), body);
    match &editor.saving {
        Some(form) => stack![base, save_dialog(app, form)].into(),
        None => base.into(),
    }
}

fn header<'a>(app: &'a App, editor: &'a Editor) -> Element<'a, Message> {
    let paint = app.paint;
    let tabs = ui::segmented(
        &paint,
        &[(Tool::Adjust, app.t(Key::Adjust)), (Tool::Filters, app.t(Key::Filters)), (Tool::Crop, app.t(Key::Crop))],
        editor.tool,
        |tool| event(EditorEvent::Tool(tool)),
    );
    let ready = (!editor.busy).then_some(event(EditorEvent::Done));
    let bar = Header::new("")
        .labels(app.t(Key::Maximize), app.t(Key::CloseWindow))
        .leading(ui::sized_button(
            &paint,
            app.t(Key::Cancel),
            Tone::Ghost,
            Size::Small,
            Some(event(EditorEvent::Cancel)),
        ))
        .leading(ui::sized_icon_button(
            &paint,
            Icon::Undo,
            app.t(Key::Undo),
            editor.can_undo().then_some(event(EditorEvent::Undo)),
            false,
            Size::Medium,
        ))
        .leading(ui::sized_icon_button(
            &paint,
            Icon::Redo,
            app.t(Key::Redo),
            editor.can_redo().then_some(event(EditorEvent::Redo)),
            false,
            Size::Medium,
        ))
        .center(tabs)
        .trailing(ui::sized_button(&paint, app.t(Key::Done), Tone::Accent, Size::Small, ready));
    window::header(&paint, &app.window, bar, Message::Window, Message::Close, 0.0)
}

fn stage_size(app: &App) -> (f32, f32) {
    let width = app.window.size.width - PANEL - 1.0 - 2.0 * SP4 * 2.0;
    let height = app.window.size.height - HEADER - HINT - 2.0 * SP4;
    (width.max(80.0), height.max(80.0))
}

fn fitted(size: (u32, u32), room: (f32, f32)) -> (f32, f32) {
    let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
    let scale = (room.0 / w).min(room.1 / h);
    ((w * scale).floor(), (h * scale).floor())
}

fn stage<'a>(app: &'a App, editor: &'a Editor) -> Element<'a, Message> {
    let paint = app.paint;
    let mut room = stage_size(app);
    let shot: Element<'_, Message> = if editor.tool == Tool::Crop {
        room.1 -= ui::DIAL_HEIGHT + SP4;
        match &editor.geometry {
            Some((handle, size)) => {
                let (w, h) = fitted(*size, room);
                let crop = editor.edit.crop;
                let rect = CropRect { x: crop.x, y: crop.y, w: crop.w, h: crop.h };
                let original = size.0.max(1) as f32 / size.1.max(1) as f32;
                let ratio = editor.edit.ratio.value(original, editor.edit.portrait);
                let overlay = ui::crop_overlay(rect, ratio, original, w, h, |rect| event(EditorEvent::Crop(rect)));
                column![
                    stack![image(handle.clone()).width(w).height(h), overlay],
                    ui::straighten_dial(editor.edit.tilt, |degrees| event(EditorEvent::Tilt(degrees))),
                ]
                .spacing(SP4)
                .align_x(Alignment::Center)
                .into()
            }
            None => ui::sized_spinner(&paint, 34.0),
        }
    } else {
        match &editor.preview {
            Some((handle, size)) => {
                let (w, h) = fitted(*size, room);
                let shown = if editor.compare {
                    editor.original.clone().unwrap_or_else(|| handle.clone())
                } else {
                    handle.clone()
                };
                let mut layers: Vec<Element<'_, Message>> = vec![image(shown).width(w).height(h).into()];
                if editor.compare {
                    layers.push(container(ui::format_badge(app.t(Key::Original))).padding(12).into());
                }
                stack(layers).width(w).height(h).into()
            }
            None => ui::sized_spinner(&paint, 34.0),
        }
    };
    let compare = mouse_area(ui::glass_button(
        &paint,
        ui::text(app.t(Key::Compare), TextStyle::CAP).color(ui::GLASS_INK),
        "",
        Some(Message::Ignore),
    ))
    .on_press(event(EditorEvent::Compare(true)))
    .on_release(event(EditorEvent::Compare(false)));
    let hint = row![
        compare,
        ui::text(app.t(Key::Or), TextStyle::CAP).color(Color { a: 0.72, ..ui::GLASS_INK }),
        ui::format_badge(app.t(Key::Space)),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let mut column = column![container(shot).center(Fill)].align_x(Alignment::Center);
    if editor.tool != Tool::Crop {
        column = column.push(container(hint).center_x(Fill).height(HINT));
    }
    ui::pane(ui::STAGE, 0.0, column.padding(SP4)).width(Fill).into()
}

fn slider_label(slider: Slider) -> Key {
    match slider {
        Slider::Exposure => Key::Exposure,
        Slider::Contrast => Key::Contrast,
        Slider::Shadows => Key::Shadows,
        Slider::Saturation => Key::Saturation,
        Slider::Warmth => Key::Warmth,
        Slider::Vignette => Key::Vignette,
    }
}

fn sliders<'a>(app: &'a App, editor: &'a Editor, title: Key, list: &[Slider]) -> Element<'a, Message> {
    let paint = app.paint;
    let mut block =
        Column::new().spacing(SP3).push(ui::text(app.t(title).to_uppercase(), TextStyle::MICRO).color(paint.fg_2()));
    for &slider in list {
        block = block.push(ui::adjust_slider(
            &paint,
            app.t(slider_label(slider)),
            editor.edit.get(slider),
            slider.min(),
            move |value| event(EditorEvent::Slider(slider, value)),
            Some((event(EditorEvent::Reset(slider)), app.t(Key::Reset))),
        ));
    }
    container(block).padding(Padding::from([SP3, 18.0])).into()
}

const fn filter_name(filter: Filter) -> Key {
    match filter {
        Filter::None => Key::FilterNone,
        Filter::Vivid => Key::FilterVivid,
        Filter::Warm => Key::FilterWarm,
        Filter::Cool => Key::FilterCool,
        Filter::Mono => Key::FilterMono,
        Filter::Noir => Key::FilterNoir,
        Filter::Film => Key::FilterFilm,
        Filter::Drama => Key::FilterDrama,
        Filter::Fade => Key::FilterFade,
    }
}

fn panel<'a>(app: &'a App, editor: &'a Editor) -> Element<'a, Message> {
    let paint = app.paint;
    let content: Element<'_, Message> = match editor.tool {
        Tool::Adjust => column![
            container(
                row![
                    container(ui::button(&paint, app.t(Key::Auto), Tone::Tonal, Some(event(EditorEvent::Auto))))
                        .width(Fill),
                    ui::outline(&paint, app.t(Key::ResetAll), event(EditorEvent::ResetAll)),
                ]
                .spacing(SP2)
            )
            .padding(Padding::from([SP4, 18.0]).bottom(SP3)),
            container(ui::histogram(&paint, editor.histogram)).padding(Padding::from([0.0, 18.0])),
            sliders(app, editor, Key::Light, &Slider::LIGHT),
            ui::separator(&paint),
            sliders(app, editor, Key::Colour, &Slider::COLOR),
            ui::separator(&paint),
            sliders(app, editor, Key::Effects, &Slider::EFFECTS),
        ]
        .into(),
        Tool::Filters => filters(app, editor),
        Tool::Crop => crop(app, editor),
    };
    container(scrollable(content).height(Fill))
        .width(PANEL)
        .height(Fill)
        .style(move |_| glide_sdk::glide_iced::style::rounded(paint.sidebar_bg(), 0.0))
        .into()
}

fn filters<'a>(app: &'a App, editor: &'a Editor) -> Element<'a, Message> {
    let paint = app.paint;
    let side = ((PANEL - 36.0 - 2.0 * 10.0) / 3.0).floor();
    let cells: Vec<Element<'_, Message>> = editor
        .swatches
        .iter()
        .map(|(filter, handle)| {
            let on = editor.edit.filter == *filter;
            let picture = ui::frame_button(
                &paint,
                image(handle.clone())
                    .width(side - 8.0)
                    .height(side - 8.0)
                    .content_fit(iced::ContentFit::Cover)
                    .border_radius(R2),
                on,
                event(EditorEvent::Filter(*filter)),
            );
            column![
                picture,
                ui::text(app.t(filter_name(*filter)), TextStyle { weight: 500, ..TextStyle::CAP }).color(if on {
                    paint.acc_text()
                } else {
                    paint.fg_1()
                }),
            ]
            .spacing(SP1)
            .align_x(Alignment::Center)
            .width(side)
            .into()
        })
        .collect();
    let mut grid = Column::new().spacing(14);
    let mut cells = cells.into_iter().peekable();
    while cells.peek().is_some() {
        grid = grid.push(Row::with_children(cells.by_ref().take(3)).spacing(10));
    }
    column![
        ui::text(app.t(Key::Filters).to_uppercase(), TextStyle::MICRO).color(paint.fg_2()),
        grid,
        ui::adjust_slider(
            &paint,
            app.t(Key::Strength),
            editor.edit.strength,
            0,
            |value| event(EditorEvent::Strength(value)),
            None
        ),
        ui::text(app.t(Key::FilterNote), TextStyle::CALL).color(paint.fg_2()),
    ]
    .spacing(SP4)
    .padding(Padding::from([SP4, 18.0]))
    .into()
}

fn ratio_label(app: &App, ratio: Ratio) -> String {
    match ratio {
        Ratio::Free => app.t(Key::Free).into(),
        Ratio::Original => app.t(Key::Original).into(),
        Ratio::Square => "1:1".into(),
        Ratio::R3x2 => "3:2".into(),
        Ratio::R4x3 => "4:3".into(),
        Ratio::R5x4 => "5:4".into(),
        Ratio::R16x9 => "16:9".into(),
        Ratio::R9x16 => "9:16".into(),
    }
}

fn crop<'a>(app: &'a App, editor: &'a Editor) -> Element<'a, Message> {
    let paint = app.paint;
    let orientation = ui::segmented(
        &paint,
        &[(false, app.t(Key::Landscape)), (true, app.t(Key::Portrait))],
        editor.edit.portrait,
        |portrait| event(EditorEvent::Portrait(portrait)),
    );
    let ratios: Vec<Element<'_, Message>> = Ratio::ALL
        .iter()
        .map(|&ratio| {
            container(ui::sized_toggle_button(
                &paint,
                ratio_label(app, ratio),
                editor.edit.ratio == ratio,
                Size::Medium,
                Some(move |_| event(EditorEvent::Ratio(ratio))),
            ))
            .width(Fill)
            .into()
        })
        .collect();
    let mut ratio_rows = Column::new().spacing(6);
    let mut ratios = ratios.into_iter().peekable();
    while ratios.peek().is_some() {
        ratio_rows = ratio_rows.push(Row::with_children(ratios.by_ref().take(3)).spacing(6));
    }
    let tools = row![
        ui::sized_icon_button(
            &paint,
            Icon::RotateLeft,
            app.t(Key::RotateLeft),
            Some(event(EditorEvent::Turn(-1))),
            false,
            Size::Large
        ),
        ui::sized_icon_button(
            &paint,
            Icon::RotateRight,
            app.t(Key::RotateRight),
            Some(event(EditorEvent::Turn(1))),
            false,
            Size::Large
        ),
        ui::sized_icon_button(
            &paint,
            Icon::FlipH,
            app.t(Key::FlipH),
            Some(event(EditorEvent::FlipH)),
            editor.edit.flip_h,
            Size::Large
        ),
        ui::sized_icon_button(
            &paint,
            Icon::FlipV,
            app.t(Key::FlipV),
            Some(event(EditorEvent::FlipV)),
            editor.edit.flip_v,
            Size::Large
        ),
    ]
    .spacing(6);
    let level = (editor.edit.tilt != 0.0).then_some(event(EditorEvent::Level));
    column![
        row![
            ui::text(app.t(Key::Ratios).to_uppercase(), TextStyle::MICRO).color(paint.fg_2()),
            Space::new().width(Fill),
            orientation
        ]
        .align_y(Alignment::Center),
        ratio_rows,
        ui::separator(&paint),
        ui::text(app.t(Key::RotateFlip).to_uppercase(), TextStyle::MICRO).color(paint.fg_2()),
        tools,
        ui::sized_button(&paint, app.t(Key::Level), Tone::Tonal, Size::Small, level),
        ui::separator(&paint),
        ui::noted_banner(&paint, ui::Severity::Accent, Icon::Info, "", app.t(Key::CropNote), Vec::new()),
        ui::link(&paint, app.t(Key::ResetCrop), Some(event(EditorEvent::ResetCrop))),
    ]
    .spacing(SP3)
    .padding(Padding::from([SP4, 18.0]))
    .into()
}

fn save_dialog<'a>(app: &'a App, form: &'a SaveForm) -> Element<'a, Message> {
    let paint = app.paint;
    let photo = app.editor.as_ref().map(|editor| &app.photos[editor.photo]);
    let format = photo.map_or("", |photo| photo.format.name);
    let option = |selected: bool,
                  enabled: bool,
                  title: &'a str,
                  hint: String,
                  extra: Option<Element<'a, Message>>,
                  message: Message| {
        let mut text = column![
            ui::text(title, TextStyle::HEAD).color(paint.fg_1()),
            ui::text(hint, TextStyle::CAP).color(paint.fg_2()),
        ]
        .spacing(2)
        .width(Fill);
        if let Some(extra) = extra {
            text = text.push(Space::new().height(SP2)).push(extra);
        }
        let line = row![ui::radio_mark(&paint, selected, enabled.then(|| message.clone())), text].spacing(SP3);
        let ring = paint.acc_bg();
        let soft = paint.acc_soft();
        let line_colour = paint.line();
        let view_bg = paint.view_bg();
        let card = container(line).padding([12.0, 14.0]).width(Fill).style(move |_| container::Style {
            background: Some(iced::Background::Color(if selected { soft } else { view_bg })),
            border: iced::Border { color: if selected { ring } else { line_colour }, width: 1.0, radius: R2.into() },
            ..container::Style::default()
        });
        let card: Element<'_, Message> =
            if enabled { mouse_area(card).on_press(message).into() } else { ui::inert(&paint, false, card) };
        card
    };
    let name = text_input(app.t(Key::CopyName), &form.name)
        .on_input(|name| event(EditorEvent::SaveName(name)))
        .on_submit(event(EditorEvent::SaveConfirm));
    let fields = row![
        container(ui::field(&paint, name)).width(Fill),
        container(ui::dropdown(
            &paint,
            Output::ALL.to_vec().into_iter().map(OutputLabel).collect(),
            Some(OutputLabel(form.output)),
            app.t(Key::CopyFormat),
            |output: OutputLabel| event(EditorEvent::SaveOutput(output.0))
        ))
        .width(108),
    ]
    .spacing(6);
    let copy = option(
        !form.replace,
        true,
        app.t(Key::SaveCopy),
        app.t(Key::SaveCopyHint).into(),
        (!form.replace).then(|| fields.into()),
        event(EditorEvent::SaveReplace(false)),
    );
    let replace_hint = if form.can_replace {
        app.t(Key::ReplaceHint).to_string()
    } else {
        app.t(Key::ReplaceUnavailable).replace("{f}", format)
    };
    let replace = option(
        form.replace,
        form.can_replace,
        app.t(Key::ReplaceOriginal),
        replace_hint,
        None,
        event(EditorEvent::SaveReplace(true)),
    );
    let confirm = if form.replace { Key::ReplaceAction } else { Key::SaveCopyAction };
    let body = if form.can_replace { app.t(Key::SaveReplaceBody) } else { app.t(Key::SaveCopyBody) };
    let card = column![
        column![
            ui::text(app.t(Key::SaveTitle), TextStyle::T3).color(paint.fg_1()),
            ui::text(body, TextStyle::CALL).color(paint.fg_2()),
        ]
        .spacing(SP1),
        column![copy, replace].spacing(SP2),
        row![
            ui::button(&paint, app.t(Key::Discard), Tone::Danger, Some(event(EditorEvent::SaveDiscard))),
            Space::new().width(Fill),
            ui::secondary(&paint, app.t(Key::Cancel), event(EditorEvent::SaveCancel)),
            ui::primary(&paint, app.t(confirm), event(EditorEvent::SaveConfirm)),
        ]
        .spacing(SP2),
    ]
    .spacing(14)
    .width(480);
    let card = container(card).padding(22).style(move |_| glide_sdk::glide_iced::style::popover(&paint));
    let card = glide_sdk::glide_iced::widgets::surface(card, &paint, glide_sdk::theme::Level::El3, R3);
    let scrim = mouse_area(
        container(Space::new())
            .width(Fill)
            .height(Fill)
            .style(|_| glide_sdk::glide_iced::style::rounded(Color { a: 0.42, ..ui::STAGE }, 0.0)),
    )
    .on_press(event(EditorEvent::SaveCancel));
    stack![scrim, container(opaque(card)).center_x(Fill).padding(Padding::default().top(92.0)).height(Length::Shrink)]
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OutputLabel(Output);

impl std::fmt::Display for OutputLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.label())
    }
}
