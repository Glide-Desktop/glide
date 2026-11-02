#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const ZERO: Self = Self { x: 0, y: 0 };

    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Size {
    pub width: i32,
    pub height: i32,
}

impl Size {
    pub const ZERO: Self = Self { width: 0, height: 0 };

    #[must_use]
    pub const fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.width <= 0 || self.height <= 0
    }

    #[must_use]
    pub const fn grow(self, insets: Insets) -> Self {
        Self::new(self.width.saturating_add(insets.width()), self.height.saturating_add(insets.height()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Insets {
    pub top: i32,
    pub left: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Insets {
    pub const NONE: Self = Self { top: 0, left: 0, right: 0, bottom: 0 };

    #[must_use]
    pub const fn width(&self) -> i32 {
        self.left.saturating_add(self.right)
    }

    #[must_use]
    pub const fn height(&self) -> i32 {
        self.top.saturating_add(self.bottom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Rectangle {
    pub loc: Point,
    pub size: Size,
}

impl Rectangle {
    #[must_use]
    pub const fn new(loc: Point, size: Size) -> Self {
        Self { loc, size }
    }

    #[must_use]
    pub const fn from_loc_and_size(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self { loc: Point::new(x, y), size: Size::new(width, height) }
    }

    #[must_use]
    pub const fn contains(&self, p: Point) -> bool {
        p.x >= self.loc.x
            && (p.x as i64) < self.loc.x as i64 + self.size.width as i64
            && p.y >= self.loc.y
            && (p.y as i64) < self.loc.y as i64 + self.size.height as i64
    }

    #[must_use]
    pub const fn center(&self) -> Point {
        Point { x: self.loc.x.saturating_add(self.size.width / 2), y: self.loc.y.saturating_add(self.size.height / 2) }
    }

    #[must_use]
    pub const fn inset(&self, insets: Insets) -> Self {
        Self {
            loc: Point::new(self.loc.x.saturating_add(insets.left), self.loc.y.saturating_add(insets.top)),
            size: Size::new(
                self.size.width.saturating_sub(insets.width()),
                self.size.height.saturating_sub(insets.height()),
            ),
        }
    }

    #[must_use]
    pub const fn outset(&self, insets: Insets) -> Self {
        Self {
            loc: Point::new(self.loc.x.saturating_sub(insets.left), self.loc.y.saturating_sub(insets.top)),
            size: self.size.grow(insets),
        }
    }

    #[must_use]
    pub const fn overlaps(&self, other: Self) -> bool {
        if self.size.is_empty() || other.size.is_empty() {
            return false;
        }

        (self.loc.x as i64) < other.loc.x as i64 + other.size.width as i64
            && (other.loc.x as i64) < self.loc.x as i64 + self.size.width as i64
            && (self.loc.y as i64) < other.loc.y as i64 + other.size.height as i64
            && (other.loc.y as i64) < self.loc.y as i64 + self.size.height as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_preserves_half_open_bounds_at_coordinate_limits() {
        let rect = Rectangle::from_loc_and_size(i32::MAX, i32::MAX, 1, 1);
        assert!(rect.contains(Point::new(i32::MAX, i32::MAX)));
        let rect = Rectangle::from_loc_and_size(i32::MIN, i32::MIN, 2, 2);
        assert!(rect.contains(Point::new(i32::MIN, i32::MIN)));
        assert!(!rect.contains(Point::new(i32::MIN + 2, i32::MIN)));
        assert!(!Rectangle::from_loc_and_size(0, 0, 0, 1).contains(Point::ZERO));
    }

    #[test]
    fn overlaps_preserves_intersections_at_coordinate_limits() {
        let rect = Rectangle::from_loc_and_size(i32::MAX, i32::MAX, 1, 1);
        assert!(rect.overlaps(rect));
        let adjacent = Rectangle::from_loc_and_size(i32::MAX - 1, i32::MAX - 1, 1, 1);
        assert!(!rect.overlaps(adjacent));
        assert!(!adjacent.overlaps(rect));
        assert!(!rect.overlaps(Rectangle::from_loc_and_size(i32::MAX, i32::MAX, 0, 1)));
    }
}
