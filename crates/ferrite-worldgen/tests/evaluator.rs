use ferrite_worldgen::{DensityFunction, DensityNode, FunctionContext};

struct Pos(i32, i32, i32);
impl FunctionContext for Pos {
    fn block_x(&self) -> i32 {
        self.0
    }
    fn block_y(&self) -> i32 {
        self.1
    }
    fn block_z(&self) -> i32 {
        self.2
    }
}
fn c(x: f64) -> Box<DensityFunction> {
    Box::new(DensityFunction::Constant(x))
}
fn node(n: DensityNode) -> DensityFunction {
    DensityFunction::Node(n)
}

#[test]
fn negative_and_squeeze_nodes_match_vanilla() {
    let ctx = Pos(0, 0, 0);
    assert_eq!(
        node(DensityNode::HalfNegative { input: c(-4.0) }).compute(&ctx),
        -2.0
    );
    assert_eq!(
        node(DensityNode::QuarterNegative { input: c(-4.0) }).compute(&ctx),
        -1.0
    );
    assert_eq!(
        node(DensityNode::Squeeze { input: c(2.0) }).compute(&ctx),
        (11.0_f32 / 24.0) as f64
    );
}

#[test]
fn range_choice_is_lower_inclusive_upper_exclusive() {
    for (value, expected) in [(-1.0, 9.0), (0.0, 7.0), (1.0, 9.0)] {
        let choice = node(DensityNode::RangeChoice {
            input: c(value),
            min_inclusive: 0.0,
            max_exclusive: 1.0,
            when_in_range: c(7.0),
            when_out_of_range: c(9.0),
        });
        assert_eq!(choice.compute(&Pos(0, 0, 0)), expected);
    }
}

#[test]
fn gradient_clamps_and_uses_axis() {
    let gradient = node(DensityNode::Gradient {
        axis: ferrite_worldgen::Axis::Y,
        from_coordinate: -10.0,
        to_coordinate: 10.0,
        from_value: -1.0,
        to_value: 1.0,
    });
    assert_eq!(gradient.compute(&Pos(9, -20, 2)), -1.0);
    assert_eq!(gradient.compute(&Pos(9, 0, 2)), 0.0);
    assert_eq!(gradient.compute(&Pos(9, 20, 2)), 1.0);
}

#[test]
fn arithmetic_and_markers_evaluate_their_children() {
    let ctx = Pos(0, 0, 0);
    let cases = [
        (
            node(DensityNode::Add {
                left: c(3.0),
                right: c(-2.0),
            }),
            1.0,
        ),
        (
            node(DensityNode::Mul {
                left: c(3.0),
                right: c(-2.0),
            }),
            -6.0,
        ),
        (
            node(DensityNode::Min {
                left: c(3.0),
                right: c(-2.0),
            }),
            -2.0,
        ),
        (
            node(DensityNode::Max {
                left: c(3.0),
                right: c(-2.0),
            }),
            3.0,
        ),
        (node(DensityNode::Abs { input: c(-2.0) }), 2.0),
        (node(DensityNode::Square { input: c(-2.0) }), 4.0),
        (node(DensityNode::Cube { input: c(-2.0) }), -8.0),
        (
            node(DensityNode::Clamp {
                input: c(-2.0),
                min: -1.0,
                max: 1.0,
            }),
            -1.0,
        ),
        (node(DensityNode::Cache { input: c(2.0) }), 2.0),
        (
            node(DensityNode::Interpolated {
                input: c(2.0),
                cell_size_xz: 4,
                cell_size_y: 8,
            }),
            2.0,
        ),
        (node(DensityNode::BlendDensity { input: c(2.0) }), 2.0),
        (node(DensityNode::BlendAlpha), 1.0),
        (node(DensityNode::BlendOffset), 0.0),
    ];
    for (function, expected) in cases {
        assert_eq!(function.compute(&ctx), expected);
    }
}

#[test]
fn cubic_spline_interpolates_extrapolates_and_evaluates_nested_points() {
    use ferrite_worldgen::{Spline, SplinePoint, SplineValue};
    let coordinate = || {
        node(DensityNode::Gradient {
            axis: ferrite_worldgen::Axis::X,
            from_coordinate: 0.0,
            to_coordinate: 10.0,
            from_value: 0.0,
            to_value: 10.0,
        })
    };
    let nested = Spline {
        coordinate: DensityFunction::Constant(0.0),
        points: vec![SplinePoint {
            location: 0.0,
            derivative: 0.0,
            value: SplineValue::Constant(2.0),
        }],
    };
    let spline = node(DensityNode::Spline {
        spline: Box::new(Spline {
            coordinate: coordinate(),
            points: vec![
                SplinePoint {
                    location: 0.0,
                    derivative: 0.0,
                    value: SplineValue::Constant(0.0),
                },
                SplinePoint {
                    location: 10.0,
                    derivative: 0.0,
                    value: SplineValue::Spline(Box::new(nested)),
                },
            ],
        }),
    });
    assert_eq!(spline.compute(&Pos(0, 0, 0)), 0.0);
    assert_eq!(spline.compute(&Pos(5, 0, 0)), 1.0);
    assert_eq!(spline.compute(&Pos(10, 0, 0)), 2.0);
}

#[test]
fn seeded_noise_matches_noise_primitive_with_vanilla_positional_seed() {
    use ferrite_noise::{Noise, NormalNoise, RandomSource, XoroshiroRandomSource};
    use ferrite_worldgen::{NoiseParameters, ResourceId, SeededNoise, WorldgenData};
    let id = ResourceId::parse("minecraft:test_noise").unwrap();
    let mut data = WorldgenData::default();
    data.noises.insert(
        id.clone(),
        NoiseParameters {
            base_octave: -8,
            octave_count: Some(5),
            base_amplitude: 1.0,
            amplitude_modifiers: Some(vec![1.0, 0.5, 1.0, 0.0, 1.0]),
        },
    );
    let seeded = SeededNoise::new(&data, 3456);
    let mut random = XoroshiroRandomSource::new(3456);
    let factory = random.fork_positional();
    let mut instance = factory.from_hash_of("minecraft:test_noise");
    let normal = NormalNoise::builder()
        .set_base_octave(-8)
        .set_octave_count(5)
        .set_amplitude_modifier(1, 0.5)
        .set_amplitude_modifier(3, 0.0)
        .build()
        .create(&mut *instance);
    let f = node(DensityNode::Noise {
        noise: id,
        xz_scale: 1.0,
        y_scale: 1.0,
        shift_x: None,
        shift_z: None,
    });
    assert_eq!(
        f.compute(&seeded.at(12, -34, 56)),
        f64::from(normal.get(12.0, -34.0, 56.0))
    );
}

#[test]
fn noise_node_matches_vanilla_reference_vector() {
    use ferrite_noise::{Noise, NoiseStack, NormalNoise, XoroshiroRandomSource};
    use ferrite_worldgen::ResourceId;
    struct KnownNoise(NoiseStack);
    impl FunctionContext for KnownNoise {
        fn block_x(&self) -> i32 {
            0
        }
        fn block_y(&self) -> i32 {
            -103
        }
        fn block_z(&self) -> i32 {
            0
        }
        fn sample_noise(&self, _: &ResourceId, x: f64, y: f64, z: f64) -> f32 {
            self.0.get(x, y, z)
        }
    }
    let mut rng = XoroshiroRandomSource::new(3456);
    let normal = NormalNoise::builder()
        .set_base_octave(-8)
        .set_octave_count(5)
        .set_amplitude_modifier(1, 0.5)
        .set_amplitude_modifier(3, 0.0)
        .build()
        .create(&mut rng);
    let function = node(DensityNode::Noise {
        noise: ResourceId::parse("test").unwrap(),
        xz_scale: 1.0,
        y_scale: 0.25,
        shift_x: Some(c(-639.6875)),
        shift_z: Some(c(-12.0)),
    });
    assert_eq!(
        function.compute(&KnownNoise(normal)).to_bits(),
        0x3fc3460920000000
    );
}

#[test]
fn shift_node_scales_all_three_coordinates() {
    use ferrite_worldgen::ResourceId;
    struct CoordinateNoise;
    impl FunctionContext for CoordinateNoise {
        fn block_x(&self) -> i32 {
            4
        }
        fn block_y(&self) -> i32 {
            8
        }
        fn block_z(&self) -> i32 {
            12
        }
        fn sample_noise(&self, _: &ResourceId, x: f64, y: f64, z: f64) -> f32 {
            (x + 10.0 * y + 100.0 * z) as f32
        }
    }
    let function = node(DensityNode::Shift {
        noise: ResourceId::parse("test").unwrap(),
    });
    assert_eq!(function.compute(&CoordinateNoise), 1284.0);
}

#[test]
fn interval_select_uses_next_interval_on_threshold() {
    for (input, expected) in [(-1.0, 10.0), (0.0, 20.0), (1.0, 30.0)] {
        let function = node(DensityNode::IntervalSelect {
            input: c(input),
            thresholds: vec![0.0, 1.0],
            functions: vec![*c(10.0), *c(20.0), *c(30.0)],
        });
        assert_eq!(function.compute(&Pos(0, 0, 0)), expected);
    }
}

#[test]
fn find_top_surface_steps_down_from_aligned_upper_bound() {
    let function = node(DensityNode::FindTopSurface {
        density: Box::new(node(DensityNode::Gradient {
            axis: ferrite_worldgen::Axis::Y,
            from_coordinate: 0.0,
            to_coordinate: 16.0,
            from_value: 1.0,
            to_value: -1.0,
        })),
        upper_bound: c(19.0),
        lower_bound: -16,
        cell_height: 4,
    });
    assert_eq!(function.compute(&Pos(0, 0, 0)), 4.0);
}

#[test]
fn interpolation_samples_cell_corners_for_negative_coordinates() {
    let x = node(DensityNode::Gradient {
        axis: ferrite_worldgen::Axis::X,
        from_coordinate: -8.0,
        to_coordinate: 8.0,
        from_value: -8.0,
        to_value: 8.0,
    });
    let function = node(DensityNode::Interpolated {
        input: Box::new(node(DensityNode::Square { input: Box::new(x) })),
        cell_size_xz: 4,
        cell_size_y: 8,
    });
    assert_eq!(function.compute(&Pos(-2, 0, 0)), 8.0);
}

#[test]
fn lerp_at_endpoints_returns_exact_selected_value() {
    let function = node(DensityNode::Lerp {
        alpha: c(1.0),
        first: c(1.0),
        second: c(1.0e-8),
    });
    assert_eq!(function.compute(&Pos(0, 0, 0)), f64::from(1.0e-8_f32));
}

#[test]
fn gradient_uses_vanillas_precomputed_float_slope() {
    let gradient = node(DensityNode::Gradient {
        axis: ferrite_worldgen::Axis::Y,
        from_coordinate: 0.0,
        to_coordinate: 24.0,
        from_value: 0.0,
        to_value: 1.0,
    });
    let expected = 7.0_f32 * (1.0_f32 / 24.0_f32);
    assert_eq!(gradient.compute(&Pos(0, 7, 0)), f64::from(expected));
}
