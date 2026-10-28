//! Exact light properties extracted from vanilla block states.
use ferrite_data::block_props;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightProperties {
    pub emission: u8,
    pub opacity: u8,
    /// Shape IDs in west, east, down, up, north, south order.
    pub faces: [u16; 6],
}
impl LightProperties {
    pub const AIR: Self = Self {
        emission: 0,
        opacity: 0,
        faces: [0; 6],
    };
    pub const OPAQUE: Self = Self {
        emission: 0,
        opacity: 15,
        faces: [1; 6],
    };
}
pub fn vanilla_properties(id: u16) -> LightProperties {
    if usize::from(id) >= block_props::STATE_COUNT {
        return LightProperties::OPAQUE;
    }
    let p = block_props::get(id);
    let mut faces = [0; 6];
    if p.use_shape_for_light_occlusion {
        for (direction, face) in faces.iter_mut().enumerate() {
            *face = block_props::face_occlusion_id(id, direction);
        }
    }
    LightProperties {
        emission: p.light_emission,
        opacity: p.light_opacity,
        faces,
    }
}

/// True when the projected union of both face shapes covers the unit square.
pub(crate) fn occludes(a: u16, b: u16, direction: usize) -> bool {
    if a == 1 || b == 1 {
        return true;
    }
    if a == 0 && b == 0 {
        return false;
    }
    let (u, v) = match direction {
        0 | 1 => (2, 1),
        2 | 3 => (0, 2),
        _ => (0, 1),
    };
    let first = block_props::shape_by_id(a);
    let second = block_props::shape_by_id(b);
    let boxes = || first.iter().chain(second.iter());
    let mut x = 0.0_f64;
    while x < 1.0 {
        let next = boxes()
            .flat_map(|r| [r[u], r[u + 3]])
            .filter(|&edge| edge > x && edge < 1.0)
            .fold(1.0_f64, f64::min);
        let midpoint = (x + next) * 0.5;
        let mut y = 0.0_f64;
        while y < 1.0 {
            let end = boxes()
                .filter(|r| r[u] <= midpoint && r[u + 3] >= midpoint && r[v] <= y && r[v + 3] > y)
                .map(|r| r[v + 3])
                .fold(y, f64::max);
            if end <= y {
                return false;
            }
            y = end;
        }
        x = next;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complementary_slab_faces_close_but_matching_faces_do_not() {
        let bottom = ferrite_data::blocks::state_id(
            "stone_slab",
            &[("type", "bottom"), ("waterlogged", "false")],
        )
        .unwrap();
        let top = ferrite_data::blocks::state_id(
            "stone_slab",
            &[("type", "top"), ("waterlogged", "false")],
        )
        .unwrap();
        let bottom = vanilla_properties(bottom).faces[0];
        let top = vanilla_properties(top).faces[1];
        assert!(occludes(bottom, top, 0));
        assert!(!occludes(bottom, bottom, 0));
    }
}
