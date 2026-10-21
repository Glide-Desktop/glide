use std::time::Duration;

pub const CONTROL: f32 = 34.0;
pub const SMALL_CONTROL: f32 = 28.0;
pub const LARGE_CONTROL: f32 = 42.0;
pub const WINDOW_HEADER: f32 = 48.0;
pub const MENU_ITEM: f32 = CONTROL;
pub const BORDER: f32 = 1.0;
pub const FOCUS_RING: f32 = 2.0;
pub const FIELD_HALO: f32 = 3.0;
pub const IDLE_CONTROLS: f32 = 0.45;
pub const TOOLTIP_DELAY: Duration = Duration::from_millis(550);
pub const BUTTON_PRESSED_SCALE: f32 = 0.975;
pub const POPOVER_CLOSED_SCALE: f32 = 0.94;
pub const POPOVER_RISE: f32 = 6.0;
pub const DIALOG_CLOSED_SCALE: f32 = 0.96;
pub const WINDOW_CONTROL: i32 = 26;

#[must_use]
pub fn titlebar_button(height: i32) -> i32 {
    i32::midpoint(height, 8).clamp(10, WINDOW_CONTROL)
}
