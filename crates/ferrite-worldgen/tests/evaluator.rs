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
