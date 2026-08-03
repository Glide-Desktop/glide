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

/// Space a decoration claims on each side of a window's frame, between the
/// frame (what the user sees and drags) and the content (what the client
/// actually draws into).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Insets {
    pub top: i32,
    pub left: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Insets {
    /// No decoration: frame and content coincide.
    pub const NONE: Self = Self {
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
    };

    #[must_use]
    pub const fn width(&self) -> i32 {
        self.left + self.right
    }

    #[must_use]
    pub const fn height(&self) -> i32 {
        self.top + self.bottom
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

    /// Shrinks the rectangle by `insets` on each side. Used to derive a
    /// decorated window's content area (the part the client actually draws
    /// into) from its frame (the part the user sees and drags).
    #[must_use]
    pub const fn inset(&self, insets: Insets) -> Self {
        Self {
            loc: Point::new(self.loc.x + insets.left, self.loc.y + insets.top),
            size: Size::new(
                self.size.width - insets.width(),
                self.size.height - insets.height(),
            ),
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
