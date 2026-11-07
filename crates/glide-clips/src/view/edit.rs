use glide_sdk::theme::TextStyle;
use glide_sdk::widget::player::{self, clock, glass_tag, trim_bar};
use glide_sdk::widget::{self as ui, glass_style};
use iced::widget::{Space, button, column, container, image, mouse_area, row, stack};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length, Padding};

use crate::app::{App, Media, Message, Mode};
use crate::i18n::{self, Key};
use crate::icons::Icon;
use crate::view::player::ink;

const PANEL_GAP: f32 = 24.0;
const CLIP_HEIGHT: f32 = 60.0;
const CLIP_RADIUS: f32 = 6.0;

fn glass_panel<'a>(content: impl Into<Element<'a, Message>>, padding: Padding) -> container::Container<'a, Message> {
    container(content)
        .padding(padding)
        .style(|_| container::Style { border: Border { radius: 12.0.into(), ..glass_style().border }, ..glass_style() })
}

fn glass_button(
    label: &str, glyph: Option<Icon>, primary: bool, height: f32, message: Option<Message>,
) -> Element<'_, Message> {
    let mut content = row![].spacing(6).align_y(Alignment::Center);
    if let Some(glyph) = glyph {
        content = content.push(ui::icon(glyph, 16.0, if primary { Color::WHITE } else { ink() }));
    }
    content = content.push(
        ui::text(label, TextStyle { size_px: 13.0, line_height_px: 16.0, weight: 500, ..TextStyle::CALL })
            .color(if primary { Color::WHITE } else { ink() }),
    );
    let accent = glide_sdk::Paint::default().acc_bg();
    let enabled = message.is_some();
    button(container(content).center_y(height).padding([0.0, 12.0]))
        .padding(0)
        .height(height)
        .on_press_maybe(message)
        .style(move |_, status| {
            let hover = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let background = match (primary, hover) {
                (true, false) => accent,
                (true, true) => Color { a: 0.9, ..accent },
                (false, false) => Color { a: 0.12, ..Color::WHITE },
                (false, true) => Color { a: 0.2, ..Color::WHITE },
            };
            button::Style {
                background: Some(Background::Color(if enabled {
                    background
                } else {
                    Color { a: 0.06, ..Color::WHITE }
                })),
                text_color: ink(),
                border: Border { radius: 8.0.into(), ..Border::default() },
                ..button::Style::default()
            }
        })
        .into()
}

fn tool(glyph: Icon, label: &str, on: bool, message: Option<Message>) -> Element<'_, Message> {
    let tint = if on { Color::from_rgb8(0xa7, 0xc8, 0xf6) } else { ink() };
    let face = button(container(ui::icon(glyph, 18.0, tint)).center(32.0))
        .padding(0)
        .width(32)
        .height(32)
        .on_press_maybe(message)
        .style(move |_, status| {
            let layer = if on {
                0.22
            } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                0.12
            } else {
                0.0
            };
            button::Style {
                background: Some(Background::Color(if on {
                    Color { a: layer, ..Color::from_rgb8(91, 155, 236) }
                } else {
                    Color { a: layer, ..Color::WHITE }
                })),
                border: Border { radius: 8.0.into(), ..Border::default() },
                ..button::Style::default()
            }
        });
    ui::tooltip(&glide_sdk::Paint::default(), face, label)
}

fn divider<'a>() -> Element<'a, Message> {
    container(container(Space::new().width(1).height(18)).style(|_| container::Style {
        background: Some(Background::Color(Color { a: 0.16, ..Color::WHITE })),
        ..container::Style::default()
    }))
    .padding([0.0, 6.0])
    .into()
}

fn summary<'a>(
    app: &'a App, media: &'a Media, glyph: Icon, heading: Key, tail: String, reset: Option<Message>,
) -> Element<'a, Message> {
    let name = media.title.clone();
    let mut line = row![
        ui::icon(glyph, 16.0, ink()),
        ui::text(format!("{} · ", app.t(heading)), TextStyle::CALL).color(ink()),
        ui::text(name, TextStyle { weight: 600, ..TextStyle::CALL }).color(ink()).wrapping(ui::Wrapping::None),
        ui::text(tail, TextStyle { size_px: 13.0, line_height_px: 16.0, ..TextStyle::NUM })
            .color(Color { a: 0.8, ..ink() }),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let right = if reset.is_some() { 6.0 } else { 14.0 };
    if let Some(reset) = reset {
        line = line.push(Space::new().width(6)).push(glass_button(app.t(Key::Reset), None, false, 26.0, Some(reset)));
    }
    container(glass_panel(line, Padding::from([6.0, 6.0]).left(14.0).right(right)))
        .width(Fill)
        .center_x(Fill)
        .padding(Padding::from([16.0, 0.0]))
        .into()
}

fn keycap(label: &str) -> Element<'_, Message> {
    container(ui::text(label, TextStyle { size_px: 10.5, line_height_px: 16.0, ..TextStyle::NUM }).color(ink()))
        .padding([0.0, 5.0])
        .style(|_| container::Style {
            border: Border { radius: 4.0.into(), color: Color { a: 0.22, ..Color::WHITE }, width: 1.0 },
            ..container::Style::default()
        })
        .into()
}

fn hint<'a>(parts: Vec<(Vec<&'a str>, &'a str)>) -> Element<'a, Message> {
    let line = parts.into_iter().fold(row![].spacing(10).align_y(Alignment::Center), |line, (keys, label)| {
        let keys = keys.into_iter().fold(row![].spacing(3), |keys, key| keys.push(keycap(key)));
        line.push(
            row![keys, ui::text(label, TextStyle::CAP).color(Color { a: 0.82, ..ink() })]
                .spacing(5)
                .align_y(Alignment::Center),
        )
    });
    glass_panel(line, Padding::from([6.0, 10.0])).into()
}

#[must_use]
pub fn trim<'a>(app: &'a App, media: &'a Media) -> Element<'a, Message> {
    let Mode::Trim { start, end, head } = app.mode else { return Space::new().into() };
    let length = media.length();
    let kept = length.mul_f32((end - start).max(0.0));
    let tail = app.t(Key::WillRemain).replace("{n}", &clock(kept, true)).replace("{total}", &clock(length, true));
    let top = summary(app, media, Icon::Scissors, Key::Trimming, tail, Some(Message::TrimReset));
    let labels = (clock(length.mul_f32(start), true), clock(length.mul_f32(end), true));
    let least = (0.2 / length.as_secs_f32().max(0.2)).min(0.5);
    let bar = trim_bar(
        media.strip.clone(),
        (start, end),
        head,
        labels,
        glide_sdk::Paint::default().acc_bg(),
        Message::TrimChanged,
    )
    .least(least);
    let play = button(container(ui::icon(Icon::Play, 15.0, Color::from_rgb8(0x18, 0x17, 0x1b))).center(36.0))
        .padding(0)
        .width(36)
        .height(36)
        .on_press(Message::TrimPlay)
        .style(|_, _| button::Style {
            background: Some(Background::Color(ink())),
            border: Border { radius: 18.0.into(), ..Border::default() },
            ..button::Style::default()
        });
    let panel = glass_panel(
        row![
            ui::tooltip(&glide_sdk::Paint::default(), play, app.t(Key::PlaySelection)),
            container(bar).width(Fill).padding(Padding::from([0.0, 4.0])),
            glass_button(app.t(Key::Cancel), None, false, 34.0, Some(Message::TrimCancel)),
            glass_button(app.t(Key::TrimButton), None, true, 34.0, Some(Message::TrimApply)),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
        Padding::from([10.0, 12.0]),
    );
    let hints = hint(vec![
        (vec!["I", "O"], app.t(Key::HintMarks)),
        (vec!["←", "→"], app.t(Key::HintFrame)),
        (vec!["Enter"], app.t(Key::HintTrim)),
        (vec!["Esc"], app.t(Key::HintCancel)),
    ]);
    let bottom = column![container(hints).width(Fill).align_x(Alignment::End), panel].spacing(12);
    stack![top, container(bottom).width(Fill).height(Fill).align_y(Alignment::End).padding(PANEL_GAP)]
        .width(Fill)
        .height(Fill)
        .into()
}

fn clip_frames<'a>(media: &'a Media, index: usize, width: f32) -> Element<'a, Message> {
    let total = media.timeline.total().as_secs_f64().max(0.001);
    let from = media.timeline.offset_of(index).as_secs_f64() / total;
    let to = from + media.timeline.clips[index].length().as_secs_f64() / total;
    let count = media.strip.len().max(1) as f64;
    let picked: Vec<Option<iced::widget::image::Handle>> = media
        .strip
        .iter()
        .enumerate()
        .filter(|(slot, _)| {
            let centre = (*slot as f64 + 0.5) / count;
            centre >= from && centre < to
        })
        .map(|(_, frame)| frame.clone())
        .collect();
    let picked = if picked.is_empty() {
        vec![media.strip.get(((from * count) as usize).min(media.strip.len().saturating_sub(1))).cloned().flatten()]
    } else {
        picked
    };
    let cells = picked.len().max(1);
    let cell = (width / cells as f32).floor().max(1.0);
    let strip = picked.into_iter().enumerate().fold(row![], |line, (slot, frame)| {
        let mut corners = iced::border::Radius::default();
        if slot == 0 {
            corners = corners.left(CLIP_RADIUS);
        }
        if slot + 1 == cells {
            corners = corners.right(CLIP_RADIUS);
        }
        let content: Element<'a, Message> = match frame {
            Some(handle) => image(handle)
                .width(cell)
                .height(CLIP_HEIGHT)
                .content_fit(iced::ContentFit::Cover)
                .border_radius(corners)
                .into(),
            None => container(Space::new().width(cell).height(CLIP_HEIGHT))
                .style(move |_| container::Style {
                    background: Some(Background::Color(Color::from_rgb8(18, 17, 20))),
                    border: Border { radius: corners, ..Border::default() },
                    ..container::Style::default()
                })
                .into(),
        };
        line.push(content)
    });
    container(strip).width(width).height(CLIP_HEIGHT).clip(true).into()
}

fn clip_caption<'a>(app: &'a App, media: &'a Media, selected: usize) -> Element<'a, Message> {
    let clip = media.timeline.clips.get(selected).copied();
    let dot = || ui::text("·", TextStyle::CAP).color(Color { a: 0.7, ..ink() });
    let mut parts = row![glass_tag(&app.t(Key::ClipLabel).replace("{n}", &(selected + 1).to_string()))]
        .spacing(8)
        .align_y(Alignment::Center);
    if let Some(clip) = clip {
        let span = format!("{} – {}", clock(clip.start, true), clock(clip.end, true));
        parts = parts
            .push(ui::text(span, TextStyle { size_px: 12.0, line_height_px: 16.0, ..TextStyle::NUM }).color(ink()));
        if let Some(source) = clip.from.checked_sub(1).and_then(|index| media.timeline.extra.get(index)) {
            parts = parts
                .push(dot())
                .push(ui::text(source.title.clone(), TextStyle::CAP).color(ink()).wrapping(ui::Wrapping::None));
        }
        if clip.transform.turn != glide_video::export::Turn::None {
            let turned = app.t(Key::Turned).replace("{n}", &clip.transform.turn.degrees().to_string());
            parts = parts.push(dot()).push(ui::text(turned, TextStyle::CAP).color(ink()));
        }
        if clip.transform.flip_h || clip.transform.flip_v {
            parts = parts.push(dot()).push(ui::text(app.t(Key::Flipped), TextStyle::CAP).color(ink()));
        }
    }
    glass_panel(parts, Padding::from([6.0, 10.0])).into()
}

#[must_use]
pub fn clips<'a>(app: &'a App, media: &'a Media) -> Element<'a, Message> {
    let Mode::Clips { dragging } = app.mode else { return Space::new().into() };
    let timeline = &media.timeline;
    let count = timeline.clips.len();
    let tail = format!("{} · {}", i18n::clips(app.lang, count), clock(timeline.total(), true));
    let top = summary(app, media, Icon::Split, Key::ClipsTitle, tail, None);
    let selected = timeline.selected.min(count.saturating_sub(1));
    let transform = timeline.clips.get(selected).map(|clip| clip.transform).unwrap_or_default();
    let panel_width = (app.window.size.width - 2.0 * PANEL_GAP - 24.0).max(200.0);
    let tools = row![
        glass_button(app.t(Key::Split), Some(Icon::Split), false, 30.0, Some(Message::SplitClip)),
        glass_button(app.t(Key::AddClip), Some(Icon::Plus), false, 30.0, Some(Message::AddClip)),
        divider(),
        tool(Icon::RotateLeft, app.t(Key::RotateLeft), false, Some(Message::TurnClip(false))),
        tool(
            Icon::RotateRight,
            app.t(Key::RotateRight),
            transform.turn != glide_video::export::Turn::None,
            Some(Message::TurnClip(true))
        ),
        tool(Icon::FlipH, app.t(Key::FlipHorizontal), transform.flip_h, Some(Message::FlipClip(true))),
        tool(Icon::FlipV, app.t(Key::FlipVertical), transform.flip_v, Some(Message::FlipClip(false))),
        divider(),
        tool(Icon::Trash, app.t(Key::RemoveClip), false, (count > 1).then_some(Message::RemoveClip)),
        Space::new().width(Fill),
        glass_button(app.t(Key::Cancel), None, false, 34.0, Some(Message::ClipsDone)),
        glass_button(app.t(Key::ExportCopy), None, true, 34.0, Some(Message::ExportOpen)),
    ]
    .spacing(2)
    .align_y(Alignment::Center)
    .height(40);
    let total = timeline.total().as_secs_f32().max(0.001);
    let usable = (panel_width - 8.0 * (count + 1) as f32 - CLIP_HEIGHT).max(60.0);
    let mut line = row![].spacing(8).align_y(Alignment::Center);
    for (index, clip) in timeline.clips.iter().enumerate() {
        if dragging.is_some_and(|from| from != index) && app.hovered_clip == Some(index) {
            line = line.push(container(Space::new().width(6).height(CLIP_HEIGHT)).style(|_| container::Style {
                background: Some(Background::Color(glide_sdk::Paint::default().acc_bg())),
                border: Border { radius: 3.0.into(), ..Border::default() },
                ..container::Style::default()
            }));
        }
        let width = (usable * clip.length().as_secs_f32() / total).max(48.0);
        let frames = clip_frames(media, index, width);
        let mut face = stack![frames];
        if clip.transform.turn != glide_video::export::Turn::None {
            face = face.push(
                container(player::frame_tag(Some(Icon::RotateRight), &format!("{}°", clip.transform.turn.degrees())))
                    .padding(4),
            );
        }
        let length = clip_length(clip.length());
        face = face.push(
            container(length)
                .width(width)
                .height(CLIP_HEIGHT)
                .align_x(Alignment::End)
                .align_y(Alignment::End)
                .padding(4),
        );
        let picked = index == selected;
        let lifted = dragging == Some(index);
        let ring = glide_sdk::Paint::default().acc_bg();
        face = face.push(container(Space::new().width(width).height(CLIP_HEIGHT)).style(move |_| container::Style {
            border: Border {
                radius: CLIP_RADIUS.into(),
                color: if picked { ring } else { Color { a: 0.14, ..Color::WHITE } },
                width: if picked { 3.0 } else { 1.0 },
            },
            ..container::Style::default()
        }));
        let bordered = container(face).width(width).height(CLIP_HEIGHT).clip(true);
        let bordered =
            if lifted { glide_sdk::glide_iced::widgets::lift(bordered, 1.03, -12.0) } else { bordered.into() };
        let area = mouse_area(bordered)
            .on_press(Message::DragClip(Some(index)))
            .on_release(match dragging {
                Some(from) if from != index => Message::MoveClip(from, index),
                _ => Message::SelectClip(index),
            })
            .on_enter(Message::HoverClip(Some(index)))
            .interaction(iced::mouse::Interaction::Grab);
        line = line.push(area);
    }
    line = line.push(add_tile(app));
    let caption = clip_caption(app, media, selected);
    let hints = glass_panel(
        ui::text(app.t(Key::ClipsHint), TextStyle::CAP).color(Color { a: 0.82, ..ink() }),
        Padding::from([6.0, 10.0]),
    );
    let panel = glass_panel(
        column![tools, container(line).height(76).align_y(Alignment::Center)].spacing(6),
        Padding::from([8.0, 12.0]).bottom(12.0),
    );
    let above = row![caption, Space::new().width(Fill), hints].align_y(Alignment::End);
    let bottom = column![above, panel].spacing(12);
    stack![top, container(bottom).width(Fill).height(Fill).align_y(Alignment::End).padding(PANEL_GAP)]
        .width(Fill)
        .height(Fill)
        .into()
}

fn add_tile(app: &App) -> Element<'_, Message> {
    let face = button(container(ui::icon(Icon::Plus, 18.0, Color { a: 0.8, ..ink() })).center(CLIP_HEIGHT))
        .padding(0)
        .width(CLIP_HEIGHT)
        .height(CLIP_HEIGHT)
        .on_press(Message::AddClip)
        .style(|_, status| {
            let layer = if matches!(status, button::Status::Hovered | button::Status::Pressed) { 0.12 } else { 0.0 };
            button::Style {
                background: Some(Background::Color(Color { a: layer, ..Color::WHITE })),
                border: Border { radius: CLIP_RADIUS.into(), color: Color { a: 0.3, ..Color::WHITE }, width: 1.5 },
                ..button::Style::default()
            }
        });
    ui::tooltip(&glide_sdk::Paint::default(), face, app.t(Key::AddClip))
}

fn clip_length<'a>(length: std::time::Duration) -> Element<'a, Message> {
    container(
        ui::text(player::clock(length, false), TextStyle { size_px: 10.5, line_height_px: 14.0, ..TextStyle::NUM })
            .color(Color::WHITE),
    )
    .padding([0.0, 4.0])
    .style(|_| container::Style {
        background: Some(Background::Color(Color { a: 0.62, ..Color::BLACK })),
        border: Border { radius: 4.0.into(), ..Border::default() },
        ..container::Style::default()
    })
    .into()
}

#[must_use]
pub fn unused_length() -> Length {
    Length::Shrink
}
