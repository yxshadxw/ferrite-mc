//! Conversions between entity yaw/pitch and look vectors.
//!
//! Yaw is in degrees, `0` facing south (`+Z`) and increasing clockwise, so
//! `90` faces west, `180` north and `-90` east. Positive pitch looks down.

use crate::EPSILON;
use crate::mth;
use crate::vec3::Vec3;

/// The unit look vector for a yaw and pitch in degrees, matching vanilla's
/// `Entity.calculateViewVector`.
pub fn look_from_yaw_pitch(yaw_degrees: f32, pitch_degrees: f32) -> Vec3 {
    let yaw = f64::from(yaw_degrees).to_radians();
    let pitch = f64::from(pitch_degrees).to_radians();
    Vec3::new(
        -yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    )
}

/// The yaw and pitch in degrees for a look vector.
///
/// Yaw is wrapped into `[-180, 180)`; a zero-length vector yields `(0, 0)`.
pub fn yaw_pitch_from_look(look: Vec3) -> (f32, f32) {
    if look.length_sqr() < EPSILON * EPSILON {
        return (0.0, 0.0);
    }

    let horizontal = look.horizontal_distance();
    let yaw = (-look.x).atan2(look.z).to_degrees();
    let pitch = (-look.y).atan2(horizontal).to_degrees();
    (mth::wrap_degrees(yaw as f32), pitch as f32)
}

#[cfg(test)]
mod tests {
    use super::{look_from_yaw_pitch, yaw_pitch_from_look};

    #[test]
    fn cardinal_directions() {
        let south = look_from_yaw_pitch(0.0, 0.0);
        assert!(south.z > 0.999);
        assert!(south.x.abs() < 1e-6);

        let west = look_from_yaw_pitch(90.0, 0.0);
        assert!(west.x < -0.999);
        assert!(west.z.abs() < 1e-6);

        let north = look_from_yaw_pitch(180.0, 0.0);
        assert!(north.z < -0.999);

        let east = look_from_yaw_pitch(-90.0, 0.0);
        assert!(east.x > 0.999);

        let down = look_from_yaw_pitch(0.0, 90.0);
        assert!(down.y < -0.999);

        let up = look_from_yaw_pitch(0.0, -90.0);
        assert!(up.y > 0.999);
    }

    #[test]
    fn round_trips() {
        let look = look_from_yaw_pitch(-45.0, 30.0);
        let (yaw, pitch) = yaw_pitch_from_look(look);
        assert!((yaw + 45.0).abs() < 1e-4, "yaw was {yaw}");
        assert!((pitch - 30.0).abs() < 1e-4, "pitch was {pitch}");
        assert_eq!(yaw_pitch_from_look(crate::Vec3::ZERO), (0.0, 0.0));
    }
}
