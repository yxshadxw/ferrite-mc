//! Coordinate arguments: `block_pos`, `vec3`, `vec2`, `column_pos`, `rotation`.
//!
//! Ports of vanilla's `WorldCoordinate`, `WorldCoordinates` and `LocalCoordinates`.

use crate::error::{CommandSyntaxError, minecraft};
use crate::reader::{ARGUMENT_SEPARATOR, StringReader};

/// One world coordinate: absolute, or relative (`~`) to the source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldCoordinate {
    pub relative: bool,
    pub value: f64,
}

impl WorldCoordinate {
    pub fn absolute(value: f64) -> Self {
        Self {
            relative: false,
            value,
        }
    }

    pub fn relative(value: f64) -> Self {
        Self {
            relative: true,
            value,
        }
    }

    /// Resolves against the source's coordinate on this axis.
    pub fn resolve(&self, origin: f64) -> f64 {
        if self.relative {
            origin + self.value
        } else {
            self.value
        }
    }
}

/// Three coordinates, either world (`x ~y ~z`) or local (`^left ^up ^forwards`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Coordinates {
    World([WorldCoordinate; 3]),
    Local { left: f64, up: f64, forwards: f64 },
}

impl Coordinates {
    /// Resolves against the source's position and rotation (Brigadier's
    /// `WorldCoordinates`/`LocalCoordinates.toAbsolute`).
    ///
    /// Local coordinates use Minecraft's axes: yaw 0 faces +Z, yaw 90 faces -X,
    /// and positive pitch looks down.
    pub fn resolve(&self, origin: [f64; 3], yaw: f32, pitch: f32) -> [f64; 3] {
        match self {
            Self::World([x, y, z]) => [
                x.resolve(origin[0]),
                y.resolve(origin[1]),
                z.resolve(origin[2]),
            ],
            Self::Local { left, up, forwards } => {
                let (sin_yaw, cos_yaw) = f64::from(yaw).to_radians().sin_cos();
                let (sin_pitch, cos_pitch) = f64::from(pitch).to_radians().sin_cos();
                let forward = [-sin_yaw * cos_pitch, -sin_pitch, cos_yaw * cos_pitch];
                let up_axis = [sin_yaw * sin_pitch, cos_pitch, -cos_yaw * sin_pitch];
                let left_axis = [cos_yaw, 0.0, -sin_yaw];
                [0, 1, 2].map(|i| {
                    origin[i] + left_axis[i] * left + up_axis[i] * up + forward[i] * forwards
                })
            }
        }
    }
}

/// Resolves a `rotation` pair against the source's yaw and pitch.
pub fn resolve_rotation(rotation: [WorldCoordinate; 2], yaw: f32, pitch: f32) -> (f32, f32) {
    (
        rotation[0].resolve(f64::from(yaw)) as f32,
        rotation[1].resolve(f64::from(pitch)) as f32,
    )
}

fn read_relative(reader: &mut StringReader) -> bool {
    if reader.peek_is('~') {
        reader.skip();
        true
    } else {
        false
    }
}

fn has_value(reader: &StringReader) -> bool {
    reader.peek().is_some_and(|c| c != ARGUMENT_SEPARATOR)
}

/// `WorldCoordinate.parseInt`.
pub fn parse_int(reader: &mut StringReader) -> Result<WorldCoordinate, CommandSyntaxError> {
    if reader.peek_is('^') {
        return Err(minecraft::pos_mixed(reader));
    }
    if !reader.can_read() {
        return Err(minecraft::pos_missing_int(reader));
    }
    let relative = read_relative(reader);
    let value = if has_value(reader) {
        if relative {
            reader.read_double()?
        } else {
            f64::from(reader.read_int()?)
        }
    } else {
        0.0
    };
    Ok(WorldCoordinate { relative, value })
}

/// `WorldCoordinate.parseDouble`; `center` adds 0.5 to absolute integers.
pub fn parse_double(
    reader: &mut StringReader,
    center: bool,
) -> Result<WorldCoordinate, CommandSyntaxError> {
    if reader.peek_is('^') {
        return Err(minecraft::pos_mixed(reader));
    }
    if !reader.can_read() {
        return Err(minecraft::pos_missing_double(reader));
    }
    let relative = read_relative(reader);
    let start = reader.cursor();
    let mut value = if has_value(reader) {
        reader.read_double()?
    } else {
        0.0
    };
    let literal = &reader.string()[start..reader.cursor()];
    if !relative && center && !literal.contains('.') {
        value += 0.5;
    }
    Ok(WorldCoordinate { relative, value })
}

fn separator(
    reader: &mut StringReader,
    start: usize,
    incomplete: fn(&StringReader) -> CommandSyntaxError,
) -> Result<(), CommandSyntaxError> {
    if reader.peek_is(ARGUMENT_SEPARATOR) {
        reader.skip();
        Ok(())
    } else {
        reader.set_cursor(start);
        Err(incomplete(reader))
    }
}

fn parse_local(reader: &mut StringReader) -> Result<Coordinates, CommandSyntaxError> {
    let start = reader.cursor();
    let read = |reader: &mut StringReader| -> Result<f64, CommandSyntaxError> {
        if !reader.can_read() {
            return Err(minecraft::pos_missing_double(reader));
        }
        if !reader.peek_is('^') {
            reader.set_cursor(start);
            return Err(minecraft::pos_mixed(reader));
        }
        reader.skip();
        if has_value(reader) {
            reader.read_double()
        } else {
            Ok(0.0)
        }
    };
    let left = read(reader)?;
    separator(reader, start, minecraft::pos3d_incomplete)?;
    let up = read(reader)?;
    separator(reader, start, minecraft::pos3d_incomplete)?;
    let forwards = read(reader)?;
    Ok(Coordinates::Local { left, up, forwards })
}

/// `BlockPosArgument.parse` (integers, `~`, or `^`).
pub fn parse_block_pos(reader: &mut StringReader) -> Result<Coordinates, CommandSyntaxError> {
    if reader.peek_is('^') {
        return parse_local(reader);
    }
    let start = reader.cursor();
    let x = parse_int(reader)?;
    separator(reader, start, minecraft::pos3d_incomplete)?;
    let y = parse_int(reader)?;
    separator(reader, start, minecraft::pos3d_incomplete)?;
    let z = parse_int(reader)?;
    Ok(Coordinates::World([x, y, z]))
}

/// `Vec3Argument.parse`; `center` shifts absolute integer x/z to the block center.
pub fn parse_vec3(
    reader: &mut StringReader,
    center: bool,
) -> Result<Coordinates, CommandSyntaxError> {
    if reader.peek_is('^') {
        return parse_local(reader);
    }
    let start = reader.cursor();
    let x = parse_double(reader, center)?;
    separator(reader, start, minecraft::pos3d_incomplete)?;
    let y = parse_double(reader, false)?;
    separator(reader, start, minecraft::pos3d_incomplete)?;
    let z = parse_double(reader, center)?;
    Ok(Coordinates::World([x, y, z]))
}

/// Two coordinates (`vec2`, `rotation` with doubles; `column_pos` with integers).
pub fn parse_pair(
    reader: &mut StringReader,
    int: bool,
    center: bool,
) -> Result<[WorldCoordinate; 2], CommandSyntaxError> {
    let start = reader.cursor();
    if !reader.can_read() {
        return Err(minecraft::pos2d_incomplete(reader));
    }
    let read = |reader: &mut StringReader| {
        if int {
            parse_int(reader)
        } else {
            parse_double(reader, center)
        }
    };
    let a = read(reader)?;
    separator(reader, start, minecraft::pos2d_incomplete)?;
    let b = read(reader)?;
    Ok([a, b])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_pos_forms() {
        let w = |s: &str| parse_block_pos(&mut StringReader::new(s.to_owned()));
        assert_eq!(
            w("1 ~ ~-2").unwrap(),
            Coordinates::World([
                WorldCoordinate::absolute(1.0),
                WorldCoordinate::relative(0.0),
                WorldCoordinate::relative(-2.0)
            ])
        );
        assert_eq!(
            w("^ ^1 ^-.5").unwrap(),
            Coordinates::Local {
                left: 0.0,
                up: 1.0,
                forwards: -0.5
            }
        );
        assert_eq!(
            w("1 2").unwrap_err().message(),
            "Incomplete (expected 3 coordinates)"
        );
        assert_eq!(w("1 2").unwrap_err().cursor(), Some(0));
        assert_eq!(
            w("1 ^ 2").unwrap_err().translation_key(),
            "argument.pos.mixed"
        );
        assert_eq!(
            w("^ 1 2").unwrap_err().translation_key(),
            "argument.pos.mixed"
        );
        assert_eq!(w("1.5 0 0").unwrap_err().message(), "Invalid integer '1.5'");
    }

    #[test]
    fn vec3_centers_integers() {
        let c = parse_vec3(&mut StringReader::new("1 2 3.0"), true).unwrap();
        assert_eq!(
            c,
            Coordinates::World([
                WorldCoordinate::absolute(1.5),
                WorldCoordinate::absolute(2.0),
                WorldCoordinate::absolute(3.0)
            ])
        );
    }

    #[test]
    fn pairs() {
        let p = parse_pair(&mut StringReader::new("~1 -2"), true, false).unwrap();
        assert_eq!(
            p,
            [
                WorldCoordinate::relative(1.0),
                WorldCoordinate::absolute(-2.0)
            ]
        );
        let e = parse_pair(&mut StringReader::new("5"), false, false).unwrap_err();
        assert_eq!(e.message(), "Incomplete (expected 2 coordinates)");
    }

    #[test]
    fn coordinates_resolve_against_position_and_rotation() {
        let origin = [10.0, 64.0, -5.0];
        let world = Coordinates::World([
            WorldCoordinate::absolute(1.5),
            WorldCoordinate::relative(2.0),
            WorldCoordinate::absolute(-3.0),
        ]);
        assert_eq!(world.resolve(origin, 0.0, 0.0), [1.5, 66.0, -3.0]);

        let local = Coordinates::Local {
            left: 1.0,
            up: 2.0,
            forwards: 3.0,
        };
        assert_eq!(local.resolve([0.0; 3], 0.0, 0.0), [1.0, 2.0, 3.0]);
        let close = |a: [f64; 3], b: [f64; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9);
        assert!(close(local.resolve([0.0; 3], 90.0, 0.0), [-3.0, 2.0, -1.0]));
        assert!(close(local.resolve([0.0; 3], 0.0, 90.0), [1.0, -3.0, -2.0]));

        let rotation = [
            WorldCoordinate::relative(90.0),
            WorldCoordinate::absolute(-10.0),
        ];
        assert_eq!(resolve_rotation(rotation, 45.0, 5.0), (135.0, -10.0));
    }
}
