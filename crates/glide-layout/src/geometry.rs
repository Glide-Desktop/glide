//! Custom geometric types (Point, Size, Rectangle).

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
    pub const ZERO: Self = Self {
        width: 0,
        height: 0,
    };

    #[must_use]
    pub const fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.width <= 0 || self.height <= 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Rectangle {
    pub loc: Point,
    pub size: Size,
}

impl Rectangle {
    pub const ZERO: Self = Self {
        loc: Point::ZERO,
        size: Size::ZERO,
    };

    #[must_use]
    pub const fn new(loc: Point, size: Size) -> Self {
        Self { loc, size }
    }

    #[must_use]
    pub const fn from_loc_and_size(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            loc: Point::new(x, y),
            size: Size::new(width, height),
        }
    }

    #[must_use]
    pub const fn contains(&self, p: Point) -> bool {
        p.x >= self.loc.x
            && p.x < self.loc.x + self.size.width
            && p.y >= self.loc.y
            && p.y < self.loc.y + self.size.height
    }

    #[must_use]
    pub const fn center(&self) -> Point {
        Point {
            x: self.loc.x + self.size.width / 2,
            y: self.loc.y + self.size.height / 2,
        }
    }

    /// Whether any part of `other` lies inside `self`.
    ///
    /// Empty rectangles never overlap anything, so a zero-sized window is not
    /// treated as being "on screen".
    #[must_use]
    pub const fn overlaps(&self, other: Self) -> bool {
        if self.size.is_empty() || other.size.is_empty() {
            return false;
        }

        self.loc.x < other.loc.x + other.size.width
            && other.loc.x < self.loc.x + self.size.width
            && self.loc.y < other.loc.y + other.size.height
            && other.loc.y < self.loc.y + self.size.height
    }
}
