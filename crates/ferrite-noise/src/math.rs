//! Bit-exact reimplementation of `java.lang.StrictMath.log` (fdlibm `__ieee754_log`).
//!
//! `Math.log` in Java delegates to `StrictMath.log`, and the resulting last-bit
//! differences against a platform libm would leak into `nextGaussian()`
//! bitwise comparisons.

const LN2_HI: f64 = 6.931_471_803_691_238E-1;
const LN2_LO: f64 = 1.908_214_929_270_587_7E-10;
const TWO54: f64 = 1.801_439_850_948_198_4E16;
const LG1: f64 = 6.666_666_666_666_735E-1;
const LG2: f64 = 3.999_999_999_940_942E-1;
const LG3: f64 = 2.857_142_874_366_239E-1;
const LG4: f64 = 2.222_219_843_214_978_4E-1;
const LG5: f64 = 1.818_357_216_161_805E-1;
const LG6: f64 = 1.531_383_769_920_937_3E-1;
const LG7: f64 = 1.479_819_860_511_658_6E-1;

fn high_word(value: f64) -> u32 {
    (value.to_bits() >> 32) as u32
}

fn with_high_word(value: f64, high: u32) -> f64 {
    f64::from_bits((u64::from(high) << 32) | (value.to_bits() & 0xFFFF_FFFF))
}

pub(crate) fn log(x: f64) -> f64 {
    let mut hx = high_word(x) as i32;
    let lx = x.to_bits() as u32;
    let mut x = x;
    let mut k: i32 = 0;

    if hx < 0x0010_0000 {
        if (((hx & 0x7FFF_FFFF) as u32) | lx) == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            return f64::NAN;
        }
        k -= 54;
        x *= TWO54;
        hx = high_word(x) as i32;
    }
    if hx >= 0x7FF0_0000 {
        return x + x;
    }

    k += (hx >> 20) - 1023;
    hx &= 0x000F_FFFF;
    let i = (hx.wrapping_add(0x95F64)) & 0x100000;
    x = with_high_word(x, (hx | (i ^ 0x3FF0_0000)) as u32);
    k += i >> 20;
    let f = x - 1.0;

    if (0x000F_FFFF & (hx.wrapping_add(2))) < 3 {
        if f == 0.0 {
            if k == 0 {
                return 0.0;
            }
            let dk = f64::from(k);
            return dk * LN2_HI + dk * LN2_LO;
        }
        let r = f * f * (0.5 - (1.0 / 3.0) * f);
        if k == 0 {
            return f - r;
        }
        let dk = f64::from(k);
        return dk * LN2_HI - ((r - dk * LN2_LO) - f);
    }

    let s = f / (2.0 + f);
    let dk = f64::from(k);
    let z = s * s;
    let i = hx.wrapping_sub(0x6147A);
    let w = z * z;
    let j = 0x6B851i32.wrapping_sub(hx);
    let t1 = w * (LG2 + w * (LG4 + w * LG6));
    let t2 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
    let i = i | j;
    let r = t2 + t1;

    if i > 0 {
        let hfsq = 0.5 * f * f;
        if k == 0 {
            f - (hfsq - s * (hfsq + r))
        } else {
            dk * LN2_HI - ((hfsq - (s * (hfsq + r) + dk * LN2_LO)) - f)
        }
    } else if k == 0 {
        f - s * (f - r)
    } else {
        dk * LN2_HI - ((s * (f - r) - dk * LN2_LO) - f)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn log_matches_java_strictmath() {
        let cases: [(f64, u64); 9] = [
            (0.5, 0xbfe62e42fefa39ef),
            (0.75, 0xbfd269621134db92),
            (0.9, 0xbfbaf8e8210a415c),
            (0.25, 0xbff62e42fefa39ef),
            (0.1, 0xc0026bb1bbb55515),
            (0.3333333333333333, 0xbff193ea7aad030b),
            (0.999999, 0xbeb0c6f82d74d230),
            (1e-300, 0xc085963447f87fb5),
            (0.49975808364706703, 0xbfe63239e98afd71),
        ];
        for (x, expected) in cases {
            let got = super::log(x);
            assert_eq!(got.to_bits(), expected, "log({x:?})");
        }
    }
}
