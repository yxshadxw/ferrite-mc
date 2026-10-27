use ferrite_world::{PaletteKind, PalettedContainer};

#[test]
fn palette_grows_and_shrinks_across_transitions() {
    for kind in [PaletteKind::Blocks, PaletteKind::Biomes] {
        let mut container = PalettedContainer::new(kind, 0, 15).unwrap();
        let n = if kind == PaletteKind::Blocks { 300 } else { 16 };
        for i in 1..n {
            container.set(i, i as u32).unwrap();
        }
        for i in 0..n {
            assert_eq!(container.get(i), Some(i as u32));
        }
        assert_eq!(container.bits_per_entry(), 15);
        for i in 1..n {
            container.set(i, 0).unwrap();
        }
        assert_eq!(container.bits_per_entry(), 0);
    }
}

#[test]
fn packed_entries_do_not_span_words() {
    let mut container = PalettedContainer::new(PaletteKind::Blocks, 0, 15).unwrap();
    for i in 1..17 {
        container.set(i, i as u32).unwrap();
    }
    assert_eq!(container.bits_per_entry(), 5);
    assert_eq!(container.packed_data().len(), 342);
    for i in 0..17 {
        assert_eq!(container.get(i), Some(i as u32));
    }
}

#[test]
fn network_encoding_has_no_data_length_prefix() {
    let mut container = PalettedContainer::new(PaletteKind::Biomes, 2, 6).unwrap();
    assert_eq!(container.encode_network(), vec![0, 2]);
    container.set(0, 3).unwrap();
    let data = container.encode_network();
    assert_eq!(&data[..4], &[1, 2, 2, 3]);
    assert_eq!(data.len(), 12);
    assert_eq!(data[11], 1);
}

#[test]
fn every_palette_transition_and_compaction() {
    for (kind, global, thresholds) in [
        (
            PaletteKind::Blocks,
            15,
            vec![
                (1, 0),
                (2, 4),
                (17, 5),
                (33, 6),
                (65, 7),
                (129, 8),
                (257, 15),
            ],
        ),
        (
            PaletteKind::Biomes,
            6,
            vec![(1, 0), (2, 1), (3, 2), (5, 3), (9, 6)],
        ),
    ] {
        let mut c = PalettedContainer::new(kind, 0, global).unwrap();
        let max = thresholds.last().unwrap().0;
        for count in 1..=max {
            if count > 1 {
                c.set(count - 1, (count - 1) as u32).unwrap();
            }
            if let Some((_, bits)) = thresholds.iter().find(|(n, _)| *n == count) {
                assert_eq!(c.bits_per_entry(), *bits, "{kind:?} count {count}");
            }
        }
        for count in (1..max).rev() {
            c.set(count, 0).unwrap();
            c.compact();
            let expected = thresholds
                .iter()
                .rev()
                .find(|(n, _)| *n <= count)
                .unwrap()
                .1;
            assert_eq!(c.bits_per_entry(), expected, "{kind:?} count {count}");
        }
    }
}
