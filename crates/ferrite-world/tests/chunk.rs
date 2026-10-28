use ferrite_world::{BlockFlags, BlockInfo, Chunk, HeightmapKind};
struct TestBlocks;
impl BlockInfo for TestBlocks {
    fn flags(&self, id: u32) -> BlockFlags {
        match id {
            0 => BlockFlags {
                air: true,
                ..Default::default()
            },
            1 => BlockFlags {
                motion_blocking: true,
                ..Default::default()
            },
            2 => BlockFlags {
                fluid: true,
                ..Default::default()
            },
            3 => BlockFlags {
                motion_blocking: true,
                leaves: true,
                ..Default::default()
            },
            _ => Default::default(),
        }
    }
}
#[test]
fn chunk_heights_and_non_air_counts_update() {
    let mut c = Chunk::new(-64, 384, 0, 0, 15, 6).unwrap();
    assert_eq!(c.sections().len(), 24);
    c.set_block(0, -64, 0, 1, &TestBlocks).unwrap();
    c.set_block(0, 10, 0, 2, &TestBlocks).unwrap();
    c.set_block(0, 20, 0, 3, &TestBlocks).unwrap();
    assert_eq!(c.height(HeightmapKind::WorldSurface, 0, 0), Some(85));
    assert_eq!(
        c.height(HeightmapKind::MotionBlockingNoLeaves, 0, 0),
        Some(75)
    );
    assert_eq!(c.height(HeightmapKind::OceanFloor, 0, 0), Some(85));
    c.set_block(0, 20, 0, 0, &TestBlocks).unwrap();
    assert_eq!(c.height(HeightmapKind::OceanFloor, 0, 0), Some(1));
    assert_eq!(c.sections()[0].block_count(), 1);
    assert_eq!(c.sections()[5].block_count(), 0);
}

#[test]
fn full_chunk_network_encoding_includes_light_masks_and_data() {
    use ferrite_world::{LightData, LightSection, encode_chunk_packet};
    let c = Chunk::new(0, 16, 0, 0, 15, 6).unwrap();
    let light = LightData {
        sky: vec![
            LightSection::Empty,
            LightSection::Data(Box::new([0x21; 2048])),
            LightSection::Empty,
        ],
        block: vec![LightSection::Empty; 3],
    };
    let encoded = encode_chunk_packet(2, -3, &c, &light).unwrap();
    assert_eq!(&encoded[..8], &[0, 0, 0, 2, 255, 255, 255, 253]);
    assert!(encoded.windows(2048).any(|a| a == [0x21; 2048]));
    assert!(encode_chunk_packet(0, 0, &c, &LightData::default()).is_err());
}

#[test]
fn network_section_counts_water_and_waterlogged_blocks() {
    use ferrite_world::ChunkSection;
    struct Vanilla;
    impl BlockInfo for Vanilla {
        fn flags(&self, id: u32) -> BlockFlags {
            let p = ferrite_data::block_props::get(id as u16);
            BlockFlags {
                air: p.is_air,
                fluid: p.fluid.is_some(),
                ..Default::default()
            }
        }
    }
    let air = ferrite_data::blocks::default_state("air").unwrap() as u32;
    let water = ferrite_data::blocks::default_state("water").unwrap() as u32;
    let slab = ferrite_data::blocks::state_id(
        "stone_slab",
        &[("type", "bottom"), ("waterlogged", "true")],
    )
    .unwrap() as u32;
    let mut s = ChunkSection::new(air, 0, 16, 6).unwrap();
    s.set_block(0, water, &Vanilla).unwrap();
    s.set_block(1, slab, &Vanilla).unwrap();
    let bytes = s.encode_network();
    assert_eq!(&bytes[..4], &[0, 2, 0, 2]);
    let mut expected = vec![0, 2, 0, 2];
    expected.extend(s.blocks().encode_network());
    expected.extend(s.biomes().encode_network());
    assert_eq!(bytes, expected);
    s.set_block(0, air, &Vanilla).unwrap();
    assert_eq!(&s.encode_network()[..4], &[0, 1, 0, 1]);
}
