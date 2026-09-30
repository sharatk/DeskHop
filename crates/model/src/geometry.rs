//! Screen geometry in per-monitor-DPI-aware virtual-screen pixels.

/// A pixel position in virtual-screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// A rectangle of pixels; `x..x + w` by `y..y + h`, end exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub const fn right(&self) -> i32 {
        self.x + self.w
    }

    pub const fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub const fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    /// The span `(start, len)` of this rectangle along the axis a side runs.
    const fn span_along(&self, side: Side) -> (i32, i32) {
        if side.is_vertical() {
            (self.y, self.h)
        } else {
            (self.x, self.w)
        }
    }
}

/// A side of a monitor or of a machine's whole screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    pub const fn opposite(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
            Self::Top => Self::Bottom,
            Self::Bottom => Self::Top,
        }
    }

    /// True for left and right, whose sides run vertically.
    pub const fn is_vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }

    /// One pixel step outward through this side.
    const fn step(self) -> (i32, i32) {
        match self {
            Self::Left => (-1, 0),
            Self::Right => (1, 0),
            Self::Top => (0, -1),
            Self::Bottom => (0, 1),
        }
    }

    /// The outward component of a movement through this side; 0 or less
    /// means the movement does not push through it.
    pub const fn outward(self, dx: i32, dy: i32) -> i32 {
        match self {
            Self::Left => -dx,
            Self::Right => dx,
            Self::Top => -dy,
            Self::Bottom => dy,
        }
    }
}

/// An opaque, stable monitor identity supplied by the agent (ADR 0005).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MonitorId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    pub id: MonitorId,
    pub rect: Rect,
    pub dpi: u32,
}

impl Monitor {
    /// True if `p` is on this monitor's last pixel row or column toward `side`.
    pub const fn on_side(&self, p: Point, side: Side) -> bool {
        if !self.rect.contains(p) {
            return false;
        }
        match side {
            Side::Left => p.x == self.rect.x,
            Side::Right => p.x == self.rect.right() - 1,
            Side::Top => p.y == self.rect.y,
            Side::Bottom => p.y == self.rect.bottom() - 1,
        }
    }

    /// True if `p` on `side` lies within `zone_px` of either end of that side.
    pub const fn in_corner(&self, p: Point, side: Side, zone_px: i32) -> bool {
        let (start, len) = self.rect.span_along(side);
        let t = if side.is_vertical() { p.y } else { p.x } - start;
        t < zone_px || t >= len - zone_px
    }
}

/// Length in pixels of a corner zone of `tenths_mm` tenths of a millimetre on
/// a monitor at `dpi`, rounded to the nearest pixel.
pub const fn corner_zone_px(dpi: u32, tenths_mm: u32) -> i32 {
    // px = mm * dpi / 25.4 = tenths_mm * dpi / 254
    let px = (tenths_mm as u64 * dpi as u64 + 127) / 254;
    if px > i32::MAX as u64 {
        i32::MAX
    } else {
        px as i32
    }
}

/// A position along a side as a fixed-point fraction: 0 is the start (top or
/// left), 65535 the end. Identical on every machine and float-free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EdgeFraction(pub u16);

impl EdgeFraction {
    /// The fraction of the way `t` lies along `0..len`.
    fn of(t: i64, len: i64) -> Self {
        if len <= 1 {
            return Self(0);
        }
        let t = t.clamp(0, len - 1);
        let max = i64::from(u16::MAX);
        let f = (t * max + (len - 1) / 2) / (len - 1);
        Self(u16::try_from(f).unwrap_or(u16::MAX))
    }

    /// The offset along `0..len` this fraction names.
    fn at(self, len: i64) -> i64 {
        if len <= 1 {
            return 0;
        }
        let max = i64::from(u16::MAX);
        (i64::from(self.0) * (len - 1) + max / 2) / max
    }
}

/// The set of monitors a machine currently has.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Screen {
    pub monitors: Vec<Monitor>,
}

impl Screen {
    pub fn new(monitors: Vec<Monitor>) -> Self {
        Self { monitors }
    }

    pub fn monitor_at(&self, p: Point) -> Option<&Monitor> {
        self.monitors.iter().find(|m| m.rect.contains(p))
    }

    /// The bounding rectangle of all monitors.
    pub fn bounds(&self) -> Option<Rect> {
        let mut it = self.monitors.iter().map(|m| m.rect);
        let first = it.next()?;
        Some(it.fold(first, |b, r| {
            let x = b.x.min(r.x);
            let y = b.y.min(r.y);
            Rect::new(
                x,
                y,
                b.right().max(r.right()) - x,
                b.bottom().max(r.bottom()) - y,
            )
        }))
    }

    /// The monitor whose outer `side` the cursor at `p` is on: `p` is on that
    /// side of its monitor and no monitor holds the pixel one step beyond.
    pub fn outer_edge(&self, p: Point, side: Side) -> Option<&Monitor> {
        let m = self.monitor_at(p)?;
        if !m.on_side(p, side) {
            return None;
        }
        let (dx, dy) = side.step();
        let beyond = Point::new(p.x + dx, p.y + dy);
        self.monitor_at(beyond).is_none().then_some(m)
    }

    /// How far along this machine's whole extent `p` lies, on the axis that
    /// `side` runs along.
    pub fn fraction_at(&self, side: Side, p: Point) -> EdgeFraction {
        let Some(b) = self.bounds() else {
            return EdgeFraction(0);
        };
        let (start, len) = b.span_along(side);
        let t = if side.is_vertical() { p.y } else { p.x };
        EdgeFraction::of(i64::from(t) - i64::from(start), i64::from(len))
    }

    /// Where a cursor entering through `side` at `fraction` lands: on the
    /// outermost monitor toward `side` that covers that position, or else the
    /// nearest monitor along the side.
    pub fn entry_point(&self, side: Side, fraction: EdgeFraction) -> Option<Point> {
        let b = self.bounds()?;
        let (start, len) = b.span_along(side);
        let t = i64::from(start) + fraction.at(i64::from(len));

        let distance = |m: &Monitor| {
            let (s, l) = m.rect.span_along(side);
            let (s, e) = (i64::from(s), i64::from(s) + i64::from(l) - 1);
            if t < s {
                s - t
            } else if t > e {
                t - e
            } else {
                0
            }
        };
        let outwardness = |m: &Monitor| match side {
            Side::Left => -m.rect.x,
            Side::Right => m.rect.right(),
            Side::Top => -m.rect.y,
            Side::Bottom => m.rect.bottom(),
        };
        let m = self
            .monitors
            .iter()
            .min_by_key(|m| (distance(m), -i64::from(outwardness(m))))?;

        let (s, l) = m.rect.span_along(side);
        let along = t.clamp(i64::from(s), i64::from(s) + i64::from(l) - 1);
        let along = i32::try_from(along).unwrap_or(s);
        Some(match side {
            Side::Left => Point::new(m.rect.x, along),
            Side::Right => Point::new(m.rect.right() - 1, along),
            Side::Top => Point::new(along, m.rect.y),
            Side::Bottom => Point::new(along, m.rect.bottom() - 1),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(id: &str, x: i32, y: i32, w: i32, h: i32, dpi: u32) -> Monitor {
        Monitor {
            id: MonitorId(id.into()),
            rect: Rect::new(x, y, w, h),
            dpi,
        }
    }

    #[test]
    fn single_monitor_edges_are_outer() {
        let s = Screen::new(vec![monitor("a", 0, 0, 1920, 1080, 96)]);
        assert!(s.outer_edge(Point::new(1919, 500), Side::Right).is_some());
        assert!(s.outer_edge(Point::new(0, 500), Side::Left).is_some());
        assert!(s.outer_edge(Point::new(1918, 500), Side::Right).is_none());
    }

    #[test]
    fn side_by_side_inner_edge_is_not_outer() {
        let s = Screen::new(vec![
            monitor("a", 0, 0, 1920, 1080, 96),
            monitor("b", 1920, 0, 1920, 1080, 96),
        ]);
        assert!(s.outer_edge(Point::new(1919, 500), Side::Right).is_none());
        assert!(s.outer_edge(Point::new(3839, 500), Side::Right).is_some());
    }

    #[test]
    fn partly_overlapping_pair() {
        // b is shorter and aligned to the bottom of a.
        let s = Screen::new(vec![
            monitor("a", 0, 0, 1920, 1440, 96),
            monitor("b", 1920, 360, 1920, 1080, 96),
        ]);
        // Beside b: inner. Above b's top: outer.
        assert!(s.outer_edge(Point::new(1919, 800), Side::Right).is_none());
        assert!(s.outer_edge(Point::new(1919, 100), Side::Right).is_some());
    }

    #[test]
    fn corner_lengths_follow_dpi() {
        assert_eq!(corner_zone_px(96, 20), 8);
        assert_eq!(corner_zone_px(192, 20), 15);
    }

    #[test]
    fn in_corner_at_both_ends() {
        let m = monitor("a", 0, 0, 1920, 1080, 96);
        assert!(m.in_corner(Point::new(1919, 7), Side::Right, 8));
        assert!(!m.in_corner(Point::new(1919, 8), Side::Right, 8));
        assert!(!m.in_corner(Point::new(1919, 1071), Side::Right, 8));
        assert!(m.in_corner(Point::new(1919, 1072), Side::Right, 8));
    }

    #[test]
    fn fractions_at_ends_and_quarter() {
        let s = Screen::new(vec![monitor("a", 0, 0, 1920, 1080, 96)]);
        assert_eq!(
            s.fraction_at(Side::Right, Point::new(1919, 0)),
            EdgeFraction(0)
        );
        assert_eq!(
            s.fraction_at(Side::Right, Point::new(1919, 1079)),
            EdgeFraction(u16::MAX)
        );
        let quarter = s.fraction_at(Side::Right, Point::new(1919, 270));
        let tall = Screen::new(vec![monitor("b", 0, 0, 3840, 2160, 192)]);
        assert_eq!(
            tall.entry_point(Side::Left, quarter),
            Some(Point::new(0, 540))
        );
    }

    #[test]
    fn round_trip_on_same_size() {
        let s = Screen::new(vec![monitor("a", 0, 0, 1920, 1080, 96)]);
        for y in [0, 1, 270, 540, 648, 1078, 1079] {
            let f = s.fraction_at(Side::Right, Point::new(1919, y));
            assert_eq!(s.entry_point(Side::Left, f), Some(Point::new(0, y)));
        }
    }

    #[test]
    fn landing_in_a_gap_clamps_to_nearest_monitor() {
        // Two monitors stacked with a gap between y 1080 and 1200.
        let s = Screen::new(vec![
            monitor("top", 0, 0, 1920, 1080, 96),
            monitor("bottom", 0, 1200, 1920, 1080, 96),
        ]);
        // Bounds are 0..2280; 1100 is in the gap, nearer the top monitor.
        let f = EdgeFraction::of(1100, 2280);
        let p = s.entry_point(Side::Left, f).unwrap();
        assert_eq!(p, Point::new(0, 1079));
    }

    #[test]
    fn entry_prefers_outermost_monitor() {
        // b sits to the right of a; entering through the right lands on b.
        let s = Screen::new(vec![
            monitor("a", 0, 0, 1920, 1080, 96),
            monitor("b", 1920, 0, 1920, 1080, 96),
        ]);
        let p = s.entry_point(Side::Right, EdgeFraction(32768)).unwrap();
        assert_eq!(p.x, 3839);
    }
}
