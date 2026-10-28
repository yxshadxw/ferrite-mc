/// One of the six block faces, in vanilla order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum Direction {
    #[default]
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Direction {
    pub const ALL: [Self; 6] = [
        Self::Down,
        Self::Up,
        Self::North,
        Self::South,
        Self::West,
        Self::East,
    ];

    /// Horizontal directions in vanilla's `Direction.Plane.HORIZONTAL` order.
    pub const HORIZONTAL: [Self; 4] = [Self::South, Self::West, Self::North, Self::East];

    pub const fn axis(self) -> Axis {
        match self {
            Self::Down | Self::Up => Axis::Y,
            Self::North | Self::South => Axis::Z,
            Self::West | Self::East => Axis::X,
        }
    }

    pub const fn opposite(self) -> Self {
        match self {
            Self::Down => Self::Up,
            Self::Up => Self::Down,
            Self::North => Self::South,
            Self::South => Self::North,
            Self::West => Self::East,
            Self::East => Self::West,
        }
    }

    /// The unit offset applied when moving one block in this direction.
    pub const fn step(self) -> (i32, i32, i32) {
        match self {
            Self::Down => (0, -1, 0),
            Self::Up => (0, 1, 0),
            Self::North => (0, 0, -1),
            Self::South => (0, 0, 1),
            Self::West => (-1, 0, 0),
            Self::East => (1, 0, 0),
        }
    }

    pub const fn is_horizontal(self) -> bool {
        matches!(self, Self::North | Self::South | Self::West | Self::East)
    }
}

/// One coordinate axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub const ALL: [Self; 3] = [Self::X, Self::Y, Self::Z];

    /// The face on the positive side of this axis.
    pub const fn positive_face(self) -> Direction {
        match self {
            Self::X => Direction::East,
            Self::Y => Direction::Up,
            Self::Z => Direction::South,
        }
    }

    /// The face on the negative side of this axis.
    pub const fn negative_face(self) -> Direction {
        match self {
            Self::X => Direction::West,
            Self::Y => Direction::Down,
            Self::Z => Direction::North,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Axis, Direction};

    #[test]
    fn opposites_and_axes() {
        assert_eq!(Direction::Up.opposite(), Direction::Down);
        assert_eq!(Direction::North.opposite(), Direction::South);
        assert_eq!(Direction::East.opposite(), Direction::West);
        assert_eq!(Direction::Up.axis(), Axis::Y);
        assert_eq!(Direction::West.axis(), Axis::X);
        assert_eq!(Direction::South.axis(), Axis::Z);
    }

    #[test]
    fn steps_match_faces() {
        assert_eq!(Direction::Down.step(), (0, -1, 0));
        assert_eq!(Direction::North.step(), (0, 0, -1));
        assert_eq!(Direction::West.step(), (-1, 0, 0));
        assert_eq!(Direction::East.step(), (1, 0, 0));
        assert_eq!(Axis::Y.negative_face(), Direction::Down);
        assert_eq!(Axis::Z.positive_face(), Direction::South);
    }
}
