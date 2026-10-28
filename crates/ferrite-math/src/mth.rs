//! Helpers mirroring vanilla's `Mth` (Minecraft math) class.

/// Floors a `f64` to an `i32`, matching vanilla's `Mth.floor(double)`.
pub fn floor(value: f64) -> i32 {
    value.floor() as i32
}

/// Floors an `f32` to an `i32`, matching vanilla's `Mth.floor(float)`.
pub fn floor_f32(value: f32) -> i32 {
    value.floor() as i32
}

/// Interpolates from `start` to `end` by `delta`, matching vanilla's
/// `Mth.lerp(double delta, double start, double end)`.
pub fn lerp(delta: f64, start: f64, end: f64) -> f64 {
    start + delta * (end - start)
}

/// `f32` variant of [`lerp`].
pub fn lerp_f32(delta: f32, start: f32, end: f32) -> f32 {
    start + delta * (end - start)
}

/// Clamps `value` into `min..=max`, matching vanilla's `Mth.clamp`.
pub fn clamp<T: PartialOrd>(value: T, min: T, max: T) -> T {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// Wraps an angle in degrees into `[-180, 180)`, matching vanilla's
/// `Mth.wrapDegrees(float)`.
pub fn wrap_degrees(degrees: f32) -> f32 {
    let mut wrapped = degrees % 360.0;
    if wrapped >= 180.0 {
        wrapped -= 360.0;
    }
    if wrapped < -180.0 {
        wrapped += 360.0;
    }
    wrapped
}

/// `f64` variant of [`wrap_degrees`].
pub fn wrap_degrees_f64(degrees: f64) -> f64 {
    let mut wrapped = degrees % 360.0;
    if wrapped >= 180.0 {
        wrapped -= 360.0;
    }
    if wrapped < -180.0 {
        wrapped += 360.0;
    }
    wrapped
}

#[cfg(test)]
mod tests {
    use super::{clamp, floor, floor_f32, lerp, lerp_f32, wrap_degrees, wrap_degrees_f64};

    #[test]
    fn floor_handles_negatives() {
        assert_eq!(floor(0.999), 0);
        assert_eq!(floor(-0.1), -1);
        assert_eq!(floor(-16.1), -17);
        assert_eq!(floor_f32(-1.0), -1);
    }

    #[test]
    fn lerp_and_clamp() {
        assert!((lerp(0.5, 2.0, 4.0) - 3.0).abs() < f64::EPSILON);
        assert!((lerp_f32(0.25, 0.0, 8.0) - 2.0).abs() < f32::EPSILON);
        assert_eq!(clamp(5, 0, 3), 3);
        assert_eq!(clamp(-2, 0, 3), 0);
        assert_eq!(clamp(2, 0, 3), 2);
    }

    #[test]
    fn wrap_degrees_uses_half_open_range() {
        assert_eq!(wrap_degrees(0.0), 0.0);
        assert_eq!(wrap_degrees(180.0), -180.0);
        assert_eq!(wrap_degrees(541.0), -179.0);
        assert_eq!(wrap_degrees(-181.0), 179.0);
        assert_eq!(wrap_degrees(90.0), 90.0);
        assert_eq!(wrap_degrees_f64(-540.0), -180.0);
    }
}
