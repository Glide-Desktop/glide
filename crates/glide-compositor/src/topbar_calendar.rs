use super::{
    Anim, CLOCK_STYLE, Cache, E_OUT, LocalRect, Logical, MemoryRenderBuffer, MemoryRenderBufferRenderElement, NC_PAD,
    OpenPopup, Point, Rectangle, Scale, Size, Slot, Topbar, TopbarTheme, local_rect, motion, numeric_label_width,
    push_popup_buffer, quantize, widest_digits,
};
use crate::i18n::Lang;
use crate::paint::TextAlign;
use crate::raster::Canvas;
use glide_theme::spacing::{SP1, SP2, SP3};

const HEADER_H: i32 = 28;
const NAV_BUTTON: i32 = 28;
const TITLE_W: i32 = 184;
const WEEK_H: i32 = 16;
const CELL_H: i32 = 28;
const ROWS: i32 = 6;
const TODAY_DOT: i32 = 24;
const NAV_GLYPH: i32 = 14;
const SLIDE_QUANTUM: f64 = 24.0;
pub(super) const BLOCK_H: i32 = HEADER_H + SP2 + WEEK_H + SP1 + ROWS * CELL_H + SP3 + 1 + SP3;

type CalKey = (YearMonth, Option<YearMonth>, i32, u8, Option<(i32, u32, u32)>, FirstWeekday, Lang, i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MetricsKey {
    lang: Lang,
    h24: bool,
    date: bool,
}

#[derive(Debug)]
pub(super) struct CalendarView {
    today: Option<LocalNow>,
    first: FirstWeekday,
    lang: Lang,
    shown: Option<YearMonth>,
    from: Option<YearMonth>,
    dir: i32,
    slide: Anim,
    cache: Cache<CalKey>,
    metrics: Option<(MetricsKey, (i32, i32))>,
}

impl Default for CalendarView {
    fn default() -> Self {
        Self {
            today: None,
            first: FirstWeekday::Monday,
            lang: Lang::default(),
            shown: None,
            from: None,
            dir: 0,
            slide: Anim::at(1.0),
            cache: Cache::default(),
            metrics: None,
        }
    }
}

impl CalendarView {
    pub(super) fn clear(&mut self) {
        self.cache.clear();
        self.metrics = None;
    }

    pub(super) fn is_animating(&self) -> bool {
        self.slide.is_animating()
    }

    pub(super) fn month(&self) -> YearMonth {
        self.shown.or_else(|| self.today.map(LocalNow::month)).unwrap_or(YearMonth { year: 1970, month0: 0 })
    }

    pub(super) fn reset(&mut self) {
        self.shown = None;
        self.from = None;
        self.slide.jump(1.0);
    }

    fn shift(&mut self, delta: i32) {
        if delta == 0 {
            return;
        }
        let current = self.month();
        self.from = Some(current);
        self.shown = Some(current.shifted(delta));
        self.dir = delta.signum();
        self.slide.jump(0.0);
        self.slide.retarget(1.0, motion::t2(), E_OUT);
    }
}

impl Topbar {
    pub fn set_now(&mut self, now: LocalNow) -> bool {
        let lang = crate::i18n::language();
        let us = env_locale_is_us();
        let h24 = self.theme.clock_24h.unwrap_or_else(|| default_24h(us));
        let date =
            if self.theme.clock_date { format_date(lang, now.weekday, now.day, now.month0) } else { String::new() };
        let clock = format_time(now.hour, now.minute, h24);
        let mut dirty = self.set_clock(date, clock);
        let first = first_weekday(lang, us);
        let view = &mut self.calendar;
        if view.today != Some(now) || view.first != first || view.lang != lang {
            let day_changed =
                view.today.is_none_or(|today| (today.year, today.month0, today.day) != (now.year, now.month0, now.day));
            view.today = Some(now);
            view.first = first;
            view.lang = lang;
            dirty |= day_changed;
        }
        if dirty {
            self.touch();
        }
        dirty
    }

    pub fn shift_calendar(&mut self, delta: i32) -> bool {
        if delta == 0 {
            return false;
        }
        self.calendar.shift(delta);
        self.touch();
        true
    }

    pub fn calendar_today(&mut self) -> bool {
        let Some(today) = self.calendar.today.map(LocalNow::month) else {
            return false;
        };
        let current = self.calendar.month();
        if current == today {
            return false;
        }
        let delta = (today.year - current.year) * 12 + i32::try_from(today.month0).unwrap_or(0)
            - i32::try_from(current.month0).unwrap_or(0);
        self.shift_calendar(delta)
    }

    pub(super) fn clock_metrics(&mut self, painter: &crate::paint::Painter) -> (i32, i32) {
        let lang = self.calendar.lang;
        let h24 = self.theme.clock_24h.unwrap_or_else(|| default_24h(env_locale_is_us()));
        let key = MetricsKey { lang, h24, date: self.theme.clock_date && !self.date.is_empty() };
        if let Some((cached, widths)) = self.calendar.metrics
            && cached == key
        {
            return widths;
        }
        let date_w = if key.date { widest_date_width(painter, lang) } else { 0 };
        let time_w =
            widest_times(h24).iter().map(|time| numeric_label_width(painter, time, &CLOCK_STYLE)).max().unwrap_or(0);
        let widths = (date_w, time_w);
        self.calendar.metrics = Some((key, widths));
        widths
    }
}

fn widest_date_width(painter: &crate::paint::Painter, lang: Lang) -> i32 {
    let style = &glide_theme::TextStyle::TOPBAR;
    let widest = (0..7)
        .flat_map(|weekday| (0..12).map(move |month| (weekday, month)))
        .map(|(weekday, month)| {
            let text = widest_digits(&format_date(lang, weekday, 28, month));
            painter.text_width_styled(&text, style)
        })
        .fold(0.0_f32, f32::max);
    widest.ceil() as i32
}

pub(super) fn layout_slots(popup: Point<i32, Logical>, width: i32, slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>) {
    let top = popup.y + NC_PAD;
    let next_x = popup.x + width - NC_PAD - NAV_BUTTON;
    slots.push((Slot::CalMonth, local_rect(popup.x + NC_PAD, top, TITLE_W, HEADER_H)));
    slots.push((Slot::CalPrev, local_rect(next_x - SP1 - NAV_BUTTON, top, NAV_BUTTON, HEADER_H)));
    slots.push((Slot::CalNext, local_rect(next_x, top, NAV_BUTTON, HEADER_H)));
}

#[allow(clippy::too_many_arguments)]
pub(super) fn push<R>(
    topbar: &mut Topbar, renderer: &mut R, painter: &crate::paint::Painter, icons: &mut crate::icon::IconLoader,
    popup: Rectangle<i32, Logical>, reveal: f64, scale: Scale<f64>, output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: crate::raster::ShellRenderer,
{
    if topbar.shown_popup() != Some(OpenPopup::NotificationCenter) {
        return;
    }
    let width = popup.size.w;
    let theme = &topbar.theme;
    let view = &mut topbar.calendar;
    let slide = view.slide.value().clamp(0.0, 1.0);
    let settled = slide >= 1.0;
    let from = view.from.filter(|_| !settled);
    let key: CalKey = (
        view.month(),
        from,
        view.dir,
        quantize(slide, SLIDE_QUANTUM),
        view.today.map(|now| (now.year, now.month0, now.day)),
        view.first,
        view.lang,
        width,
    );
    let frame = Frame {
        month: view.month(),
        from,
        dir: view.dir,
        slide,
        today: view.today,
        first: view.first,
        lang: view.lang,
    };
    let Some(buffer) =
        view.cache.resolve(key, output_scale, || draw(painter, theme, &frame, width, output_scale, Some(icons)))
    else {
        return;
    };
    let element = Rectangle::new(Point::from((popup.loc.x, popup.loc.y + NC_PAD)), Size::from((width, BLOCK_H)));
    push_popup_buffer(renderer, buffer, popup, element, reveal, scale, elements);
}

struct Frame {
    month: YearMonth,
    from: Option<YearMonth>,
    dir: i32,
    slide: f64,
    today: Option<LocalNow>,
    first: FirstWeekday,
    lang: Lang,
}

fn draw_header(c: &mut Canvas<'_>, painter: &crate::paint::Painter, theme: &TopbarTheme, frame: &Frame) {
    use glide_theme::TextStyle;
    let width = c.w;
    let title = month_title(frame.lang, frame.month);
    c.text(
        painter,
        (NC_PAD + SP2, NC_PAD + TITLE_W, 0, HEADER_H),
        TextAlign::Left,
        &title,
        theme.label,
        &TextStyle::HEAD,
    );

    let next_x = width - NC_PAD - NAV_BUTTON;
    for (x, icon, fallback) in [(next_x - SP1 - NAV_BUTTON, "go-previous", "\u{2039}"), (next_x, "go-next", "\u{203a}")]
    {
        let at = (x + (NAV_BUTTON - NAV_GLYPH) / 2, (HEADER_H - NAV_GLYPH) / 2);
        if !c.icon(icon, NAV_GLYPH, at, theme.muted) {
            c.text(
                painter,
                (x, x + NAV_BUTTON, 0, HEADER_H),
                TextAlign::Center,
                fallback,
                theme.muted,
                &TextStyle::HEAD,
            );
        }
    }

    let grid_w = width - NC_PAD * 2;
    let cell_w = grid_w / 7;
    let inset = (grid_w - cell_w * 7) / 2;
    let week_y = HEADER_H + SP2;
    for (column, name) in header_weekdays(frame.lang, frame.first).iter().enumerate() {
        let x = NC_PAD + inset + i32::try_from(column).unwrap_or(0) * cell_w;
        let band = (x, x + cell_w, week_y, week_y + WEEK_H);
        c.text(painter, band, TextAlign::Center, name, theme.faint, &TextStyle::MICRO);
    }
}

fn draw(
    painter: &crate::paint::Painter, theme: &TopbarTheme, frame: &Frame, width: i32, scale: i32,
    icons: Option<&mut crate::icon::IconLoader>,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(width, BLOCK_H, scale)?.with_icons(icons);
    draw_header(&mut c, painter, theme, frame);

    let grid_w = width - NC_PAD * 2;
    let cell_w = grid_w / 7;
    let inset = (grid_w - cell_w * 7) / 2;
    let mut grid = Canvas::new(grid_w, ROWS * CELL_H, scale)?;
    let travel = f64::from(grid_w);
    let grid_ctx = GridPaint { painter, theme, cell_w, inset, today: frame.today, first: frame.first };
    let incoming = (f64::from(frame.dir) * (1.0 - frame.slide) * travel).round() as i32;
    if let Some(from) = frame.from {
        let outgoing = incoming - (f64::from(frame.dir) * travel).round() as i32;
        draw_month(&mut grid, &grid_ctx, from, outgoing);
    }
    draw_month(&mut grid, &grid_ctx, frame.month, incoming);
    let grid_y = HEADER_H + SP2 + WEEK_H + SP1;
    c.blit(&grid.buf, local_rect(NC_PAD, grid_y, grid_w, grid.h));
    c.fill(local_rect(NC_PAD, BLOCK_H - SP3 - 1, grid_w, 1), theme.border, 0.0);
    Some(c.finish())
}

struct GridPaint<'a> {
    painter: &'a crate::paint::Painter,
    theme: &'a TopbarTheme,
    cell_w: i32,
    inset: i32,
    today: Option<LocalNow>,
    first: FirstWeekday,
}

fn draw_month(c: &mut Canvas<'_>, ctx: &GridPaint<'_>, month: YearMonth, offset: i32) {
    for (index, cell) in month_grid(month, ctx.first).iter().enumerate() {
        let index = i32::try_from(index).unwrap_or(0);
        let x = offset + ctx.inset + (index % 7) * ctx.cell_w;
        let y = (index / 7) * CELL_H;
        if x + ctx.cell_w <= 0 || x >= c.w {
            continue;
        }
        let rect: LocalRect = local_rect(x, y, ctx.cell_w, CELL_H);
        let is_today = cell.in_month && ctx.today.is_some_and(|now| now.month() == month && now.day == cell.day);
        if is_today {
            let dot = local_rect(x + (ctx.cell_w - TODAY_DOT) / 2, y + (CELL_H - TODAY_DOT) / 2, TODAY_DOT, TODAY_DOT);
            c.fill(dot, ctx.theme.active_background, glide_theme::shape::R_ROUND);
        }
        let colour = if is_today {
            ctx.theme.active_label
        } else if cell.in_month {
            ctx.theme.label
        } else {
            ctx.theme.faint
        };
        c.text_in(ctx.painter, rect, TextAlign::Center, &cell.day.to_string(), colour, &CLOCK_STYLE);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FirstWeekday {
    Monday,
    Sunday,
}

impl FirstWeekday {
    #[must_use]
    pub const fn offset(self) -> u32 {
        match self {
            Self::Monday => 1,
            Self::Sunday => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct YearMonth {
    pub year: i32,
    pub month0: u32,
}

impl YearMonth {
    #[must_use]
    pub fn new(year: i32, month0: u32) -> Self {
        Self { year, month0: 0 }.shifted(i32::try_from(month0).unwrap_or(0))
    }

    #[must_use]
    pub fn shifted(self, delta: i32) -> Self {
        let index = i64::from(self.year) * 12 + i64::from(self.month0) + i64::from(delta);
        let year = i32::try_from(index.div_euclid(12)).unwrap_or(self.year);
        let month0 = u32::try_from(index.rem_euclid(12)).unwrap_or(0);
        Self { year, month0 }
    }
}

#[must_use]
pub const fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

#[must_use]
pub const fn days_in_month(year: i32, month0: u32) -> u32 {
    match month0 % 12 {
        1 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        3 | 5 | 8 | 10 => 30,
        _ => 31,
    }
}

#[must_use]
pub fn weekday(year: i32, month0: u32, day: u32) -> u32 {
    const T: [i64; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let month0 = month0 % 12;
    let y = i64::from(year) - i64::from(month0 < 2);
    let index = usize::try_from(month0).unwrap_or(0);
    let sum = y + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400) + T[index] + i64::from(day);
    u32::try_from(sum.rem_euclid(7)).unwrap_or(0)
}

#[must_use]
pub fn leading_days(month: YearMonth, first: FirstWeekday) -> u32 {
    (weekday(month.year, month.month0, 1) + 7 - first.offset()) % 7
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub day: u32,
    pub in_month: bool,
}

pub const GRID_ROWS: usize = 6;
pub const GRID_CELLS: usize = GRID_ROWS * 7;

#[must_use]
pub fn month_grid(month: YearMonth, first: FirstWeekday) -> [Cell; GRID_CELLS] {
    let lead = leading_days(month, first);
    let previous = month.shifted(-1);
    let previous_len = days_in_month(previous.year, previous.month0);
    let length = days_in_month(month.year, month.month0);
    let mut cells = [Cell { day: 0, in_month: false }; GRID_CELLS];
    for (index, cell) in cells.iter_mut().enumerate() {
        let index = u32::try_from(index).unwrap_or(0);
        *cell = if index < lead {
            Cell { day: previous_len - lead + index + 1, in_month: false }
        } else if index - lead < length {
            Cell { day: index - lead + 1, in_month: true }
        } else {
            Cell { day: index - lead - length + 1, in_month: false }
        };
    }
    cells
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalNow {
    pub year: i32,
    pub month0: u32,
    pub day: u32,
    pub weekday: u32,
    pub hour: u32,
    pub minute: u32,
}

pub fn refresh_timezone(now: libc::time_t) {
    unsafe extern "C" {
        fn tzset();
    }
    static LAST_MINUTE: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(i64::MIN);
    let minute = now.div_euclid(60);
    if LAST_MINUTE.swap(minute, std::sync::atomic::Ordering::Relaxed) != minute {
        // SAFETY: `tzset` takes glibc's timezone lock, which `localtime_r`
        // also takes, and only re-reads `TZ` / `/etc/localtime`.
        unsafe { tzset() };
    }
}

impl LocalNow {
    #[must_use]
    pub fn now() -> Option<Self> {
        let mut raw: libc::time_t = 0;
        // SAFETY: `time` writes a single `time_t` through a pointer we own.
        let now = unsafe { libc::time(&raw mut raw) };
        if now == -1 {
            return None;
        }
        refresh_timezone(now);
        // SAFETY: `tm` is a plain C struct of integers and a pointer for which
        // all-zero bytes is a valid value; `localtime_r` overwrites it below.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        // SAFETY: both pointers address locals that outlive the call, and
        // `localtime_r` writes only the `tm` we hand it.
        let filled = unsafe { libc::localtime_r(&raw const now, &raw mut tm) };
        if filled.is_null() {
            return None;
        }
        Some(Self {
            year: tm.tm_year + 1900,
            month0: u32::try_from(tm.tm_mon).ok()?.min(11),
            day: u32::try_from(tm.tm_mday).ok()?,
            weekday: u32::try_from(tm.tm_wday).ok()? % 7,
            hour: u32::try_from(tm.tm_hour).ok()?,
            minute: u32::try_from(tm.tm_min).ok()?,
        })
    }

    #[must_use]
    pub const fn month(self) -> YearMonth {
        YearMonth { year: self.year, month0: self.month0 }
    }
}

#[must_use]
pub fn format_date(lang: Lang, weekday: u32, day: u32, month0: u32) -> String {
    let name = crate::i18n::weekday_short(lang, weekday);
    let month = crate::i18n::month_short(lang, month0);
    match lang {
        Lang::Uk => format!("{name} {day} {month}."),
        Lang::En => format!("{name} {day} {month}"),
    }
}

#[must_use]
pub fn format_time(hour: u32, minute: u32, h24: bool) -> String {
    if h24 {
        return format!("{hour:02}:{minute:02}");
    }
    let suffix = if hour < 12 { "AM" } else { "PM" };
    let twelve = match hour % 12 {
        0 => 12,
        other => other,
    };
    format!("{twelve}:{minute:02} {suffix}")
}

#[must_use]
pub fn widest_times(h24: bool) -> Vec<String> {
    if h24 { vec![format_time(20, 58, true)] } else { vec![format_time(10, 58, false), format_time(22, 58, false)] }
}

#[must_use]
pub fn locale_is_us(lc_all: Option<&str>, lc_time: Option<&str>, lang: Option<&str>) -> bool {
    let locale = [lc_all, lc_time, lang]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .unwrap_or_default();
    let base = locale.split(['.', '@']).next().unwrap_or_default();
    base.eq_ignore_ascii_case("en_US")
}

#[must_use]
pub fn env_locale_is_us() -> bool {
    let var = |name: &str| std::env::var(name).ok();
    locale_is_us(var("LC_ALL").as_deref(), var("LC_TIME").as_deref(), var("LANG").as_deref())
}

#[must_use]
pub const fn default_24h(us_locale: bool) -> bool {
    !us_locale
}

#[must_use]
pub const fn first_weekday(lang: Lang, us_locale: bool) -> FirstWeekday {
    match lang {
        Lang::En if us_locale => FirstWeekday::Sunday,
        _ => FirstWeekday::Monday,
    }
}

#[must_use]
pub fn header_weekdays(lang: Lang, first: FirstWeekday) -> [&'static str; 7] {
    let mut out = [""; 7];
    for (column, slot) in out.iter_mut().enumerate() {
        let column = u32::try_from(column).unwrap_or(0);
        *slot = crate::i18n::weekday_short(lang, (column + first.offset()) % 7);
    }
    out
}

#[must_use]
pub fn month_title(lang: Lang, month: YearMonth) -> String {
    format!("{} {}", crate::i18n::month_name(lang, month.month0), month.year)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drawn_grid_has_a_row_for_every_row_of_the_model() {
        assert_eq!(usize::try_from(ROWS).unwrap(), GRID_ROWS);
    }

    #[test]
    fn leap_years_follow_the_gregorian_rule() {
        assert!(is_leap(2024));
        assert!(!is_leap(2026));
        assert!(!is_leap(1900));
        assert!(is_leap(2000));
    }

    #[test]
    fn month_lengths_cover_february_in_leap_and_common_years() {
        assert_eq!(days_in_month(2026, 0), 31);
        assert_eq!(days_in_month(2026, 1), 28);
        assert_eq!(days_in_month(2024, 1), 29);
        assert_eq!(days_in_month(2100, 1), 28);
        assert_eq!(days_in_month(2026, 3), 30);
        assert_eq!(days_in_month(2026, 8), 30);
        assert_eq!(days_in_month(2026, 11), 31);
        let year: u32 = (0..12).map(|m| days_in_month(2026, m)).sum();
        assert_eq!(year, 365);
        let leap: u32 = (0..12).map(|m| days_in_month(2024, m)).sum();
        assert_eq!(leap, 366);
    }

    #[test]
    fn weekdays_match_known_dates() {
        assert_eq!(weekday(2026, 8, 24), 4);
        assert_eq!(weekday(2000, 0, 1), 6);
        assert_eq!(weekday(2024, 1, 29), 4);
        assert_eq!(weekday(1970, 0, 1), 4);
    }

    #[test]
    fn september_2026_starts_on_a_tuesday() {
        let september = YearMonth::new(2026, 8);
        assert_eq!(leading_days(september, FirstWeekday::Monday), 1);
        assert_eq!(leading_days(september, FirstWeekday::Sunday), 2);
    }

    #[test]
    fn a_month_starting_on_the_first_weekday_has_no_leading_days() {
        let june = YearMonth::new(2026, 5);
        assert_eq!(weekday(2026, 5, 1), 1);
        assert_eq!(leading_days(june, FirstWeekday::Monday), 0);
        assert_eq!(leading_days(june, FirstWeekday::Sunday), 1);
    }

    #[test]
    fn the_grid_places_every_day_once_after_the_leading_days() {
        let september = YearMonth::new(2026, 8);
        let grid = month_grid(september, FirstWeekday::Monday);
        assert_eq!(grid[0], Cell { day: 31, in_month: false });
        assert_eq!(grid[1], Cell { day: 1, in_month: true });
        let days: Vec<u32> = grid.iter().filter(|c| c.in_month).map(|c| c.day).collect();
        assert_eq!(days, (1..=30).collect::<Vec<_>>());
        assert_eq!(grid[31], Cell { day: 1, in_month: false });
        assert_eq!(grid[GRID_CELLS - 1].day, 11);
    }

    #[test]
    fn a_leap_february_fills_twenty_nine_cells() {
        let grid = month_grid(YearMonth::new(2024, 1), FirstWeekday::Sunday);
        assert_eq!(grid.iter().filter(|c| c.in_month).count(), 29);
        assert_eq!(leading_days(YearMonth::new(2024, 1), FirstWeekday::Sunday), 4);
        assert_eq!(grid[4].day, 1);
        assert_eq!(grid[3].day, 31);
    }

    #[test]
    fn shifting_a_month_wraps_across_years() {
        let january = YearMonth::new(2026, 0);
        assert_eq!(january.shifted(-1), YearMonth::new(2025, 11));
        assert_eq!(YearMonth::new(2026, 11).shifted(1), YearMonth::new(2027, 0));
        assert_eq!(january.shifted(25), YearMonth::new(2028, 1));
        assert_eq!(YearMonth::new(2026, 13), YearMonth::new(2027, 1));
    }

    #[test]
    fn dates_read_naturally_in_each_language() {
        assert_eq!(format_date(Lang::Uk, 4, 24, 8), "чт 24 вер.");
        assert_eq!(format_date(Lang::En, 4, 24, 8), "Thu 24 Sep");
        assert_eq!(format_date(Lang::En, 0, 1, 0), "Sun 1 Jan");
    }

    #[test]
    fn times_use_the_requested_clock() {
        assert_eq!(format_time(14, 5, true), "14:05");
        assert_eq!(format_time(9, 41, true), "09:41");
        assert_eq!(format_time(14, 5, false), "2:05 PM");
        assert_eq!(format_time(0, 0, false), "12:00 AM");
        assert_eq!(format_time(12, 30, false), "12:30 PM");
    }

    #[test]
    fn the_widest_times_have_as_many_characters_as_any_time() {
        for h24 in [true, false] {
            let widest = widest_times(h24);
            for hour in 0..24 {
                let time = format_time(hour, 58, h24);
                let suffix = &time[time.len() - 2..];
                assert!(
                    widest
                        .iter()
                        .any(|candidate| candidate.ends_with(suffix)
                            && candidate.chars().count() >= time.chars().count()),
                    "{time}"
                );
            }
        }
    }

    #[test]
    fn only_a_us_locale_defaults_to_twelve_hours() {
        assert!(locale_is_us(None, None, Some("en_US.UTF-8")));
        assert!(!locale_is_us(None, None, Some("en_GB.UTF-8")));
        assert!(!locale_is_us(None, None, Some("uk_UA.UTF-8")));
        assert!(locale_is_us(None, Some("en_US"), Some("uk_UA.UTF-8")));
        assert!(!locale_is_us(Some("C"), Some("en_US"), None));
        assert!(!default_24h(true));
        assert!(default_24h(false));
    }

    #[test]
    fn the_week_starts_on_monday_except_in_the_us() {
        assert_eq!(first_weekday(Lang::Uk, true), FirstWeekday::Monday);
        assert_eq!(first_weekday(Lang::Uk, false), FirstWeekday::Monday);
        assert_eq!(first_weekday(Lang::En, true), FirstWeekday::Sunday);
        assert_eq!(first_weekday(Lang::En, false), FirstWeekday::Monday);
        assert_eq!(header_weekdays(Lang::Uk, FirstWeekday::Monday)[0], "пн");
        assert_eq!(header_weekdays(Lang::En, FirstWeekday::Sunday)[0], "Sun");
        assert_eq!(header_weekdays(Lang::En, FirstWeekday::Monday)[6], "Sun");
    }

    #[test]
    fn month_titles_name_the_month_and_year() {
        assert_eq!(month_title(Lang::Uk, YearMonth::new(2026, 8)), "Вересень 2026");
        assert_eq!(month_title(Lang::En, YearMonth::new(2026, 8)), "September 2026");
    }
}
