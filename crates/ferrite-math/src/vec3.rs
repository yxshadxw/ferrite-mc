use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

use crate::direction::Axis;

/// A three-dimensional vector of `f64`, matching vanilla's `Vec3`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);

    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn length_sqr(self) -> f64 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    pub fn length(self) -> f64 {
        self.length_sqr().sqrt()
    }

    pub fn horizontal_distance_sqr(self) -> f64 {
        self.x * self.x + self.z * self.z
    }

    pub fn horizontal_distance(self) -> f64 {
        self.horizontal_distance_sqr().sqrt()
    }

    /// Returns the unit vector, or [`Vec3::ZERO`] for vectors shorter than
    /// `1e-4`, like vanilla's `Vec3.normalize`.
    pub fn normalize(self) -> Self {
        let length = self.length();
        if length < 1.0e-4 {
            Self::ZERO
        } else {
            self * (1.0 / length)
        }
    }

    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    /// Linear interpolation from `self` to `other` by `delta`.
    pub fn lerp(self, other: Self, delta: f64) -> Self {
        Self::new(
            self.x + (other.x - self.x) * delta,
            self.y + (other.y - self.y) * delta,
            self.z + (other.z - self.z) * delta,
        )
    }

    pub fn distance_to_sqr(self, other: Self) -> f64 {
        (self - other).length_sqr()
    }

    pub fn distance_to(self, other: Self) -> f64 {
        self.distance_to_sqr(other).sqrt()
    }

    pub(crate) fn component(self, axis: Axis) -> f64 {
        match axis {
            Axis::X => self.x,
            Axis::Y => self.y,
            Axis::Z => self.z,
        }
    }
}

impl Add for Vec3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for Vec3 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl SubAssign for Vec3 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Neg for Vec3 {
    type Output = Self;

    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

impl Mul<f64> for Vec3 {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl Div<f64> for Vec3 {
    type Output = Self;

    fn div(self, rhs: f64) -> Self {
        Self::new(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::Vec3;

    #[test]
    fn length_and_normalize() {
        let vector = Vec3::new(3.0, 4.0, 0.0);
        assert_eq!(vector.length_sqr(), 25.0);
        assert_eq!(vector.length(), 5.0);

        let unit = vector.normalize();
        assert!((unit.length() - 1.0).abs() < 1e-12);
        assert_eq!(Vec3::new(1e-5, 0.0, 0.0).normalize(), Vec3::ZERO);
    }

    #[test]
    fn dot_and_cross() {
        let x = Vec3::new(1.0, 0.0, 0.0);
        let y = Vec3::new(0.0, 1.0, 0.0);
        assert_eq!(x.dot(y), 0.0);
        assert_eq!(x.cross(y), Vec3::new(0.0, 0.0, 1.0));
    }
}
