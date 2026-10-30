//! Evaluation of density functions that do not depend on seeded noise.

use crate::{Axis, DensityFunction, DensityNode, ResourceId, Spline, SplineValue};

/// Block coordinates used when evaluating a density function.
pub trait FunctionContext {
    fn block_x(&self) -> i32;
    fn block_y(&self) -> i32;
    fn block_z(&self) -> i32;
    fn sample_noise(&self, id: &ResourceId, _x: f64, _y: f64, _z: f64) -> f32 {
        panic!("noise sampler not available for {id}")
    }
    fn sample_blended(&self, _settings: [f64; 5], _x: i32, _y: i32, _z: i32) -> f32 {
        panic!("blended noise sampler not available")
    }
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
                    DensityNode::Noise {
                        noise,
                        xz_scale,
                        y_scale,
                        shift_x,
                        shift_z,
                    } => {
                        let x = f64::from(ctx.block_x()) * xz_scale
                            + shift_x.as_ref().map_or(0.0, |f| f64::from(eval(f)));
                        let y = f64::from(ctx.block_y()) * y_scale;
                        let z = f64::from(ctx.block_z()) * xz_scale
                            + shift_z.as_ref().map_or(0.0, |f| f64::from(eval(f)));
                        ctx.sample_noise(noise, x, y, z)
                    }
                    DensityNode::ShiftA { noise } => {
                        ctx.sample_noise(
                            noise,
                            f64::from(ctx.block_x()) * 0.25,
                            0.0,
                            f64::from(ctx.block_z()) * 0.25,
                        ) * 4.0
                    }
                    DensityNode::ShiftB { noise } => {
                        ctx.sample_noise(
                            noise,
                            f64::from(ctx.block_z()) * 0.25,
                            f64::from(ctx.block_x()) * 0.25,
                            0.0,
                        ) * 4.0
                    }
                    DensityNode::Shift { noise } => {
                        ctx.sample_noise(
                            noise,
                            f64::from(ctx.block_x()) * 0.25,
                            f64::from(ctx.block_y()) * 0.25,
                            f64::from(ctx.block_z()) * 0.25,
                        ) * 4.0
                    }
                    DensityNode::OldBlendedNoise { xz_scale, y_scale, xz_factor, y_factor, smear_scale_multiplier } => {
                        ctx.sample_blended([*xz_scale, *y_scale, *xz_factor, *y_factor, *smear_scale_multiplier], ctx.block_x(), ctx.block_y(), ctx.block_z())
                    }
                    DensityNode::Spline { spline } => spline.compute_float(ctx),
                    _ => panic!("density node requires a sampler: {node:?}"),
                }
            }
        }
    }
}

impl Spline {
    fn compute_float(&self, ctx: &impl FunctionContext) -> f32 {
        let x = self.coordinate.compute_float(ctx);
        let index = self
            .points
            .partition_point(|point| (point.location as f32) <= x);
        let value = |value: &SplineValue| match value {
            SplineValue::Constant(value) => *value as f32,
            SplineValue::Spline(spline) => spline.compute_float(ctx),
        };
        if index == 0 || index == self.points.len() {
            let point = &self.points[if index == 0 { 0 } else { index - 1 }];
            return value(&point.value) + point.derivative as f32 * (x - point.location as f32);
        }
        let left = &self.points[index - 1];
        let right = &self.points[index];
        let length = right.location as f32 - left.location as f32;
        let t = (x - left.location as f32) / length;
        let a = value(&left.value);
        let b = value(&right.value);
        let delta = b - a;
        let m = left.derivative as f32 * length - delta;
        let n = -right.derivative as f32 * length + delta;
        (a + t * (b - a)) + t * (1.0 - t) * (m + t * (n - m))
    }
}
