//! Evaluation of density functions that do not depend on seeded noise.

use crate::{Axis, DensityFunction, DensityNode};

/// Block coordinates used when evaluating a density function.
pub trait FunctionContext {
    fn block_x(&self) -> i32;
    fn block_y(&self) -> i32;
    fn block_z(&self) -> i32;
}

impl DensityFunction {
    /// Evaluates a resolved, noise-free function with 26.3 float arithmetic.
    ///
    /// # Panics
    /// Panics for unresolved references and nodes requiring a seeded sampler.
    pub fn compute(&self, ctx: &impl FunctionContext) -> f64 {
        f64::from(self.compute_float(ctx))
    }

    fn compute_float(&self, ctx: &impl FunctionContext) -> f32 {
        match self {
            Self::Constant(value) => *value as f32,
            Self::Reference(id) => panic!("unresolved density function: {id}"),
            Self::Node(node) => {
                let eval = |value: &DensityFunction| value.compute_float(ctx);
                match node {
                    DensityNode::Add { left, right } => eval(left) + eval(right),
                    DensityNode::Sub { left, right } => eval(left) - eval(right),
                    DensityNode::Mul { left, right } => eval(left) * eval(right),
                    DensityNode::Div { left, right } => eval(left) / eval(right),
                    DensityNode::Min { left, right } => eval(left).min(eval(right)),
                    DensityNode::Max { left, right } => eval(left).max(eval(right)),
                    DensityNode::Abs { input } => eval(input).abs(),
                    DensityNode::Square { input } => {
                        let x = eval(input);
                        x * x
                    }
                    DensityNode::Cube { input } => {
                        let x = eval(input);
                        x * x * x
                    }
                    DensityNode::Negate { input } => -eval(input),
                    DensityNode::HalfNegative { input } => {
                        let x = eval(input);
                        if x > 0.0 { x } else { x * 0.5 }
                    }
                    DensityNode::QuarterNegative { input } => {
                        let x = eval(input);
                        if x > 0.0 { x } else { x * 0.25 }
                    }
                    DensityNode::Squeeze { input } => {
                        let x = eval(input).clamp(-1.0, 1.0);
                        x / 2.0 - x * x * x / 24.0
                    }
                    DensityNode::Clamp { input, min, max } => {
                        eval(input).clamp(*min as f32, *max as f32)
                    }
                    DensityNode::Gradient {
                        axis,
                        from_coordinate,
                        from_value,
                        to_coordinate,
                        to_value,
                    } => {
                        let coordinate = match axis {
                            Axis::X => ctx.block_x(),
                            Axis::Y => ctx.block_y(),
                            Axis::Z => ctx.block_z(),
                        } as f32;
                        let t = ((coordinate - *from_coordinate as f32)
                            / (*to_coordinate as f32 - *from_coordinate as f32))
                            .clamp(0.0, 1.0);
                        *from_value as f32 + t * (*to_value as f32 - *from_value as f32)
                    }
                    DensityNode::Interpolated { input, .. }
                    | DensityNode::Cache { input }
                    | DensityNode::BlendDensity { input } => eval(input),
                    DensityNode::BlendAlpha => 1.0,
                    DensityNode::BlendOffset | DensityNode::Beardifier => 0.0,
                    DensityNode::RangeChoice {
                        input,
                        min_inclusive,
                        max_exclusive,
                        when_in_range,
                        when_out_of_range,
                    } => {
                        let x = eval(input);
                        if x >= *min_inclusive as f32 && x < *max_exclusive as f32 {
                            eval(when_in_range)
                        } else {
                            eval(when_out_of_range)
                        }
                    }
                    DensityNode::Lerp {
                        alpha,
                        first,
                        second,
                    } => {
                        let a = eval(alpha);
                        let x = eval(first);
                        x + a * (eval(second) - x)
                    }
                    _ => panic!("density node requires a sampler: {node:?}"),
                }
            }
        }
    }
}
