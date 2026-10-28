use crate::EPSILON;
use crate::direction::{Axis, Direction};
use crate::vec3::Vec3;

/// An axis-aligned bounding box, matching vanilla's `AABB`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AABB {
    pub min: Vec3,
    pub max: Vec3,
}

/// Where a ray enters an [`AABB`], and which face it entered through.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipHit {
    pub location: Vec3,
    pub face: Direction,
}

impl AABB {
    pub const fn new(
        min_x: f64,
        min_y: f64,
        min_z: f64,
        max_x: f64,
        max_y: f64,
        max_z: f64,
    ) -> Self {
        Self {
            min: Vec3::new(min_x, min_y, min_z),
            max: Vec3::new(max_x, max_y, max_z),
        }
    }

    pub const fn from_corners(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    pub fn size(self) -> Vec3 {
        self.max - self.min
    }

    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Grows the box by the given amounts on each axis.
    pub fn inflate(self, x: f64, y: f64, z: f64) -> Self {
        Self {
            min: Vec3::new(self.min.x - x, self.min.y - y, self.min.z - z),
            max: Vec3::new(self.max.x + x, self.max.y + y, self.max.z + z),
        }
    }

    /// Grows the box by the same amount on every side.
    pub fn inflate_all(self, amount: f64) -> Self {
        self.inflate(amount, amount, amount)
    }

    /// Shrinks the box by the given amounts on each axis.
    pub fn deflate(self, x: f64, y: f64, z: f64) -> Self {
        self.inflate(-x, -y, -z)
    }

    /// Shrinks the box by the same amount on every side.
    pub fn deflate_all(self, amount: f64) -> Self {
        self.inflate_all(-amount)
    }

    pub fn move_by(self, delta: Vec3) -> Self {
        Self {
            min: self.min + delta,
            max: self.max + delta,
        }
    }

    /// The union of this box and this box moved by `delta`, as used to gather
    /// collision shapes along a movement.
    pub fn expand_towards(self, delta: Vec3) -> Self {
        self.union(self.move_by(delta))
    }

    /// Strict overlap test, matching vanilla's `AABB.intersects`.
    pub fn intersects(self, other: Self) -> bool {
        self.min.x < other.max.x
            && self.max.x > other.min.x
            && self.min.y < other.max.y
            && self.max.y > other.min.y
            && self.min.z < other.max.z
            && self.max.z > other.min.z
    }

    /// Whether `point` lies inside the half-open box `[min, max)`.
    pub fn contains(self, point: Vec3) -> bool {
        point.x >= self.min.x
            && point.x < self.max.x
            && point.y >= self.min.y
            && point.y < self.max.y
            && point.z >= self.min.z
            && point.z < self.max.z
    }

    /// Whether `other` lies fully inside this box (inclusive bounds).
    pub fn contains_box(self, other: Self) -> bool {
        other.min.x >= self.min.x
            && other.max.x <= self.max.x
            && other.min.y >= self.min.y
            && other.max.y <= self.max.y
            && other.min.z >= self.min.z
            && other.max.z <= self.max.z
    }

    pub fn union(self, other: Self) -> Self {
        Self {
            min: Vec3::new(
                self.min.x.min(other.min.x),
                self.min.y.min(other.min.y),
                self.min.z.min(other.min.z),
            ),
            max: Vec3::new(
                self.max.x.max(other.max.x),
                self.max.y.max(other.max.y),
                self.max.z.max(other.max.z),
            ),
        }
    }

    /// Clips the segment `from..to` against this box.
    ///
    /// Returns the first point at which the segment enters the box and the
    /// face it entered through. A segment that starts inside the box reports
    /// the start point and the nearest face it would exit through.
    pub fn clip(self, from: Vec3, to: Vec3) -> Option<ClipHit> {
        let delta = to - from;
        let mut enter_t = 0.0_f64;
        let mut exit_t = 1.0_f64;
        let mut enter_face: Option<Direction> = None;
        let mut nearest_exit: Option<(f64, Direction)> = None;

        for axis in Axis::ALL {
            let origin = from.component(axis);
            let direction = delta.component(axis);
            let min = self.min.component(axis);
            let max = self.max.component(axis);

            if direction.abs() < EPSILON {
                if origin < min - EPSILON || origin > max + EPSILON {
                    return None;
                }
                continue;
            }

            let inverse = 1.0 / direction;
            let (near_t, far_t, near_face, far_face) = if direction > 0.0 {
                (
                    (min - origin) * inverse,
                    (max - origin) * inverse,
                    axis.negative_face(),
                    axis.positive_face(),
                )
            } else {
                (
                    (max - origin) * inverse,
                    (min - origin) * inverse,
                    axis.positive_face(),
                    axis.negative_face(),
                )
            };

            if near_t >= -EPSILON && near_t > enter_t - EPSILON {
                enter_t = enter_t.max(near_t);
                enter_face = Some(near_face);
            }
            if far_t < exit_t {
                exit_t = far_t;
            }
            if nearest_exit.is_none_or(|(exit, _)| far_t < exit) {
                nearest_exit = Some((far_t, far_face));
            }
            if enter_t > exit_t + EPSILON {
                return None;
            }
        }

        if exit_t < -EPSILON || enter_t > 1.0 + EPSILON {
            return None;
        }

        let t = enter_t.clamp(0.0, 1.0);
        let face = match enter_face {
            Some(face) => face,
            // The origin is inside the box; report the face it would leave by.
            None => nearest_exit?.1,
        };
        Some(ClipHit {
            location: from + delta * t,
            face,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::AABB;
    use crate::direction::Direction;
    use crate::vec3::Vec3;

    fn assert_near(actual: Vec3, expected: Vec3) {
        assert!(
            (actual - expected).length() < 1e-9,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn inflate_deflate_and_move() {
        let cube = AABB::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        assert_eq!(
            cube.inflate_all(0.5),
            AABB::new(-0.5, -0.5, -0.5, 1.5, 1.5, 1.5)
        );
        assert_eq!(cube.inflate_all(0.5).deflate_all(0.5), cube);
        assert_eq!(
            cube.move_by(Vec3::new(1.0, 2.0, 3.0)),
            AABB::new(1.0, 2.0, 3.0, 2.0, 3.0, 4.0)
        );
        assert_eq!(cube.center(), Vec3::new(0.5, 0.5, 0.5));
    }

    #[test]
    fn intersects_contains_and_union() {
        let cube = AABB::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        assert!(cube.intersects(AABB::new(0.5, 0.5, 0.5, 2.0, 2.0, 2.0)));
        assert!(!cube.intersects(AABB::new(1.0, 0.0, 0.0, 2.0, 1.0, 1.0)));

        assert!(cube.contains(Vec3::new(0.0, 0.5, 0.5)));
        assert!(!cube.contains(Vec3::new(1.0, 0.5, 0.5)));
        assert!(!cube.contains(Vec3::new(0.5, 1.5, 0.5)));

        assert_eq!(
            cube.union(AABB::new(0.5, 0.5, 0.5, 2.0, 2.0, 2.0)),
            AABB::new(0.0, 0.0, 0.0, 2.0, 2.0, 2.0)
        );
        assert!(AABB::new(0.0, 0.0, 0.0, 3.0, 3.0, 3.0).contains_box(cube));
    }

    #[test]
    fn clip_reports_entry_face() {
        let cube = AABB::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        let west = cube
            .clip(Vec3::new(-1.0, 0.5, 0.5), Vec3::new(2.0, 0.5, 0.5))
            .unwrap();
        assert_eq!(west.face, Direction::West);
        assert_near(west.location, Vec3::new(0.0, 0.5, 0.5));

        let east = cube
            .clip(Vec3::new(2.0, 0.5, 0.5), Vec3::new(-1.0, 0.5, 0.5))
            .unwrap();
        assert_eq!(east.face, Direction::East);
        assert_near(east.location, Vec3::new(1.0, 0.5, 0.5));

        let up = cube
            .clip(Vec3::new(0.5, 2.0, 0.5), Vec3::new(0.5, -1.0, 0.5))
            .unwrap();
        assert_eq!(up.face, Direction::Up);
        assert_near(up.location, Vec3::new(0.5, 1.0, 0.5));

        assert!(
            cube.clip(Vec3::new(-1.0, 2.0, 0.5), Vec3::new(2.0, 2.0, 0.5))
                .is_none()
        );
    }
}
