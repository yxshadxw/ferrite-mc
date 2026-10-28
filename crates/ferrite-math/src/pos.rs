use crate::direction::Direction;
use crate::mth;
use crate::vec3::Vec3;

/// A block position, matching vanilla's `BlockPos`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// The position containing `position`, flooring each coordinate.
    pub fn containing(position: Vec3) -> Self {
        Self::new(
            mth::floor(position.x),
            mth::floor(position.y),
            mth::floor(position.z),
        )
    }

    pub const fn offset(self, dx: i32, dy: i32, dz: i32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.z + dz)
    }

    pub const fn relative(self, direction: Direction) -> Self {
        let (dx, dy, dz) = direction.step();
        self.offset(dx, dy, dz)
    }

    pub const fn relative_n(self, direction: Direction, n: i32) -> Self {
        let (dx, dy, dz) = direction.step();
        self.offset(dx * n, dy * n, dz * n)
    }

    pub const fn above(self) -> Self {
        self.offset(0, 1, 0)
    }

    pub const fn below(self) -> Self {
        self.offset(0, -1, 0)
    }

    pub const fn north(self) -> Self {
        self.offset(0, 0, -1)
    }

    pub const fn south(self) -> Self {
        self.offset(0, 0, 1)
    }

    pub const fn west(self) -> Self {
        self.offset(-1, 0, 0)
    }

    pub const fn east(self) -> Self {
        self.offset(1, 0, 0)
    }

    /// The center of the block.
    pub fn center(self) -> Vec3 {
        Vec3::new(
            f64::from(self.x) + 0.5,
            f64::from(self.y) + 0.5,
            f64::from(self.z) + 0.5,
        )
    }

    pub fn chunk_pos(self) -> ChunkPos {
        ChunkPos::from_block(self)
    }

    pub fn section_pos(self) -> SectionPos {
        SectionPos::from_block(self)
    }
}

/// A chunk column, matching vanilla's `ChunkPos`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// The chunk containing `pos`, using floor division for negative
    /// coordinates.
    pub fn from_block(pos: BlockPos) -> Self {
        Self::new(
            SectionPos::block_to_section(pos.x),
            SectionPos::block_to_section(pos.z),
        )
    }

    /// The block position at the minimum corner of this chunk.
    pub const fn min_block(self) -> BlockPos {
        BlockPos::new(
            SectionPos::section_to_block(self.x),
            0,
            SectionPos::section_to_block(self.z),
        )
    }
}

/// A 16x16x16 section position, matching vanilla's `SectionPos`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct SectionPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl SectionPos {
    /// Number of bits per axis in a section coordinate.
    pub const SECTION_BITS: u32 = 4;

    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub fn from_block(pos: BlockPos) -> Self {
        Self::new(
            Self::block_to_section(pos.x),
            Self::block_to_section(pos.y),
            Self::block_to_section(pos.z),
        )
    }

    /// Section coordinate containing a block coordinate.
    ///
    /// Rust's arithmetic shift equals floor division by 16, so `-1` maps to
    /// section `-1` and `-17` to section `-2`, as in vanilla.
    pub const fn block_to_section(coord: i32) -> i32 {
        coord >> Self::SECTION_BITS
    }

    /// The minimum block coordinate of a section coordinate.
    pub const fn section_to_block(coord: i32) -> i32 {
        coord << Self::SECTION_BITS
    }

    /// The block position at the minimum corner of this section.
    pub const fn min_block(self) -> BlockPos {
        BlockPos::new(
            Self::section_to_block(self.x),
            Self::section_to_block(self.y),
            Self::section_to_block(self.z),
        )
    }

    pub const fn chunk_pos(self) -> ChunkPos {
        ChunkPos::new(self.x, self.z)
    }
}

#[cfg(test)]
mod tests {
    use super::{BlockPos, ChunkPos, SectionPos};
    use crate::direction::Direction;
    use crate::vec3::Vec3;

    #[test]
    fn offsets_and_directions() {
        let pos = BlockPos::new(10, 64, -3);
        assert_eq!(pos.offset(1, -2, 3), BlockPos::new(11, 62, 0));
        assert_eq!(pos.relative(Direction::North), BlockPos::new(10, 64, -4));
        assert_eq!(pos.relative_n(Direction::West, 3), BlockPos::new(7, 64, -3));
        assert_eq!(pos.above().east().south(), BlockPos::new(11, 65, -2));
        assert_eq!(pos.center(), Vec3::new(10.5, 64.5, -2.5));
    }

    #[test]
    fn containing_floors_negative_coordinates() {
        assert_eq!(
            BlockPos::containing(Vec3::new(-0.1, -1.0, -16.1)),
            BlockPos::new(-1, -1, -17)
        );
        assert_eq!(
            BlockPos::containing(Vec3::new(15.999, 0.0, 16.0)),
            BlockPos::new(15, 0, 16)
        );
    }

    #[test]
    fn negative_section_conversions_floor() {
        assert_eq!(SectionPos::block_to_section(-1), -1);
        assert_eq!(SectionPos::block_to_section(-16), -1);
        assert_eq!(SectionPos::block_to_section(-17), -2);
        assert_eq!(SectionPos::block_to_section(15), 0);
        assert_eq!(SectionPos::block_to_section(16), 1);
        assert_eq!(SectionPos::section_to_block(-2), -32);
        assert_eq!(
            SectionPos::from_block(BlockPos::new(-1, -17, 16)),
            SectionPos::new(-1, -2, 1)
        );
        assert_eq!(
            SectionPos::new(-1, -1, -1).min_block(),
            BlockPos::new(-16, -16, -16)
        );
    }

    #[test]
    fn chunks_floor_negative_coordinates() {
        assert_eq!(BlockPos::new(-1, 0, -1).chunk_pos(), ChunkPos::new(-1, -1));
        assert_eq!(BlockPos::new(-16, 70, 15).chunk_pos(), ChunkPos::new(-1, 0));
        assert_eq!(BlockPos::new(-17, 0, 16).chunk_pos(), ChunkPos::new(-2, 1));
        assert_eq!(
            ChunkPos::new(-1, -2).min_block(),
            BlockPos::new(-16, 0, -32)
        );
        assert_eq!(SectionPos::new(-1, -2, 3).chunk_pos(), ChunkPos::new(-1, 3));
    }
}
