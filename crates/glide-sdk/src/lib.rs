pub mod anim;
pub mod app;
pub mod look;
pub mod menu;
pub mod overlay;
pub mod system;
pub mod widget;
pub mod window;

pub use app::{App, Harness, Settings, Shell, run};
pub use glide_iced::Paint;
pub use look::{Lang, Look};
pub use menu::{Item, Menu};
pub use overlay::Dialog;
pub use widget::{Icon, Tone};
pub use {glide_app, glide_iced, glide_theme, glide_ui, iced};

pub mod theme {
    pub use glide_iced::style::{R2, R3, SP1, SP2, SP3, SP4, SP5, SP6, over, to_iced};
    pub use glide_theme::elevation::Level;
    pub use glide_theme::shape::{R_ROUND, R1, R4};
    pub use glide_theme::{Accent, Appearance, TextStyle, Tokens, component, motion};
}

pub mod prelude {
    pub use crate::anim::{self, Speed};
    pub use crate::theme::{SP1, SP2, SP3, SP4, SP5, SP6, TextStyle};
    pub use crate::widget::{self, Icon, Severity, Tone, text};
    pub use crate::{App, Dialog, Item, Lang, Menu, Paint, Settings, Shell};
    pub use iced::widget::{Column, Row, Space, container};
    pub use iced::{Alignment, Element, Fill, Length, Padding, Subscription, Task};
}
