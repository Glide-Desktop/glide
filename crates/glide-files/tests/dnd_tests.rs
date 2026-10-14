use std::path::PathBuf;

use glide_files::ViewMode;
use glide_files::dnd::{gnome_files, parse_gnome_files, parse_uri_list, uri_list};
use glide_files::widgets::content::{GRID_PADDING, LIST_PADDING, Metrics, TILE_GAP};

#[test]
fn uri_lists_round_trip_awkward_names() {
    let paths = vec![PathBuf::from("/home/me/Фото 2024/кіт.png"), PathBuf::from("/tmp/a b#c%d.txt")];
    let text = uri_list(&paths);
    assert!(text.lines().all(|line| line.starts_with("file:///")));
    assert!(text.ends_with("\r\n"));
    assert_eq!(parse_uri_list(&text), paths);
}

#[test]
fn foreign_uri_lists_skip_comments_and_remote_entries() {
    let text =
        "# dragged from a browser\r\nhttps://example.com/x.png\r\nfile://localhost/tmp/x.png\r\nfile:///tmp/y\r\n";
    assert_eq!(parse_uri_list(text), vec![PathBuf::from("/tmp/x.png"), PathBuf::from("/tmp/y")]);
}

#[test]
fn gnome_copied_files_carry_cut_or_copy() {
    let paths = vec![PathBuf::from("/tmp/one"), PathBuf::from("/tmp/two words")];
    let cut = gnome_files(&paths, true);
    assert!(cut.starts_with("cut\n"));
    assert_eq!(parse_gnome_files(&cut), Some((true, paths.clone())));
    assert_eq!(parse_gnome_files(&gnome_files(&paths, false)), Some((false, paths)));
    assert_eq!(parse_gnome_files("move\nfile:///tmp"), None);
}

#[test]
fn the_grid_cell_under_the_pointer_is_found_and_gaps_are_not_cells() {
    let m = Metrics::default();
    let width = 2.0 * GRID_PADDING + 3.0 * m.tile_w + 2.0 * TILE_GAP + 1.0;
    let first = m.entry_at(ViewMode::Grid, width, 10, GRID_PADDING + 1.0, GRID_PADDING + 1.0);
    assert_eq!(first, Some(0));
    let second_row = m.entry_at(
        ViewMode::Grid,
        width,
        10,
        GRID_PADDING + m.tile_w + TILE_GAP + 5.0,
        GRID_PADDING + m.tile_h + TILE_GAP + 5.0,
    );
    assert_eq!(second_row, Some(4));
    let gap = m.entry_at(ViewMode::Grid, width, 10, GRID_PADDING + m.tile_w + TILE_GAP / 2.0, GRID_PADDING + 5.0);
    assert_eq!(gap, None);
    assert_eq!(m.entry_at(ViewMode::Grid, width, 2, width - 10.0, GRID_PADDING + 5.0), None);
}

#[test]
fn a_rubber_band_selects_every_tile_it_touches() {
    let m = Metrics::default();
    let width = 2.0 * GRID_PADDING + 3.0 * m.tile_w + 2.0 * TILE_GAP + 1.0;
    let corner = (GRID_PADDING + 5.0, GRID_PADDING + 5.0);
    let across = (GRID_PADDING + m.tile_w + TILE_GAP + 5.0, GRID_PADDING + m.tile_h + TILE_GAP + 5.0);
    assert_eq!(m.entries_in(ViewMode::Grid, width, 9, corner, across), [0, 1, 3, 4]);
    assert_eq!(m.entries_in(ViewMode::Grid, width, 9, across, corner), [0, 1, 3, 4]);
    let gap = GRID_PADDING + m.tile_w + TILE_GAP / 2.0;
    assert!(m.entries_in(ViewMode::Grid, width, 9, (gap, 0.0), (gap + 1.0, 5.0)).is_empty());
    let rows =
        m.entries_in(ViewMode::List, 600.0, 10, (10.0, LIST_PADDING + 1.5 * m.row), (20.0, LIST_PADDING + 3.2 * m.row));
    assert_eq!(rows, [1, 2, 3]);
}

#[test]
fn the_list_row_under_the_pointer_is_found() {
    let m = Metrics::default();
    let y = LIST_PADDING + 2.5 * m.row;
    assert_eq!(m.entry_at(ViewMode::List, 800.0, 10, 300.0, y), Some(2));
    assert_eq!(m.entry_at(ViewMode::List, 800.0, 2, 300.0, y), None);
}
