mod model;
mod service;
mod watcher;

pub use model::{
    Address, DEFAULT_ITEM_PATH, HOST_PREFIX, ITEM_INTERFACE, ItemProps, Pixmap, Registry, Status, WATCHER_INTERFACE,
    WATCHER_NAME, WATCHER_PATH, argb_to_premultiplied_bgra, fit_square, is_monochrome, parse_props, pick_pixmap,
    pixmap_square,
};
pub use service::{Sink, TrayEvent, TrayHandle, TrayItem, start};
