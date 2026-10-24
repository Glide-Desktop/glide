use std::path::{Path, PathBuf};

use glide_sdk::theme::TextStyle;
use glide_sdk::widget::text;
use iced::widget::{Row as IcedRow, column, container, row, space};
use iced::{Alignment, Color, ContentFit, Element, Fill, Length, Task};

use super::common::gated;
use super::{Ctx, Pane};
use crate::i18n::Key as K;
use crate::icons::Icon;
use crate::services::config::{ConfigStore, WallpaperFit};
use crate::services::run::blocking;
use crate::services::wallpaper::{self, Gallery, Source, Thumbnail};
use crate::theme::{self, Accent, ThemeState};
use crate::widgets::{self, Group, Row};
use glide_sdk::widget::mock as sketch;

mod tr {
    crate::i18n::catalogue! {
        Current => "Current wallpaper", "Поточні шпалери";
        Builtin => "Glide", "Glide";
        BuiltinName => "Glide default", "Типові шпалери Glide";
        BuiltinHint => "Built in, always available", "Вбудовані, завжди доступні";
        SolidName => "Solid colour", "Суцільний колір";
        FitHint => "How the picture fills screens of a different shape", "Як зображення заповнює екрани іншої форми";
        FitCoverHint => "Crops to fill", "Обрізає, щоб заповнити";
        FitContainHint => "Shows all of it", "Показує повністю";
        FitCenterHint => "Original size", "Оригінальний розмір";
        FitTileHint => "Repeats", "Повторює";
        Backdrop => "Background colour", "Колір тла";
        BackdropHint => "Fills the screen around a picture that does not cover it", "Заповнює екран навколо зображення, яке його не покриває";
        Yours => "Your pictures", "Ваші зображення";
        YoursHint => "From ~/Pictures, wallpapers and user desktop folders", "З ~/Pictures, шпалер і користувацьких тек";
        NoYours => "No pictures yet", "Поки немає зображень";
        NoYoursHint => "Put JPEG or PNG files into ~/Pictures/Wallpapers", "Покладіть файли JPEG або PNG у ~/Pictures/Wallpapers";
        OpenFolder => "Open folder", "Відкрити теку";
        System => "System wallpapers", "Системні шпалери";
        SystemHint => "Installed with your desktop environments and distribution", "Встановлені зі стільничними середовищами та дистрибутивом";
        Colours => "Solid colours", "Суцільні кольори";
        ColoursHint => "A flat colour instead of a picture", "Рівний колір замість зображення";
        CustomColour => "Custom colour", "Власний колір";
        CustomColourHint => "Hex value such as #1d2433", "Шістнадцяткове значення, як-от #1d2433";
        ColourInvalid => "Use six hex digits, like #1d2433", "Потрібно шість шістнадцяткових цифр, як-от #1d2433";
        SolidFailed => "Could not create the colour image:", "Не вдалося створити зображення кольору:";
        FolderFailed => "Could not open the folder:", "Не вдалося відкрити теку:";
        Pixels => "px", "пкс";
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Wallpapers(Gallery),
    Reload,
    Pick(PathBuf),
    UseBuiltin,
    SetFit(WallpaperFit),
    CustomPath(String),
    ApplyCustom,
    CustomThumbnail(Option<Thumbnail>),
    Solid([u8; 3]),
    SolidHex(String),
    ApplySolidHex,
    SolidReady([u8; 3], Result<PathBuf, String>),
    Backdrop([u8; 3]),
    OpenFolder,
    FolderOpened(Result<(), String>),
}

#[allow(clippy::struct_excessive_bools)] // independent loading and validation flags
#[derive(Debug, Default)]
pub struct Page {
    gallery: Gallery,
    loading: bool,
    loaded: bool,
    custom_path: String,
    custom_invalid: bool,
    solid_hex: String,
    solid_invalid: bool,
    error: Option<(tr::Key, String)>,
}

pub const PALETTE: &[[u8; 3]] = &[[0x1d, 0x24, 0x33], [0x12, 0x14, 0x18], [0x3a, 0x3d, 0x44], [0xe9, 0xe5, 0xdc]];

#[must_use]
pub fn palette() -> Vec<[u8; 3]> {
    PALETTE
        .iter()
        .copied()
        .chain(Accent::ALL.iter().map(|accent| channel_bytes(theme::to_iced_color(accent.ramp()[4]))))
        .collect()
}

fn channel_bytes(color: Color) -> [u8; 3] {
    let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    [byte(color.r), byte(color.g), byte(color.b)]
}

fn to_color([r, g, b]: [u8; 3]) -> Color {
    Color::from_rgb8(r, g, b)
}

#[must_use]
pub fn backdrop(config: &ConfigStore) -> [u8; 3] {
    config
        .get_str("general", "background")
        .and_then(|text| {
            let digits = text.trim().trim_start_matches('#');
            digits.get(..6).and_then(wallpaper::parse_hex)
        })
        .unwrap_or([0x22, 0x2c, 0x3c])
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "wallpaper",
        "background",
        "picture",
        "image",
        "solid",
        "colour",
        "color",
        "шпалер",
        "фон",
        "тло",
        "зображен",
        "колір",
    ];

    fn open(&mut self, _c: Ctx, config: &ConfigStore) -> Task<Msg> {
        if self.loaded || self.loading {
            return Task::none();
        }
        self.load(config)
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Wallpapers(gallery) => {
                self.gallery = gallery;
                self.loading = false;
                self.loaded = true;
            }
            Msg::Reload => {
                if !self.loading {
                    return self.load(config);
                }
            }
            Msg::Pick(path) => config.set_wallpaper(Some(&path)),
            Msg::UseBuiltin => config.set_wallpaper(None),
            Msg::SetFit(fit) => config.set_wallpaper_fit(fit),
            Msg::CustomPath(path) => {
                self.custom_path = path;
                self.custom_invalid = false;
            }
            Msg::ApplyCustom => {
                let expanded = expand_home(self.custom_path.trim());
                if expanded.is_file() && wallpaper::is_image(&expanded) {
                    config.set_wallpaper(Some(&expanded));
                    self.custom_path.clear();
                    if !self.has(&expanded) {
                        return Task::perform(blocking(move || wallpaper::thumbnail(&expanded)), |thumb| {
                            Msg::CustomThumbnail(thumb.flatten())
                        });
                    }
                } else {
                    self.custom_invalid = true;
                }
            }
            Msg::CustomThumbnail(Some(thumb)) => {
                if !self.has(&thumb.path) {
                    self.gallery.items.insert(0, thumb);
                }
            }
            Msg::CustomThumbnail(None) => {}
            Msg::Solid(rgb) => return Self::apply_solid(rgb),
            Msg::SolidHex(text) => {
                self.solid_hex = text;
                self.solid_invalid = false;
            }
            Msg::ApplySolidHex => match wallpaper::parse_hex(&self.solid_hex) {
                Some(rgb) => {
                    self.solid_hex.clear();
                    return Self::apply_solid(rgb);
                }
                None => self.solid_invalid = true,
            },
            Msg::SolidReady(rgb, Ok(path)) => {
                self.error = None;
                config.set_wallpaper(Some(&path));
                config.set_str("general", "background", &wallpaper::hex(rgb));
            }
            Msg::SolidReady(_, Err(err)) => {
                self.error = Some((tr::Key::SolidFailed, err));
            }
            Msg::Backdrop(rgb) => config.set_str("general", "background", &wallpaper::hex(rgb)),
            Msg::OpenFolder => {
                return Task::perform(blocking(wallpaper::open_user_folder), |result| {
                    Msg::FolderOpened(result.unwrap_or(Ok(())))
                });
            }
            Msg::FolderOpened(result) => {
                self.error = result.err().map(|err| (tr::Key::FolderFailed, err));
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let current = config.wallpaper();
        let current = current.as_deref();
        let error = self.error.as_ref().map(|(key, detail)| {
            let message = format!("{} {detail}", c.t(*key));
            widgets::banner(c.theme, crate::theme::Tone::Danger, Icon::Warning, message, None)
        });
        column![
            self.hero(&c, config, current),
            error,
            self.fit_group(&c, config, current),
            self.gallery_group(&c, current),
            self.colours_group(&c, current),
            self.custom_group(&c),
        ]
        .spacing(theme::SECTION_GAP)
        .into()
    }
}

enum Look<'a> {
    Picture(&'a Thumbnail),
    Solid([u8; 3]),
    Pending,
}

impl Page {
    fn load(&mut self, config: &ConfigStore) -> Task<Msg> {
        self.loading = true;
        let current = config.wallpaper();
        Task::perform(blocking(move || wallpaper::gallery(current.as_deref())), |gallery| {
            Msg::Wallpapers(gallery.unwrap_or_default())
        })
    }

    fn apply_solid(rgb: [u8; 3]) -> Task<Msg> {
        Task::perform(blocking(move || wallpaper::write_solid(rgb)), move |path| {
            Msg::SolidReady(rgb, path.unwrap_or_else(|| Err(String::from("cancelled"))))
        })
    }

    fn has(&self, path: &Path) -> bool {
        self.gallery.items.iter().any(|thumb| thumb.path == path)
    }

    fn look<'a>(&'a self, current: Option<&Path>) -> Look<'a> {
        match current {
            None => self.gallery.builtin.as_ref().map_or(Look::Pending, Look::Picture),
            Some(path) => {
                if let Some(rgb) = wallpaper::solid_color(path) {
                    return Look::Solid(rgb);
                }
                self.gallery.items.iter().find(|thumb| thumb.path == path).map_or(Look::Pending, Look::Picture)
            }
        }
    }

    fn hero<'a>(&'a self, c: &Ctx, config: &'a ConfigStore, current: Option<&Path>) -> Element<'a, Msg> {
        let t = c.theme;
        let fit = config.wallpaper_fit();
        let look = self.look(current);
        let (name, detail) = match (&look, current) {
            (Look::Solid(rgb), _) => (c.t(tr::Key::SolidName).to_string(), wallpaper::hex(*rgb).to_uppercase()),
            (_, None) => (c.t(tr::Key::BuiltinName).to_string(), c.t(tr::Key::BuiltinHint).to_string()),
            (look, Some(path)) => (
                path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
                match look {
                    Look::Picture(thumb) => format!(
                        "{} × {} {} · {}",
                        thumb.width,
                        thumb.height,
                        c.t(tr::Key::Pixels),
                        path.parent().map_or_else(String::new, |dir| dir.to_string_lossy().into_owned())
                    ),
                    _ => path.to_string_lossy().into_owned(),
                },
            ),
        };
        let screen = render(&t, &look, fit, backdrop(config), HERO_W, HERO_H);
        let preview = iced::widget::stack![
            screen,
            column![sketch::topbar(t), space().height(Fill)],
            container(sketch::mini_dock(t))
                .width(Fill)
                .height(Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::End)
                .padding(iced::Padding::ZERO.bottom(6.0)),
        ]
        .width(HERO_W)
        .height(HERO_H);
        let framed = sketch::outlined_surface(iced::Color::TRANSPARENT, t.line_2(), 1.0, theme::R2, preview)
            .width(HERO_W)
            .height(HERO_H)
            .clip(true);
        let status: Element<'a, Msg> = if self.loading {
            widgets::spinner_label(t, c.t(K::Loading))
        } else {
            widgets::icon_button(t, Icon::Refresh, Some(Msg::Reload))
        };
        Group::new(t)
            .title(c.t(tr::Key::Current))
            .suffix(status)
            .push(container(framed).center_x(Fill).padding([16, 12]))
            .push(Row::new(t, name).subtitle(detail).prefix(widgets::tile(Icon::Image, t.acc_bg(), 32.0)).build())
            .build()
    }

    fn fit_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore, current: Option<&Path>) -> Element<'a, Msg> {
        let t = c.theme;
        let fit = config.wallpaper_fit();
        let look = self.look(current);
        let colour = backdrop(config);
        let cards = IcedRow::with_children(WallpaperFit::ALL.iter().map(|&option| {
            let (label, hint) = fit_keys(option);
            column![
                widgets::preview_card(
                    t,
                    render(&t, &look, option, colour, FIT_W, FIT_H),
                    c.t(label),
                    option == fit,
                    Some(Msg::SetFit(option)),
                ),
                text(c.t(hint), TextStyle::CAP).color(t.fg_3()),
            ]
            .spacing(2)
            .align_x(Alignment::Center)
            .width(Length::FillPortion(1))
            .into()
        }))
        .spacing(12);
        let mut group = Group::new(t)
            .title(c.t(K::WallpaperFit))
            .description(c.t(tr::Key::FitHint))
            .push(container(gated(*c, cards)).padding([14, 8]));
        if matches!(fit, WallpaperFit::Contain | WallpaperFit::Center) {
            let swatches = swatches(&t, 2.0, 18.0, |rgb| rgb == colour, Msg::Backdrop);
            group = group
                .push(Row::new(t, c.t(tr::Key::Backdrop)).subtitle(c.t(tr::Key::BackdropHint)).build())
                .push(container(gated(*c, swatches)).padding([4.0, theme::ROW_PADDING_X]));
        }
        group.build()
    }

    fn gallery_group<'a>(&'a self, c: &Ctx, current: Option<&Path>) -> Element<'a, Msg> {
        let t = c.theme;
        let pick = |thumb: &Thumbnail| {
            tile(&t, current == Some(thumb.path.as_path()), Msg::Pick(thumb.path.clone()), picture(thumb, None, &t))
        };
        let builtin = self.gallery.builtin.as_ref().map(|builtin| {
            tile(&t, current.is_none(), Msg::UseBuiltin, picture(builtin, Some(c.t(tr::Key::Builtin)), &t))
        });
        let own_pictures = self.gallery.from(Source::Yours);
        let empty = (self.loaded && own_pictures.is_empty())
            .then(|| widgets::list_empty(t, Icon::Image, c.t(tr::Key::NoYours), c.t(tr::Key::NoYoursHint)));
        let yours = builtin.into_iter().chain(own_pictures.into_iter().map(pick)).collect();
        let own = Group::new(t)
            .title(c.t(tr::Key::Yours))
            .description(c.t(tr::Key::YoursHint))
            .suffix(widgets::secondary(t, c.t(tr::Key::OpenFolder), Some(Msg::OpenFolder)))
            .push(gated(*c, grid(yours)))
            .extend(empty);
        let system: Vec<Element<'a, Msg>> = self.gallery.from(Source::System).into_iter().map(pick).collect();
        let system = (!system.is_empty()).then(|| {
            Group::new(t)
                .title(c.t(tr::Key::System))
                .description(c.t(tr::Key::SystemHint))
                .push(gated(*c, grid(system)))
                .build()
        });
        column![own.build(), system].spacing(theme::SECTION_GAP).into()
    }

    fn colours_group<'a>(&'a self, c: &Ctx, current: Option<&Path>) -> Element<'a, Msg> {
        let t = c.theme;
        let selected = current.and_then(wallpaper::solid_color);
        let swatches = swatches(&t, 4.0, 28.0, |rgb| selected == Some(rgb), Msg::Solid);
        let field =
            widgets::field(t, "#1d2433", &self.solid_hex, Msg::SolidHex, Some(Msg::ApplySolidHex), self.solid_invalid);
        Group::new(t)
            .title(c.t(tr::Key::Colours))
            .description(c.t(tr::Key::ColoursHint))
            .push(container(gated(*c, swatches)).padding([10.0, theme::ROW_PADDING_X]))
            .push(
                Row::new(t, c.t(tr::Key::CustomColour))
                    .subtitle(if self.solid_invalid {
                        c.t(tr::Key::ColourInvalid)
                    } else {
                        c.t(tr::Key::CustomColourHint)
                    })
                    .suffix(apply_field(c, field, Msg::ApplySolidHex, 240.0))
                    .build(),
            )
            .build()
    }

    fn custom_group<'a>(&'a self, c: &Ctx) -> Element<'a, Msg> {
        let t = c.theme;
        let custom = widgets::field(
            t,
            c.t(K::WallpaperPathPlaceholder),
            &self.custom_path,
            Msg::CustomPath,
            Some(Msg::ApplyCustom),
            self.custom_invalid,
        );
        Group::new(t)
            .push(
                Row::new(t, c.t(K::CustomImage))
                    .subtitle(if self.custom_invalid { c.t(K::ImageNotFound) } else { c.t(K::CustomImageHint) })
                    .suffix(apply_field(c, custom, Msg::ApplyCustom, 340.0))
                    .build(),
            )
            .build()
    }
}

fn apply_field<'a>(c: &Ctx, field: iced::widget::TextInput<'a, Msg>, apply: Msg, width: f32) -> Element<'a, Msg> {
    let t = c.theme;
    let line = row![field.width(Fill), widgets::secondary(t, c.t(K::Apply), Some(apply))]
        .spacing(8)
        .align_y(Alignment::Center);
    gated(*c, widgets::flexible(line, width))
}

fn swatches<'a>(
    t: &ThemeState, spacing: f32, size: f32, selected: impl Fn([u8; 3]) -> bool, on_press: fn([u8; 3]) -> Msg,
) -> iced::widget::row::Wrapping<'a, Msg> {
    IcedRow::with_children(
        palette()
            .into_iter()
            .map(|rgb| widgets::colour_swatch(*t, to_color(rgb), selected(rgb), Some(on_press(rgb)), size)),
    )
    .spacing(spacing)
    .align_y(Alignment::Center)
    .wrap()
}

pub(crate) const HERO_W: f32 = 400.0;
pub(crate) const HERO_H: f32 = 225.0;
pub(crate) const FIT_W: f32 = 96.0;
pub(crate) const FIT_H: f32 = 54.0;
pub(crate) const REFERENCE_WIDTH: f32 = 1920.0;

const fn fit_keys(fit: WallpaperFit) -> (K, tr::Key) {
    match fit {
        WallpaperFit::Cover => (K::FitCover, tr::Key::FitCoverHint),
        WallpaperFit::Contain => (K::FitContain, tr::Key::FitContainHint),
        WallpaperFit::Center => (K::FitCenter, tr::Key::FitCenterHint),
        WallpaperFit::Tile => (K::FitTile, tr::Key::FitTileHint),
    }
}

fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

fn render<'a>(
    t: &ThemeState, look: &Look<'_>, fit: WallpaperFit, backdrop: [u8; 3], width: f32, height: f32,
) -> Element<'a, Msg> {
    let wallpaper = match look {
        Look::Solid(rgb) => sketch::Wallpaper::Solid(to_color(*rgb)),
        Look::Pending => sketch::Wallpaper::Pending,
        Look::Picture(thumb) => sketch::Wallpaper::Picture {
            handle: thumb.handle.clone(),
            width: thumb.width as f32,
            height: thumb.height as f32,
        },
    };
    let fit = match fit {
        WallpaperFit::Cover => sketch::Fit::Cover,
        WallpaperFit::Contain => sketch::Fit::Contain,
        WallpaperFit::Center => sketch::Fit::Center,
        WallpaperFit::Tile => sketch::Fit::Tile,
    };
    sketch::wallpaper(*t, &wallpaper, fit, to_color(backdrop), width, height, REFERENCE_WIDTH)
}

fn picture<'a>(thumb: &Thumbnail, badge: Option<&'a str>, t: &ThemeState) -> Element<'a, Msg> {
    let shot = glide_sdk::widget::picture(thumb.handle.clone(), Fill, TILE_H, ContentFit::Cover, theme::R2);
    match badge {
        Some(label) => iced::widget::stack![
            shot,
            container(glide_sdk::widget::bubble(&widgets::paint(t), text(label, TextStyle::CAP)))
                .padding(6)
                .align_bottom(Fill)
                .align_left(Fill),
        ]
        .into(),
        None => shot,
    }
}

pub const TILE_H: f32 = 84.0;

fn tile<'a>(t: &ThemeState, selected: bool, message: Msg, content: Element<'a, Msg>) -> Element<'a, Msg> {
    container(glide_sdk::widget::frame_button(&widgets::paint(t), content, selected, message))
        .width(Length::FillPortion(1))
        .into()
}

fn grid(tiles: Vec<Element<'_, Msg>>) -> Element<'_, Msg> {
    container(widgets::grid(tiles, 4, 10.0)).padding([theme::ROW_PADDING_X, theme::ROW_PADDING_X]).into()
}
