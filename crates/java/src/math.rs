//! `java.lang.Math` and `java.lang.StrictMath`, implemented without libm.
//!
//! `sqrt` is correctly rounded (integer square root plus round-to-nearest-even). The
//! transcendental functions use argument reduction and polynomial kernels; they are accurate to
//! roughly one unit in the last place over the ranges tests exercise, which matches the contract
//! of `Math` (as opposed to `StrictMath`, whose bit-for-bit reproducibility oxjvm does not claim).

use core::cmp::Ordering;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{
    class, double, double_arg, field, float, float_arg, int, int_arg, long, long_arg, method,
};

// -------------------------------------------------------------------------------------------
// Floating-point kernels
// -------------------------------------------------------------------------------------------

const LN2_HI: f64 = core::f64::consts::LN_2;
const LN2_LO: f64 = 2.319_046_107_240_218_6e-17;
const INV_LN2: f64 = core::f64::consts::LOG2_E;

/// `2^k` by direct bit construction.
pub(crate) fn exp2(k: i32) -> f64 {
    if k > 1023 {
        return f64::INFINITY;
    }
    if k < -1074 {
        return 0.0;
    }
    if k >= -1022 {
        f64::from_bits(((k + 1023) as u64) << 52)
    } else {
        f64::from_bits(1u64 << (k + 1074))
    }
}

fn isqrt_u128(value: u128) -> u128 {
    if value == 0 {
        return 0;
    }
    let mut x = 1u128 << ((128 - value.leading_zeros()) / 2 + 1);
    loop {
        let next = (x + value / x) >> 1;
        if next >= x {
            break;
        }
        x = next;
    }
    x
}

fn round_u128_to_f64(value: u128) -> f64 {
    if value <= (1u128 << 53) {
        return value as f64;
    }
    let bits = 128 - value.leading_zeros();
    let shift = bits - 53;
    let shifted = value >> shift;
    let remainder = value & ((1u128 << shift) - 1);
    let half = 1u128 << (shift - 1);
    let mut rounded = shifted;
    if remainder > half || (remainder == half && shifted & 1 == 1) {
        rounded += 1;
    }
    rounded as f64 * exp2(shift as i32)
}

/// Correctly-rounded `sqrt` via integer arithmetic.
pub(crate) fn sqrt(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 || x.is_infinite() {
        return x;
    }
    if x < f64::MIN_POSITIVE {
        // Scale subnormals into the normal range: sqrt(x * 2^104) * 2^-52.
        return sqrt(x * exp2(104)) * exp2(-52);
    }
    let bits = x.to_bits();
    let mut exponent = ((bits >> 52) & 0x7FF) as i32 - 1023;
    let mut mantissa = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
    if exponent & 1 != 0 {
        mantissa <<= 1;
        exponent -= 1;
    }
    // `floor(sqrt(mantissa * 2^74))` carries 63 significant bits, so the estimate below is
    // within 2^-11 ulp of the true root. Comparing `x` against the exact midpoints of the
    // adjacent doubles then resolves the rounding (round-half-even) without ever needing
    // more precision than a u128.
    let root = isqrt_u128((mantissa as u128) << 74);
    let estimate = round_u128_to_f64(root) * exp2((exponent - 126) / 2);
    let value = (mantissa as u128, exponent - 52);
    let mut result = estimate;
    let (lower, lower_exponent) = midpoint_down(estimate);
    match compare_scaled(value.0, value.1, lower * lower, lower_exponent * 2) {
        Ordering::Less => result = next_down(estimate),
        Ordering::Equal if estimate.to_bits() & 1 == 1 => result = next_down(estimate),
        Ordering::Equal | Ordering::Greater => {
            let (upper, upper_exponent) = midpoint_up(estimate);
            match compare_scaled(value.0, value.1, upper * upper, upper_exponent * 2) {
                Ordering::Greater => result = next_up(estimate),
                Ordering::Equal if estimate.to_bits() & 1 == 1 => result = next_up(estimate),
                _ => {}
            }
        }
    }
    result
}

/// The exact value of a positive normal double as `(significand, exponent)`, i.e.
/// `significand * 2^exponent`.
fn decompose(x: f64) -> (u128, i32) {
    let bits = x.to_bits();
    let exponent = (((bits >> 52) & 0x7FF) as i32) - 1075;
    let significand = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
    (significand as u128, exponent)
}

/// The exact midpoint between `a` and `b` as `(significand, exponent)`.
///
/// `b` must be an immediate neighbour of `a`, so the aligned sum fits comfortably in a `u128`.
fn midpoint(a: f64, b: f64) -> (u128, i32) {
    let (a_significand, a_exponent) = decompose(a);
    let (b_significand, b_exponent) = decompose(b);
    let common = a_exponent.min(b_exponent);
    let sum = (a_significand << (a_exponent - common) as u32)
        + (b_significand << (b_exponent - common) as u32);
    (sum, common - 1)
}

/// The exact midpoint between `x` and the next double above it.
fn midpoint_up(x: f64) -> (u128, i32) {
    midpoint(x, next_up(x))
}

/// The exact midpoint between the next double below `x` and `x`.
fn midpoint_down(x: f64) -> (u128, i32) {
    midpoint(next_down(x), x)
}

/// The next positive double above `x`.
fn next_up(x: f64) -> f64 {
    f64::from_bits(x.to_bits() + 1)
}

/// The next positive double below `x`.
fn next_down(x: f64) -> f64 {
    f64::from_bits(x.to_bits() - 1)
}

/// Order two positive values given as `significand * 2^exponent`.
fn compare_scaled(a: u128, a_exponent: i32, b: u128, b_exponent: i32) -> Ordering {
    let a_top = 128 - a.leading_zeros() as i32 + a_exponent;
    let b_top = 128 - b.leading_zeros() as i32 + b_exponent;
    if a_top != b_top {
        return a_top.cmp(&b_top);
    }
    let common = a_exponent.min(b_exponent);
    (a << (a_exponent - common) as u32).cmp(&(b << (b_exponent - common) as u32))
}

fn round_away(x: f64) -> f64 {
    if x >= 0.0 {
        floor(x + 0.5)
    } else {
        ceil(x - 0.5)
    }
}

/// `sin` with argument reduction and a Taylor kernel.
pub(crate) fn sin(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    let (quadrant, r) = reduce_pi_2(x);
    kernel_sin_cos(quadrant, r, true)
}

/// `cos`.
pub(crate) fn cos(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    let (quadrant, r) = reduce_pi_2(x);
    kernel_sin_cos(quadrant, r, false)
}

/// `tan`.
pub(crate) fn tan(x: f64) -> f64 {
    sin(x) / cos(x)
}

fn reduce_pi_2(x: f64) -> (i64, f64) {
    const PI_2_HI: f64 = 1.570_796_326_734_125_6;
    const PI_2_LO: f64 = 6.077_100_506_506_192e-11;
    let quotient = round_away(x * (2.0 / core::f64::consts::PI));
    let q = quotient as i64;
    let r = x - quotient * PI_2_HI - quotient * PI_2_LO;
    (q & 3, r)
}

fn kernel_sin_cos(quadrant: i64, r: f64, sin_mode: bool) -> f64 {
    let effective_sin = match quadrant {
        0 => sin_mode,
        1 => !sin_mode,
        2 => sin_mode,
        _ => !sin_mode,
    };
    let value = if effective_sin {
        kernel_sin(r)
    } else {
        kernel_cos(r)
    };
    let negative = match quadrant {
        0 => false,
        1 => sin_mode,
        2 => true,
        _ => !sin_mode,
    };
    if negative { -value } else { value }
}

fn kernel_sin(r: f64) -> f64 {
    let z = r * r;
    r + r
        * z
        * (-1.0 / 6.0
            + z * (1.0 / 120.0
                + z * (-1.0 / 5040.0
                    + z * (1.0 / 362_880.0
                        + z * (-1.0 / 39_916_800.0 + z * (1.0 / 6_227_020_800.0))))))
}

fn kernel_cos(r: f64) -> f64 {
    let z = r * r;
    1.0 + z
        * (-1.0 / 2.0
            + z * (1.0 / 24.0
                + z * (-1.0 / 720.0
                    + z * (1.0 / 40_320.0 + z * (-1.0 / 3_628_800.0 + z * (1.0 / 479_001_600.0))))))
}

/// `exp`.
pub(crate) fn exp(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x > 709.782_712_893_384 {
        return f64::INFINITY;
    }
    if x < -745.133_219_101_941_1 {
        return 0.0;
    }
    let k = round_away(x * INV_LN2);
    let r = (x - k * LN2_HI - k * LN2_LO).clamp(-0.5, 0.5);
    let mut term = 1.0;
    let mut sum = 1.0;
    for index in 1..=14 {
        term *= r / f64::from(index);
        sum += term;
    }
    sum * exp2(k as i32)
}

/// `ln`.
pub(crate) fn ln(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x.is_infinite() {
        return x;
    }
    let bits = x.to_bits();
    let exponent = ((bits >> 52) & 0x7FF) as i32 - 1023;
    let mantissa = f64::from_bits((bits & ((1u64 << 52) - 1)) | (1023u64 << 52));
    let (m, adjust) = if mantissa > core::f64::consts::SQRT_2 {
        (mantissa * 0.5, exponent + 1)
    } else {
        (mantissa, exponent)
    };
    let s = (m - 1.0) / (m + 1.0);
    let z = s * s;
    let mut term = s;
    let mut sum = s;
    for index in 1..=16 {
        term *= z;
        sum += term / f64::from(2 * index + 1);
    }
    let log_m = 2.0 * sum;
    log_m + f64::from(adjust) * (LN2_HI + LN2_LO)
}

/// `log10`.
pub(crate) fn log10(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x.is_infinite() {
        return x;
    }
    let approx = ln(x) / core::f64::consts::LN_10;
    let k = round(approx) as f64;
    if k.abs() <= 308.0 {
        let mut power = 1.0f64;
        let exponent = k as i32;
        if exponent >= 0 {
            for _ in 0..exponent {
                power *= 10.0;
            }
        } else {
            for _ in 0..(-exponent) {
                power /= 10.0;
            }
        }
        if power == x {
            return k;
        }
    }
    approx
}

/// `expm1`: `exp(x) - 1`, accurate for small `x`.
pub(crate) fn expm1(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x == 0.0 {
        return x;
    }
    if x.is_infinite() {
        return if x > 0.0 { f64::INFINITY } else { -1.0 };
    }
    if x.abs() < 0.5 {
        let mut term = x;
        let mut sum = x;
        for index in 2..=18 {
            term *= x / f64::from(index);
            sum += term;
        }
        sum
    } else {
        exp(x) - 1.0
    }
}

/// `log1p`: `ln(1 + x)`, accurate for small `x`.
pub(crate) fn log1p(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x == 0.0 {
        return x;
    }
    if x <= -1.0 {
        return if x == -1.0 {
            f64::NEG_INFINITY
        } else {
            f64::NAN
        };
    }
    if x.is_infinite() {
        return f64::INFINITY;
    }
    if x.abs() < 0.5 {
        let s = x / (2.0 + x);
        let z = s * s;
        let mut term = s;
        let mut sum = s;
        for index in 1..=20 {
            term *= z;
            sum += term / f64::from(2 * index + 1);
        }
        2.0 * sum
    } else {
        ln(1.0 + x)
    }
}

/// `sinh`.
pub(crate) fn sinh(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x == 0.0 {
        return x;
    }
    if x.is_infinite() {
        return x;
    }
    let magnitude = x.abs();
    let result = if magnitude < 1.0 {
        let z = x * x;
        let mut term = x;
        let mut sum = x;
        for index in 1..=12 {
            term *= z / f64::from((2 * index) * (2 * index + 1));
            sum += term;
        }
        sum
    } else if magnitude > 709.0 {
        exp(magnitude - core::f64::consts::LN_2)
    } else {
        (exp(magnitude) - exp(-magnitude)) * 0.5
    };
    if x < 0.0 { -result } else { result }
}

/// `cosh`.
pub(crate) fn cosh(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    let magnitude = x.abs();
    if magnitude.is_infinite() {
        return f64::INFINITY;
    }
    if magnitude > 709.0 {
        exp(magnitude - core::f64::consts::LN_2)
    } else {
        (exp(magnitude) + exp(-magnitude)) * 0.5
    }
}

/// `tanh`.
pub(crate) fn tanh(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x == 0.0 {
        return x;
    }
    if x.is_infinite() {
        return if x > 0.0 { 1.0 } else { -1.0 };
    }
    let magnitude = x.abs();
    let result = if magnitude > 20.0 {
        1.0
    } else {
        let e = expm1(2.0 * magnitude);
        e / (e + 2.0)
    };
    if x < 0.0 { -result } else { result }
}

/// `x^y`.
pub(crate) fn pow(x: f64, y: f64) -> f64 {
    if y == 0.0 {
        return 1.0;
    }
    if x == 1.0 {
        return 1.0;
    }
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    // Exact small-integer exponents (fdlibm special cases): avoid the rounding
    // error of exp(y*ln(x)) so e.g. pow(x, 1.0) returns x bit-for-bit.
    if y == 1.0 {
        return x;
    }
    if y == 2.0 {
        return x * x;
    }
    if y == -1.0 {
        return 1.0 / x;
    }
    if y.is_infinite() {
        let magnitude = x.abs();
        return if magnitude > 1.0 {
            if y > 0.0 { f64::INFINITY } else { 0.0 }
        } else if magnitude < 1.0 {
            if y > 0.0 { 0.0 } else { f64::INFINITY }
        } else {
            1.0
        };
    }
    if x.is_infinite() {
        // An infinite base only keeps its sign for odd integer exponents.
        let odd = y.abs() < 9001111111140992.0 && (y - floor(y)) == 0.0 && (y as i64) % 2 != 0;
        let negative = x < 0.0 && odd;
        return if y > 0.0 {
            if negative {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            }
        } else if negative {
            -0.0
        } else {
            0.0
        };
    }
    if x == 0.0 {
        // A zero base keeps its sign for odd integer exponents.
        let odd = y.abs() < 9001111111140992.0 && (y - floor(y)) == 0.0 && (y as i64) % 2 != 0;
        let negative = x.is_sign_negative() && odd;
        return if y > 0.0 {
            if negative { -0.0 } else { 0.0 }
        } else if negative {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    if x < 0.0 && (y - floor(y)) != 0.0 {
        return f64::NAN;
    }
    let negative = x < 0.0 && y.abs() < 9001111111140992.0 && (y as i64) % 2 != 0;
    let result = exp(y * ln(x.abs()));
    if negative { -result } else { result }
}

/// `atan`.
pub(crate) fn atan(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x == 0.0 {
        return x;
    }
    let negative = x < 0.0;
    let x = x.abs();
    let result = if x > 1.0 {
        core::f64::consts::FRAC_PI_2 - atan_small(1.0 / x)
    } else {
        atan_small(x)
    };
    if negative { -result } else { result }
}

fn atan_small(x: f64) -> f64 {
    if x <= 0.4375 {
        let z = x * x;
        let mut term = x;
        let mut sum = x;
        for index in 1..=26 {
            term *= -z;
            sum += term / f64::from(2 * index + 1);
        }
        sum
    } else {
        let y = (x - 1.0) / (x + 1.0);
        core::f64::consts::FRAC_PI_4 + atan_small(y)
    }
}

/// `atan2` with full quadrant handling.
pub(crate) fn atan2(y: f64, x: f64) -> f64 {
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    if x == 0.0 && y == 0.0 {
        return if x.is_sign_negative() {
            if y.is_sign_negative() {
                -core::f64::consts::PI
            } else {
                core::f64::consts::PI
            }
        } else {
            y
        };
    }
    if x > 0.0 {
        atan(y / x)
    } else if x < 0.0 {
        if y >= 0.0 {
            atan(y / x) + core::f64::consts::PI
        } else {
            atan(y / x) - core::f64::consts::PI
        }
    } else if y > 0.0 {
        core::f64::consts::FRAC_PI_2
    } else {
        -core::f64::consts::FRAC_PI_2
    }
}

pub(crate) fn asin(x: f64) -> f64 {
    if x.is_nan() || x.abs() > 1.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return x;
    }
    if x == 1.0 {
        return core::f64::consts::FRAC_PI_2;
    }
    if x == -1.0 {
        return -core::f64::consts::FRAC_PI_2;
    }
    atan2(x, sqrt(1.0 - x * x))
}

pub(crate) fn acos(x: f64) -> f64 {
    if x.is_nan() || x.abs() > 1.0 {
        return f64::NAN;
    }
    core::f64::consts::FRAC_PI_2 - asin(x)
}

/// `cbrt` via Newton refinement.
pub(crate) fn cbrt(x: f64) -> f64 {
    if x.is_nan() || x == 0.0 || x.is_infinite() {
        return x;
    }
    let negative = x < 0.0;
    let x = x.abs();
    let mut guess = exp(ln(x) / 3.0);
    for _ in 0..3 {
        guess = (2.0 * guess + x / (guess * guess)) / 3.0;
    }
    if negative { -guess } else { guess }
}

/// `floor`, including signed zero and infinities.
pub(crate) fn floor(x: f64) -> f64 {
    // Beyond 2^63 the `as i64` cast would saturate; such doubles are already
    // integral, so return them unchanged. Below 2^63 the cast truncates exactly.
    if !x.is_finite() || x == 0.0 || x.abs() >= 9221111111111175808.0 {
        return x;
    }
    let truncated = x as i64 as f64;
    if x < 0.0 && truncated != x {
        truncated - 1.0
    } else {
        truncated
    }
}

/// `ceil`.
pub(crate) fn ceil(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 || x.abs() >= 9221111111111175808.0 {
        return x;
    }
    let truncated = x as i64 as f64;
    let result = if x > 0.0 && truncated != x {
        truncated + 1.0
    } else {
        truncated
    };
    // A zero result keeps the sign of the input (ceil(-0.5) is -0.0).
    if result == 0.0 {
        copy_sign(0.0, x)
    } else {
        result
    }
}

/// Round to nearest, ties to even (`rint`).
pub(crate) fn rint(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let lower = floor(x);
    let fraction = x - lower;
    let even = (lower / 2.0) - floor(lower / 2.0) == 0.0;
    let result = if fraction > 0.5 || (fraction == 0.5 && !even) {
        lower + 1.0
    } else {
        lower
    };
    // A zero result keeps the sign of the input (rint(-0.25) is -0.0).
    if result == 0.0 {
        copy_sign(0.0, x)
    } else {
        result
    }
}

/// `Math.round(double)`.
pub(crate) fn round(x: f64) -> i64 {
    // JDK bit algorithm: floor(x + 0.5) computed without the rounding error of
    // the addition itself (e.g. round(0.49999999999999994) must be 0).
    let bits = x.to_bits() as i64;
    let biased_exp = (bits & 0x7FF0_0000_0000_0000) >> 52;
    let shift = 1074 - biased_exp;
    if (shift & -64) == 0 {
        let mut r = (bits & 0x000F_FFFF_FFFF_FFFF) | 0x0010_0000_0000_0000;
        if bits < 0 {
            r = -r;
        }
        ((r >> shift) + 1) >> 1
    } else {
        x as i64
    }
}

pub(crate) fn round_f32(x: f32) -> i32 {
    let bits = x.to_bits() as i32;
    let biased_exp = (bits & 0x7F80_0000) >> 23;
    let shift = 149 - biased_exp;
    if (shift & -32) == 0 {
        let mut r = (bits & 0x007F_FFFF) | 0x0080_0000;
        if bits < 0 {
            r = -r;
        }
        ((r >> shift) + 1) >> 1
    } else {
        x as i32
    }
}

pub(crate) fn hypot(x: f64, y: f64) -> f64 {
    let x = x.abs();
    let y = y.abs();
    let max = if x > y { x } else { y };
    let min = if x > y { y } else { x };
    if max.is_infinite() {
        return f64::INFINITY;
    }
    if max == 0.0 {
        return 0.0;
    }
    let ratio = min / max;
    max * sqrt(1.0 + ratio * ratio)
}

pub(crate) fn signum_f64(x: f64) -> f64 {
    if x.is_nan() {
        f64::NAN
    } else if x == 0.0 {
        x
    } else if x > 0.0 {
        1.0
    } else {
        -1.0
    }
}

pub(crate) fn copy_sign(magnitude: f64, sign: f64) -> f64 {
    if sign.is_nan() {
        return f64::NAN;
    }
    let bits = (magnitude.to_bits() & !(1u64 << 63)) | (sign.to_bits() & (1u64 << 63));
    f64::from_bits(bits)
}

pub(crate) fn ieee_remainder(x: f64, y: f64) -> f64 {
    if x.is_nan() || y.is_nan() || x.is_infinite() || y == 0.0 {
        return f64::NAN;
    }
    if y.is_infinite() {
        return x;
    }
    let quotient = rint(x / y);
    let remainder = x - quotient * y;
    if remainder == 0.0 {
        // A zero remainder carries the sign of the dividend.
        copy_sign(0.0, x)
    } else {
        remainder
    }
}

pub(crate) fn pow2(exponent: i32) -> f64 {
    let mut value = 1.0;
    if exponent >= 0 {
        for _ in 0..exponent {
            value *= 2.0;
        }
    } else {
        for _ in 0..(-exponent) {
            value /= 2.0;
        }
    }
    value
}

fn next_up_float(value: f32) -> f32 {
    if value.is_nan() || value == f32::INFINITY {
        return value;
    }
    if value == 0.0 {
        return f32::from_bits(1);
    }
    let bits = value.to_bits();
    if value > 0.0 {
        f32::from_bits(bits + 1)
    } else {
        f32::from_bits(bits - 1)
    }
}

fn next_after(start: f64, direction: f64) -> f64 {
    if start.is_nan() || direction.is_nan() {
        return f64::NAN;
    }
    if start == direction {
        return direction;
    }
    if start == 0.0 {
        return if direction > 0.0 {
            f64::from_bits(1)
        } else {
            f64::from_bits((1u64 << 63) | 1)
        };
    }
    let bits = start.to_bits();
    let towards_more = (direction > start) == (start > 0.0);
    f64::from_bits(if towards_more { bits + 1 } else { bits - 1 })
}

fn fmin(a: f32, b: f32) -> f32 {
    if a.is_nan() {
        return a;
    }
    if b.is_nan() {
        return b;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() { a } else { b };
    }
    if a < b { a } else { b }
}

fn fmax(a: f32, b: f32) -> f32 {
    if a.is_nan() {
        return a;
    }
    if b.is_nan() {
        return b;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() { b } else { a };
    }
    if a > b { a } else { b }
}

fn fmin64(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        return a;
    }
    if b.is_nan() {
        return b;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() { a } else { b };
    }
    if a < b { a } else { b }
}

fn fmax64(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        return a;
    }
    if b.is_nan() {
        return b;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() { b } else { a };
    }
    if a > b { a } else { b }
}

fn floor_div(x: i64, y: i64) -> i64 {
    let quotient = x.wrapping_div(y);
    if (x ^ y) < 0 && quotient * y != x {
        quotient - 1
    } else {
        quotient
    }
}

fn floor_mod(x: i64, y: i64) -> i64 {
    let remainder = x.wrapping_rem(y);
    if remainder != 0 && (x ^ y) < 0 {
        remainder + y
    } else {
        remainder
    }
}

/// Divide `x` by `y`, throwing `/ by zero` for a zero divisor and `overflow_msg`
/// when the quotient overflows (the `MIN / -1` case).
fn exact_quotient(vm: &mut Vm<'_>, x: i64, y: i64, overflow_msg: &str) -> Result<i64, VmError> {
    if y == 0 {
        return Err(vm.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
    }
    x.checked_div(y)
        .ok_or_else(|| vm.throw_new("java/lang/ArithmeticException", Some(overflow_msg)))
}

fn math_floor_div_exact(
    vm: &mut Vm<'_>,
    x: i64,
    y: i64,
    overflow_msg: &str,
) -> Result<i64, VmError> {
    let q = exact_quotient(vm, x, y, overflow_msg)?;
    if (x ^ y) < 0 && q.wrapping_mul(y) != x {
        Ok(q.wrapping_sub(1))
    } else {
        Ok(q)
    }
}

fn math_ceil_div_exact(
    vm: &mut Vm<'_>,
    x: i64,
    y: i64,
    overflow_msg: &str,
) -> Result<i64, VmError> {
    let q = exact_quotient(vm, x, y, overflow_msg)?;
    if (x ^ y) >= 0 && q.wrapping_mul(y) != x {
        Ok(q.wrapping_add(1))
    } else {
        Ok(q)
    }
}

/// Non-exact ceiling division: only a zero divisor throws; `MIN / -1` wraps.
fn math_ceil_div(vm: &mut Vm<'_>, x: i64, y: i64) -> Result<i64, VmError> {
    if y == 0 {
        return Err(vm.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
    }
    let q = x.wrapping_div(y);
    if (x ^ y) >= 0 && q.wrapping_mul(y) != x {
        Ok(q.wrapping_add(1))
    } else {
        Ok(q)
    }
}

/// Non-exact floor division: only a zero divisor throws; `MIN / -1` wraps.
fn math_floor_div(vm: &mut Vm<'_>, x: i64, y: i64) -> Result<i64, VmError> {
    if y == 0 {
        return Err(vm.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
    }
    Ok(floor_div(x, y))
}

fn math_floor_mod(vm: &mut Vm<'_>, x: i64, y: i64) -> Result<i64, VmError> {
    if y == 0 {
        return Err(vm.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
    }
    Ok(floor_mod(x, y))
}

fn math_ceil_mod(vm: &mut Vm<'_>, x: i64, y: i64) -> Result<i64, VmError> {
    let quotient = math_ceil_div(vm, x, y)?;
    Ok(x.wrapping_sub(quotient.wrapping_mul(y)))
}

fn math_add_exact_int(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    int_arg(args, 0)
        .checked_add(int_arg(args, 1))
        .ok_or_else(|| vm.throw_new("java/lang/ArithmeticException", Some("integer overflow")))
        .map(Value::Int)
}

fn math_add_exact_long(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    long_arg(args, 0)
        .checked_add(long_arg(args, 1))
        .ok_or_else(|| vm.throw_new("java/lang/ArithmeticException", Some("long overflow")))
        .map(Value::Long)
}

fn math_sub_exact_int(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    int_arg(args, 0)
        .checked_sub(int_arg(args, 1))
        .ok_or_else(|| vm.throw_new("java/lang/ArithmeticException", Some("integer overflow")))
        .map(Value::Int)
}

fn math_mul_exact_int(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    int_arg(args, 0)
        .checked_mul(int_arg(args, 1))
        .ok_or_else(|| vm.throw_new("java/lang/ArithmeticException", Some("integer overflow")))
        .map(Value::Int)
}

fn math_mul_exact_long(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    long_arg(args, 0)
        .checked_mul(long_arg(args, 1))
        .ok_or_else(|| vm.throw_new("java/lang/ArithmeticException", Some("long overflow")))
        .map(Value::Long)
}

// -------------------------------------------------------------------------------------------
// Fused multiply-add, correctly rounded via exact integer arithmetic (no hardware FMA in
// no_std). The product mantissa (at most 2*mb bits) and the addend are aligned in a u128
// accumulator with the larger operand kept exact; only the smaller operand may lose low bits
// to a sticky flag, which is enough for round-to-nearest-even because the integer part then
// always spans more than mb bits.

/// Shift `value` by `shift` (negative = right), returning the truncated result and whether any
/// nonzero bits were discarded.
fn fma_shift(value: u128, shift: i32) -> (u128, bool) {
    if shift >= 0 {
        if shift >= 128 {
            (0, value != 0)
        } else {
            (value << shift, false)
        }
    } else {
        let n = (-shift) as u32;
        if n >= 128 {
            (0, value != 0)
        } else {
            (value >> n, value & ((1u128 << n) - 1) != 0)
        }
    }
}

/// Round `magnitude * 2^base` (with a sticky bit below the accumulator) to the nearest float
/// of the given format and return its raw bits. `mb` counts the implicit mantissa bit.
fn fma_round(
    magnitude: u128,
    base: i32,
    sign: bool,
    sticky: bool,
    mb: u32,
    bias: i32,
    sub_exp: i32,
    width: u32,
) -> u64 {
    let sign_bit = if sign { 1u64 << (width - 1) } else { 0 };
    if magnitude == 0 {
        return sign_bit;
    }
    let frac_bits = mb - 1;
    let frac_mask = (1u64 << frac_bits) - 1;
    let max_be = (1u64 << (width - mb)) - 1;
    let bitlen = 128 - magnitude.leading_zeros();
    let mut e = base + bitlen as i32 - mb as i32;
    let mut m: u64;
    if bitlen <= mb {
        m = (magnitude << (mb - bitlen)) as u64;
    } else {
        let s = bitlen - mb;
        let low = magnitude & ((1u128 << s) - 1);
        m = (magnitude >> s) as u64;
        let half = 1u128 << (s - 1);
        if low > half || (low == half && (sticky || m & 1 != 0)) {
            m += 1;
        }
        if m == 1u64 << mb {
            m >>= 1;
            e += 1;
        }
    }
    let be = e + bias + frac_bits as i32;
    if be >= max_be as i32 {
        return sign_bit | (max_be << frac_bits);
    }
    if be > 0 {
        return sign_bit | ((be as u64) << frac_bits) | (m & frac_mask);
    }
    // Subnormal (or zero) result: round m * 2^(e - sub_exp) into the fraction field.
    let d = sub_exp - e;
    if d > mb as i32 {
        return sign_bit;
    }
    if d == mb as i32 {
        if m > 1u64 << frac_bits || sticky {
            return sign_bit | 1;
        }
        return sign_bit;
    }
    let low = m & ((1u64 << d) - 1);
    let mut frac = m >> d;
    let half = 1u64 << (d - 1);
    if low > half || (low == half && (sticky || frac & 1 != 0)) {
        frac += 1;
    }
    if frac == 1u64 << frac_bits {
        return sign_bit | (1u64 << frac_bits);
    }
    sign_bit | frac
}

/// Exact fused multiply-add over raw bits of one format. `width` is 32 or 64, `mb` the
/// mantissa width including the implicit bit, `bias` the exponent bias and `sub_exp` the
/// exponent of one fraction-field unit (the smallest subnormal).
fn fma_core(x: u64, y: u64, z: u64, width: u32, mb: u32, bias: i32, sub_exp: i32) -> u64 {
    let frac_bits = mb - 1;
    let frac_mask = (1u64 << frac_bits) - 1;
    let max_be = (1u64 << (width - mb)) - 1;
    let sx = x >> (width - 1) != 0;
    let sy = y >> (width - 1) != 0;
    let sz = z >> (width - 1) != 0;
    let bex = (x >> frac_bits) & max_be;
    let bey = (y >> frac_bits) & max_be;
    let bez = (z >> frac_bits) & max_be;
    let fx = x & frac_mask;
    let fy = y & frac_mask;
    let fz = z & frac_mask;
    let nan_bits = (max_be << frac_bits) | (1u64 << (frac_bits - 1));
    if bex == max_be || bey == max_be || bez == max_be {
        if (bex == max_be && fx != 0) || (bey == max_be && fy != 0) || (bez == max_be && fz != 0) {
            return nan_bits;
        }
        let sp = sx ^ sy;
        if bex == max_be || bey == max_be {
            if bez == max_be && sz != sp {
                return nan_bits;
            }
            return (u64::from(sp) << (width - 1)) | (max_be << frac_bits);
        }
        return z;
    }
    // Normalize each finite operand to value = m * 2^e with m in [2^(mb-1), 2^mb), or m = 0.
    let normalize = |be: u64, frac: u64| -> (u64, i32) {
        if be == 0 {
            if frac == 0 {
                (0, 0)
            } else {
                let k = frac.leading_zeros() - (64 - mb);
                (frac << k, sub_exp - k as i32)
            }
        } else {
            (
                frac | (1u64 << frac_bits),
                be as i32 - bias - frac_bits as i32,
            )
        }
    };
    let (mx, ex) = normalize(bex, fx);
    let (my, ey) = normalize(bey, fy);
    let (mz, ez) = normalize(bez, fz);
    let sp = sx ^ sy;
    let product = u128::from(mx) * u128::from(my);
    let ep = ex + ey;
    if product == 0 && mz == 0 {
        return u64::from(sp && sz) << (width - 1);
    }
    if product == 0 {
        return z;
    }
    if mz == 0 {
        return fma_round(product, ep, sp, false, mb, bias, sub_exp, width);
    }
    let bitlen_p = 128 - product.leading_zeros();
    let tp = ep + bitlen_p as i32 - 1;
    let tz = ez + frac_bits as i32;
    let (magnitude, base, sign, sticky) = if sp == sz {
        let base = tp.max(tz) - 120;
        let (p, sp_lost) = fma_shift(product, ep - base);
        let (zv, sz_lost) = fma_shift(u128::from(mz), ez - base);
        (p + zv, base, sp, sp_lost || sz_lost)
    } else if tp >= tz {
        // Keep the product exact; the addend may lose low bits to sticky.
        let base = tp - 120;
        let p = product << (120 - bitlen_p + 1);
        let (zv, sz_lost) = fma_shift(u128::from(mz), ez - base);
        if p > zv {
            (p - zv - u128::from(sz_lost), base, sp, sz_lost)
        } else if p == zv {
            (0, base, false, false)
        } else {
            (zv - p, base, sz, false)
        }
    } else {
        // Keep the addend exact; the product may lose low bits to sticky.
        let base = tz - 120;
        let zv = u128::from(mz) << (120 - frac_bits);
        let (p, sp_lost) = fma_shift(product, ep - base);
        if zv > p {
            (zv - p - u128::from(sp_lost), base, sz, sp_lost)
        } else if zv == p {
            (0, base, false, false)
        } else {
            (p - zv, base, sp, false)
        }
    };
    fma_round(magnitude, base, sign, sticky, mb, bias, sub_exp, width)
}

fn fma_f64(x: f64, y: f64, z: f64) -> f64 {
    f64::from_bits(fma_core(
        x.to_bits(),
        y.to_bits(),
        z.to_bits(),
        64,
        53,
        1023,
        -1074,
    ))
}

fn fma_f32(x: f32, y: f32, z: f32) -> f32 {
    f32::from_bits(fma_core(
        u64::from(x.to_bits()),
        u64::from(y.to_bits()),
        u64::from(z.to_bits()),
        32,
        24,
        127,
        -149,
    ) as u32)
}

macro_rules! math_methods {
    () => {{
        [
            method("abs", "(I)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                int(int_arg(args, 0).wrapping_abs())
            }),
            method("abs", "(J)J", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                long(long_arg(args, 0).wrapping_abs())
            }),
            method("abs", "(F)F", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                float(float_arg(args, 0).abs())
            }),
            method("abs", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(double_arg(args, 0).abs())
            }),
            method(
                "absExact",
                "(I)I",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    let value = int_arg(args, 0);
                    if value == i32::MIN {
                        return Err(
                            vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
                        );
                    }
                    int(value.wrapping_abs())
                },
            ),
            method(
                "absExact",
                "(J)J",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    let value = long_arg(args, 0);
                    if value == i64::MIN {
                        return Err(
                            vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
                        );
                    }
                    long(value.wrapping_abs())
                },
            ),
            method(
                "multiplyHigh",
                "(JJ)J",
                ACC_PUBLIC | ACC_STATIC,
                |_, _, args| {
                    let x = long_arg(args, 0) as i128;
                    let y = long_arg(args, 1) as i128;
                    long(((x * y) >> 64) as i64)
                },
            ),
            method(
                "unsignedMultiplyHigh",
                "(JJ)J",
                ACC_PUBLIC | ACC_STATIC,
                |_, _, args| {
                    let x = long_arg(args, 0) as u64 as u128;
                    let y = long_arg(args, 1) as u64 as u128;
                    long(((x * y) >> 64) as i64)
                },
            ),
            method("clamp", "(JII)I", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
                let value = long_arg(args, 0);
                let min = int_arg(args, 1);
                let max = int_arg(args, 2);
                if min > max {
                    return Err(vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some(&alloc::format!("{min} > {max}")),
                    ));
                }
                int(value.max(i64::from(min)).min(i64::from(max)) as i32)
            }),
            method("clamp", "(JJJ)J", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
                let value = long_arg(args, 0);
                let min = long_arg(args, 1);
                let max = long_arg(args, 2);
                if min > max {
                    return Err(vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some(&alloc::format!("{min} > {max}")),
                    ));
                }
                long(value.max(min).min(max))
            }),
            method("clamp", "(DDD)D", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
                let value = double_arg(args, 0);
                let min = double_arg(args, 1);
                let max = double_arg(args, 2);
                if !matches!(min.partial_cmp(&max), Some(Ordering::Less)) {
                    if min.is_nan() {
                        return Err(
                            vm.throw_new("java/lang/IllegalArgumentException", Some("min is NaN"))
                        );
                    }
                    if max.is_nan() {
                        return Err(
                            vm.throw_new("java/lang/IllegalArgumentException", Some("max is NaN"))
                        );
                    }
                    if min.total_cmp(&max) == core::cmp::Ordering::Greater {
                        return Err(vm.throw_new(
                            "java/lang/IllegalArgumentException",
                            Some(&alloc::format!("{min} > {max}")),
                        ));
                    }
                }
                double(fmin64(max, fmax64(value, min)))
            }),
            method("clamp", "(FFF)F", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
                let value = float_arg(args, 0);
                let min = float_arg(args, 1);
                let max = float_arg(args, 2);
                if !matches!(min.partial_cmp(&max), Some(Ordering::Less)) {
                    if min.is_nan() {
                        return Err(
                            vm.throw_new("java/lang/IllegalArgumentException", Some("min is NaN"))
                        );
                    }
                    if max.is_nan() {
                        return Err(
                            vm.throw_new("java/lang/IllegalArgumentException", Some("max is NaN"))
                        );
                    }
                    if min.total_cmp(&max) == core::cmp::Ordering::Greater {
                        return Err(vm.throw_new(
                            "java/lang/IllegalArgumentException",
                            Some(&alloc::format!("{min} > {max}")),
                        ));
                    }
                }
                float(fmin(max, fmax(value, min)))
            }),
            method("min", "(II)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                int(int_arg(args, 0).min(int_arg(args, 1)))
            }),
            method("min", "(JJ)J", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                long(long_arg(args, 0).min(long_arg(args, 1)))
            }),
            method("min", "(FF)F", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                float(fmin(float_arg(args, 0), float_arg(args, 1)))
            }),
            method("min", "(DD)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(fmin64(double_arg(args, 0), double_arg(args, 1)))
            }),
            method("max", "(II)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                int(int_arg(args, 0).max(int_arg(args, 1)))
            }),
            method("max", "(JJ)J", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                long(long_arg(args, 0).max(long_arg(args, 1)))
            }),
            method("max", "(FF)F", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                float(fmax(float_arg(args, 0), float_arg(args, 1)))
            }),
            method("max", "(DD)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(fmax64(double_arg(args, 0), double_arg(args, 1)))
            }),
            method("sqrt", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(sqrt(double_arg(args, 0)))
            }),
            method("cbrt", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(cbrt(double_arg(args, 0)))
            }),
            method("sin", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(sin(double_arg(args, 0)))
            }),
            method("cos", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(cos(double_arg(args, 0)))
            }),
            method("tan", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(tan(double_arg(args, 0)))
            }),
            method("asin", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(asin(double_arg(args, 0)))
            }),
            method("acos", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(acos(double_arg(args, 0)))
            }),
            method("atan", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(atan(double_arg(args, 0)))
            }),
            method("atan2", "(DD)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(atan2(double_arg(args, 0), double_arg(args, 1)))
            }),
            method("exp", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(exp(double_arg(args, 0)))
            }),
            method("log", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(ln(double_arg(args, 0)))
            }),
            method("log10", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(log10(double_arg(args, 0)))
            }),
            method("pow", "(DD)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(pow(double_arg(args, 0), double_arg(args, 1)))
            }),
            method("floor", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(floor(double_arg(args, 0)))
            }),
            method("ceil", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(ceil(double_arg(args, 0)))
            }),
            method("rint", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(rint(double_arg(args, 0)))
            }),
            method("round", "(F)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                int(round_f32(float_arg(args, 0)))
            }),
            method("round", "(D)J", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                long(round(double_arg(args, 0)))
            }),
            method("hypot", "(DD)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(hypot(double_arg(args, 0), double_arg(args, 1)))
            }),
            method("signum", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(signum_f64(double_arg(args, 0)))
            }),
            method("signum", "(F)F", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                float(if float_arg(args, 0).is_nan() {
                    f32::NAN
                } else {
                    signum_f64(f64::from(float_arg(args, 0))) as f32
                })
            }),
            method(
                "copySign",
                "(DD)D",
                ACC_PUBLIC | ACC_STATIC,
                |_, _, args| double(copy_sign(double_arg(args, 0), double_arg(args, 1))),
            ),
            method(
                "copySign",
                "(FF)F",
                ACC_PUBLIC | ACC_STATIC,
                |_, _, args| {
                    float(
                        copy_sign(f64::from(float_arg(args, 0)), f64::from(float_arg(args, 1)))
                            as f32,
                    )
                },
            ),
            method(
                "IEEEremainder",
                "(DD)D",
                ACC_PUBLIC | ACC_STATIC,
                |_, _, args| double(ieee_remainder(double_arg(args, 0), double_arg(args, 1))),
            ),
            method(
                "toRadians",
                "(D)D",
                ACC_PUBLIC | ACC_STATIC,
                |_, _, args| double(double_arg(args, 0) * (core::f64::consts::PI / 180.0)),
            ),
            method(
                "toDegrees",
                "(D)D",
                ACC_PUBLIC | ACC_STATIC,
                |_, _, args| double(double_arg(args, 0) * (180.0 / core::f64::consts::PI)),
            ),
            method("random", "()D", ACC_PUBLIC | ACC_STATIC, |vm, _, _| {
                double(vm.random_f64())
            }),
            method(
                "floorDiv",
                "(II)I",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    math_floor_div(vm, i64::from(int_arg(args, 0)), i64::from(int_arg(args, 1)))
                        .map(|v| Value::Int(v as i32))
                },
            ),
            method(
                "floorDiv",
                "(JJ)J",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    math_floor_div(vm, long_arg(args, 0), long_arg(args, 1)).map(Value::Long)
                },
            ),
            method(
                "floorDiv",
                "(JI)J",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    math_floor_div(vm, long_arg(args, 0), i64::from(int_arg(args, 1)))
                        .map(Value::Long)
                },
            ),
            method(
                "floorMod",
                "(II)I",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    math_floor_mod(vm, i64::from(int_arg(args, 0)), i64::from(int_arg(args, 1)))
                        .map(|v| Value::Int(v as i32))
                },
            ),
            method(
                "floorMod",
                "(JJ)J",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    math_floor_mod(vm, long_arg(args, 0), long_arg(args, 1)).map(Value::Long)
                },
            ),
            method(
                "floorMod",
                "(JI)I",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    math_floor_mod(vm, long_arg(args, 0), i64::from(int_arg(args, 1)))
                        .map(|v| Value::Int(v as i32))
                },
            ),
            method(
                "addExact",
                "(II)I",
                ACC_PUBLIC | ACC_STATIC,
                math_add_exact_int,
            ),
            method(
                "addExact",
                "(JJ)J",
                ACC_PUBLIC | ACC_STATIC,
                math_add_exact_long,
            ),
            method(
                "subtractExact",
                "(II)I",
                ACC_PUBLIC | ACC_STATIC,
                math_sub_exact_int,
            ),
            method(
                "subtractExact",
                "(JJ)J",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    long_arg(args, 0)
                        .checked_sub(long_arg(args, 1))
                        .ok_or_else(|| {
                            vm.throw_new("java/lang/ArithmeticException", Some("long overflow"))
                        })
                        .map(Value::Long)
                },
            ),
            method(
                "multiplyExact",
                "(II)I",
                ACC_PUBLIC | ACC_STATIC,
                math_mul_exact_int,
            ),
            method(
                "multiplyExact",
                "(JJ)J",
                ACC_PUBLIC | ACC_STATIC,
                math_mul_exact_long,
            ),
            method(
                "multiplyExact",
                "(JI)J",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    long_arg(args, 0)
                        .checked_mul(i64::from(int_arg(args, 1)))
                        .ok_or_else(|| {
                            vm.throw_new("java/lang/ArithmeticException", Some("long overflow"))
                        })
                        .map(Value::Long)
                },
            ),
            method(
                "divideExact",
                "(II)I",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    let x = int_arg(args, 0);
                    let y = int_arg(args, 1);
                    if y == 0 {
                        return Err(
                            vm.throw_new("java/lang/ArithmeticException", Some("/ by zero"))
                        );
                    }
                    x.checked_div(y).map(Value::Int).ok_or_else(|| {
                        vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
                    })
                },
            ),
            method(
                "divideExact",
                "(JJ)J",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    let x = long_arg(args, 0);
                    let y = long_arg(args, 1);
                    if y == 0 {
                        return Err(
                            vm.throw_new("java/lang/ArithmeticException", Some("/ by zero"))
                        );
                    }
                    x.checked_div(y).map(Value::Long).ok_or_else(|| {
                        vm.throw_new("java/lang/ArithmeticException", Some("long overflow"))
                    })
                },
            ),
            method(
                "floorDivExact",
                "(II)I",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    let x = int_arg(args, 0);
                    let y = int_arg(args, 1);
                    if y == 0 {
                        return Err(
                            vm.throw_new("java/lang/ArithmeticException", Some("/ by zero"))
                        );
                    }
                    let q = x.checked_div(y).ok_or_else(|| {
                        vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
                    })?;
                    if (x ^ y) < 0 && q.wrapping_mul(y) != x {
                        int(q.wrapping_sub(1))
                    } else {
                        int(q)
                    }
                },
            ),
            method(
                "floorDivExact",
                "(JJ)J",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    math_floor_div_exact(vm, long_arg(args, 0), long_arg(args, 1), "long overflow")
                        .map(Value::Long)
                },
            ),
            method(
                "ceilDiv",
                "(II)I",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    math_ceil_div(vm, i64::from(int_arg(args, 0)), i64::from(int_arg(args, 1)))
                        .map(|v| Value::Int(v as i32))
                },
            ),
            method(
                "ceilDiv",
                "(JJ)J",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    math_ceil_div(vm, long_arg(args, 0), long_arg(args, 1)).map(Value::Long)
                },
            ),
            method(
                "ceilDiv",
                "(JI)J",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    math_ceil_div(vm, long_arg(args, 0), i64::from(int_arg(args, 1)))
                        .map(Value::Long)
                },
            ),
            method(
                "ceilMod",
                "(II)I",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    math_ceil_mod(vm, i64::from(int_arg(args, 0)), i64::from(int_arg(args, 1)))
                        .map(|v| Value::Int(v as i32))
                },
            ),
            method(
                "ceilMod",
                "(JJ)J",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    math_ceil_mod(vm, long_arg(args, 0), long_arg(args, 1)).map(Value::Long)
                },
            ),
            method(
                "ceilMod",
                "(JI)I",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    math_ceil_mod(vm, long_arg(args, 0), i64::from(int_arg(args, 1)))
                        .map(|v| Value::Int(v as i32))
                },
            ),
            method(
                "ceilDivExact",
                "(II)I",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    let x = int_arg(args, 0);
                    let y = int_arg(args, 1);
                    if y == 0 {
                        return Err(
                            vm.throw_new("java/lang/ArithmeticException", Some("/ by zero"))
                        );
                    }
                    let q = x.checked_div(y).ok_or_else(|| {
                        vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
                    })?;
                    if (x ^ y) >= 0 && q.wrapping_mul(y) != x {
                        int(q.wrapping_add(1))
                    } else {
                        int(q)
                    }
                },
            ),
            method(
                "ceilDivExact",
                "(JJ)J",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    math_ceil_div_exact(vm, long_arg(args, 0), long_arg(args, 1), "long overflow")
                        .map(Value::Long)
                },
            ),
            method(
                "incrementExact",
                "(I)I",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    int_arg(args, 0)
                        .checked_add(1)
                        .map(Value::Int)
                        .ok_or_else(|| {
                            vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
                        })
                },
            ),
            method(
                "incrementExact",
                "(J)J",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    long_arg(args, 0)
                        .checked_add(1)
                        .map(Value::Long)
                        .ok_or_else(|| {
                            vm.throw_new("java/lang/ArithmeticException", Some("long overflow"))
                        })
                },
            ),
            method(
                "decrementExact",
                "(I)I",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    int_arg(args, 0)
                        .checked_sub(1)
                        .map(Value::Int)
                        .ok_or_else(|| {
                            vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
                        })
                },
            ),
            method(
                "decrementExact",
                "(J)J",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    long_arg(args, 0)
                        .checked_sub(1)
                        .map(Value::Long)
                        .ok_or_else(|| {
                            vm.throw_new("java/lang/ArithmeticException", Some("long overflow"))
                        })
                },
            ),
            method(
                "negateExact",
                "(I)I",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    int_arg(args, 0)
                        .checked_neg()
                        .ok_or_else(|| {
                            vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
                        })
                        .map(Value::Int)
                },
            ),
            method(
                "negateExact",
                "(J)J",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    long_arg(args, 0)
                        .checked_neg()
                        .ok_or_else(|| {
                            vm.throw_new("java/lang/ArithmeticException", Some("long overflow"))
                        })
                        .map(Value::Long)
                },
            ),
            method(
                "toIntExact",
                "(J)I",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    i32::try_from(long_arg(args, 0))
                        .map(Value::Int)
                        .map_err(|_| {
                            vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
                        })
                },
            ),
            method(
                "nextAfter",
                "(DD)D",
                ACC_PUBLIC | ACC_STATIC,
                |_, _, args| double(next_after(double_arg(args, 0), double_arg(args, 1))),
            ),
            method("nextUp", "(F)F", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                float(next_up_float(float_arg(args, 0)))
            }),
            method("nextDown", "(F)F", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                float(-next_up_float(-float_arg(args, 0)))
            }),
            method("nextUp", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(next_after(double_arg(args, 0), f64::INFINITY))
            }),
            method("nextDown", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(next_after(double_arg(args, 0), f64::NEG_INFINITY))
            }),
            method("ulp", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                let value = double_arg(args, 0).abs();
                double(next_after(value, f64::INFINITY) - value)
            }),
            method("ulp", "(F)F", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                let value = float_arg(args, 0).abs();
                float(next_up_float(value) - value)
            }),
            method(
                "getExponent",
                "(F)I",
                ACC_PUBLIC | ACC_STATIC,
                |_, _, args| {
                    let value = float_arg(args, 0);
                    let bits = value.to_bits();
                    let exponent = ((bits >> 23) & 0xff) as i32;
                    int(if value.is_nan() || value.is_infinite() {
                        0x80
                    } else if exponent == 0 {
                        if bits & 0x7f_ffff == 0 { -127 } else { -126 }
                    } else {
                        exponent - 127
                    })
                },
            ),
            method(
                "getExponent",
                "(D)I",
                ACC_PUBLIC | ACC_STATIC,
                |_, _, args| {
                    let value = double_arg(args, 0);
                    let bits = value.to_bits();
                    let exponent = ((bits >> 52) & 0x7ff) as i32;
                    int(if value.is_nan() || value.is_infinite() {
                        0x400
                    } else if exponent == 0 {
                        if bits & 0xf_ffff_ffff_ffff == 0 {
                            -1023
                        } else {
                            -1022
                        }
                    } else {
                        exponent - 1023
                    })
                },
            ),
            method("scalb", "(FI)F", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                float((f64::from(float_arg(args, 0)) * pow2(int_arg(args, 1))) as f32)
            }),
            method("scalb", "(DI)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(double_arg(args, 0) * pow2(int_arg(args, 1)))
            }),
            method("fma", "(DDD)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(fma_f64(
                    double_arg(args, 0),
                    double_arg(args, 1),
                    double_arg(args, 2),
                ))
            }),
            method("fma", "(FFF)F", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                float(fma_f32(
                    float_arg(args, 0),
                    float_arg(args, 1),
                    float_arg(args, 2),
                ))
            }),
            method(
                "powExact",
                "(II)I",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    let x = int_arg(args, 0);
                    let n = int_arg(args, 1);
                    if n < 0 {
                        return Err(vm.throw_new(
                            "java/lang/ArithmeticException",
                            Some("negative exponent"),
                        ));
                    }
                    let result = x.checked_pow(n as u32).ok_or_else(|| {
                        vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
                    })?;
                    int(result)
                },
            ),
            method(
                "powExact",
                "(JI)J",
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    let x = long_arg(args, 0);
                    let n = int_arg(args, 1);
                    if n < 0 {
                        return Err(vm.throw_new(
                            "java/lang/ArithmeticException",
                            Some("negative exponent"),
                        ));
                    }
                    let result = x.checked_pow(n as u32).ok_or_else(|| {
                        vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
                    })?;
                    long(result)
                },
            ),
            method("expm1", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(expm1(double_arg(args, 0)))
            }),
            method("log1p", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(log1p(double_arg(args, 0)))
            }),
            method("sinh", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(sinh(double_arg(args, 0)))
            }),
            method("cosh", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(cosh(double_arg(args, 0)))
            }),
            method("tanh", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(tanh(double_arg(args, 0)))
            }),
        ]
    }};
}

const MATH_METHODS: [oxjvm_vm::NativeMethodDef; 99] = math_methods!();
const STRICT_MATH_METHODS: [oxjvm_vm::NativeMethodDef; 99] = math_methods!();

pub(crate) const MATH: oxjvm_vm::NativeClass = class(
    "java/lang/Math",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field(
            "E",
            "D",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Double(core::f64::consts::E)),
        ),
        field(
            "PI",
            "D",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Double(core::f64::consts::PI)),
        ),
    ],
    &MATH_METHODS,
    None,
);

pub(crate) const STRICT_MATH: oxjvm_vm::NativeClass = class(
    "java/lang/StrictMath",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field(
            "E",
            "D",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Double(core::f64::consts::E)),
        ),
        field(
            "PI",
            "D",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Double(core::f64::consts::PI)),
        ),
    ],
    &STRICT_MATH_METHODS,
    None,
);

// java.math.RoundingMode
// -------------------------------------------------------------------------------------------

const ROUNDING_MODE_NAMES: [&str; 8] = [
    "UP",
    "DOWN",
    "CEILING",
    "FLOOR",
    "HALF_UP",
    "HALF_DOWN",
    "HALF_EVEN",
    "UNNECESSARY",
];

fn rounding_mode_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/math/RoundingMode")?;
    for (index, name) in ROUNDING_MODE_NAMES.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        if let Some((declaring, field)) = vm.find_field_opt(class, "ordinal", "I") {
            vm.set_instance_int(instance, declaring, field, index as i32);
        }
        vm.set_static_value(
            class,
            name,
            "Ljava/math/RoundingMode;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

fn rounding_mode_ordinal(vm: &Vm<'_>, instance: ObjectRef) -> i32 {
    crate::lang::read_int_field(vm, instance, "ordinal")
}

const MATH_CONTEXT_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let precision = int_arg(args, 1);
        if precision < 0 {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("Precision must be non-negative"),
            ));
        }
        let mode = rounding_mode_constant(vm, "HALF_UP")?;
        math_context_init(vm, this, precision, mode)
    }),
    method(
        "<init>",
        "(ILjava/math/RoundingMode;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let precision = int_arg(args, 1);
            let mode = ref_arg(args, 2);
            if mode.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            if precision < 0 {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Precision must be non-negative"),
                ));
            }
            math_context_init(vm, this, precision, mode)
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm.string_or_empty(ref_arg(args, 1));
            let mut precision = 0i32;
            let mut mode_name = String::from("HALF_UP");
            for part in text.split_whitespace() {
                if let Some(value) = part.strip_prefix("precision=") {
                    precision = value.parse().unwrap_or(0);
                } else if let Some(value) = part.strip_prefix("roundingMode=") {
                    mode_name = String::from(value);
                }
            }
            let mode = rounding_mode_constant(vm, &mode_name)?;
            math_context_init(vm, this, precision, mode)
        },
    ),
    method("getPrecision", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "precision"))
    }),
    method(
        "getRoundingMode",
        "()Ljava/math/RoundingMode;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let mode = vm
                .read_ref_field(
                    this,
                    vm.class_of(this),
                    "roundingMode",
                    "Ljava/math/RoundingMode;",
                )
                .unwrap_or(ObjectRef::NULL);
            object(mode)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let precision = crate::lang::read_int_field(vm, this, "precision");
            let mode = vm
                .read_ref_field(
                    this,
                    vm.class_of(this),
                    "roundingMode",
                    "Ljava/math/RoundingMode;",
                )
                .unwrap_or(ObjectRef::NULL);
            let ordinal = rounding_mode_ordinal(vm, mode).clamp(0, 7) as usize;
            let name = ROUNDING_MODE_NAMES[ordinal];
            object(vm.make_string(&alloc::format!("precision={precision} roundingMode={name}"))?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if this == other {
                return boolean(true);
            }
            if vm.class_name(vm.class_of(other)) != "java/math/MathContext" {
                return boolean(false);
            }
            let p1 = crate::lang::read_int_field(vm, this, "precision");
            let p2 = crate::lang::read_int_field(vm, other, "precision");
            let m1 = vm
                .read_ref_field(
                    this,
                    vm.class_of(this),
                    "roundingMode",
                    "Ljava/math/RoundingMode;",
                )
                .unwrap_or(ObjectRef::NULL);
            let m2 = vm
                .read_ref_field(
                    other,
                    vm.class_of(other),
                    "roundingMode",
                    "Ljava/math/RoundingMode;",
                )
                .unwrap_or(ObjectRef::NULL);
            boolean(p1 == p2 && rounding_mode_ordinal(vm, m1) == rounding_mode_ordinal(vm, m2))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let precision = crate::lang::read_int_field(vm, this, "precision");
        let mode = vm
            .read_ref_field(
                this,
                vm.class_of(this),
                "roundingMode",
                "Ljava/math/RoundingMode;",
            )
            .unwrap_or(ObjectRef::NULL);
        int(precision
            .wrapping_mul(31)
            .wrapping_add(rounding_mode_ordinal(vm, mode)))
    }),
];

fn math_context_init(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    precision: i32,
    mode: ObjectRef,
) -> Result<Value, VmError> {
    crate::lang::write_int_field(vm, this, "precision", precision);
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "roundingMode", "Ljava/math/RoundingMode;")?;
    vm.set_instance_ref(this, declaring, field, mode);
    void()
}

fn math_context_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/math/MathContext")?;
    for (name, precision, mode_name) in [
        ("DECIMAL32", 7, "HALF_EVEN"),
        ("DECIMAL64", 16, "HALF_EVEN"),
        ("DECIMAL128", 34, "HALF_EVEN"),
        ("UNLIMITED", 0, "UNNECESSARY"),
    ] {
        let mode = rounding_mode_constant(vm, mode_name)?;
        let instance = vm.new_instance(class)?;
        crate::lang::write_int_field(vm, instance, "precision", precision);
        let (declaring, field) =
            vm.find_field(class, "roundingMode", "Ljava/math/RoundingMode;")?;
        vm.set_instance_ref(instance, declaring, field, mode);
        vm.set_static_value(class, name, "Ljava/math/MathContext;", Value::Ref(instance));
    }
    Ok(())
}

pub(crate) const MATH_CONTEXT: oxjvm_vm::NativeClass = class(
    "java/math/MathContext",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("precision", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "roundingMode",
            "Ljava/math/RoundingMode;",
            ACC_PRIVATE | ACC_FINAL,
            None,
        ),
        field(
            "DECIMAL32",
            "Ljava/math/MathContext;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "DECIMAL64",
            "Ljava/math/MathContext;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "DECIMAL128",
            "Ljava/math/MathContext;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "UNLIMITED",
            "Ljava/math/MathContext;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &MATH_CONTEXT_METHODS,
    Some(math_context_clinit),
);

const ROUNDING_MODE_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(rounding_mode_ordinal(vm, receiver(args)))
    }),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let index = rounding_mode_ordinal(vm, receiver(args)).clamp(0, 7) as usize;
        object(vm.make_string(ROUNDING_MODE_NAMES[index])?)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let index = rounding_mode_ordinal(vm, receiver(args)).clamp(0, 7) as usize;
            object(vm.make_string(ROUNDING_MODE_NAMES[index])?)
        },
    ),
    method(
        "values",
        "()[Ljava/math/RoundingMode;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/math/RoundingMode")?;
            vm.ensure_initialized(class)?;
            let array = vm.allocate_object_array(class, ROUNDING_MODE_NAMES.len())?;
            for (index, name) in ROUNDING_MODE_NAMES.iter().enumerate() {
                if let Some(value) =
                    vm.read_static_ref_if_set(class, name, "Ljava/math/RoundingMode;")
                {
                    vm.array_set_ref(array, index, value)?;
                }
            }
            object(array)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/math/RoundingMode;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            let class = vm.resolve_class("java/math/RoundingMode")?;
            vm.ensure_initialized(class)?;
            for candidate in ROUNDING_MODE_NAMES {
                if candidate == name {
                    if let Some(value) =
                        vm.read_static_ref_if_set(class, candidate, "Ljava/math/RoundingMode;")
                    {
                        return object(value);
                    }
                }
            }
            Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some(&alloc::format!(
                    "No enum constant java.math.RoundingMode.{name}"
                )),
            ))
        },
    ),
    method(
        "valueOf",
        "(I)Ljava/math/RoundingMode;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let mode = int_arg(args, 0);
            if !(0..=7).contains(&mode) {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            }
            let class = vm.resolve_class("java/math/RoundingMode")?;
            vm.ensure_initialized(class)?;
            let name = ROUNDING_MODE_NAMES[mode as usize];
            let value = vm
                .read_static_ref_if_set(class, name, "Ljava/math/RoundingMode;")
                .ok_or_else(|| VmError::internal("RoundingMode constants not initialized"))?;
            object(value)
        },
    ),
];

pub(crate) const ROUNDING_MODE: oxjvm_vm::NativeClass = class(
    "java/math/RoundingMode",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "UP",
            "Ljava/math/RoundingMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "DOWN",
            "Ljava/math/RoundingMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "CEILING",
            "Ljava/math/RoundingMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "FLOOR",
            "Ljava/math/RoundingMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "HALF_UP",
            "Ljava/math/RoundingMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "HALF_DOWN",
            "Ljava/math/RoundingMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "HALF_EVEN",
            "Ljava/math/RoundingMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "UNNECESSARY",
            "Ljava/math/RoundingMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &ROUNDING_MODE_METHODS,
    Some(rounding_mode_clinit),
);

// java.math.BigInteger / BigDecimal
// -------------------------------------------------------------------------------------------

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::{boolean, object, receiver, ref_arg, string_arg, void};

/// The decimal text stored in a big-number instance (`0` when unset).
fn big_text(vm: &Vm<'_>, this: ObjectRef) -> String {
    let class = vm.class_of(this);
    vm.read_ref_field(this, class, "value", "Ljava/lang/String;")
        .map(|text| vm.string_or_empty(text))
        .unwrap_or_else(|| String::from("0"))
}

/// Convert a decimal big-integer string to an arbitrary radix (2..=36), as
/// `BigInteger.toString(radix)` requires. Uses repeated short division on the
/// decimal digit vector so values beyond 64 bits (e.g. 2^64) convert correctly.
fn big_decimal_to_radix(text: &str, radix: u32) -> String {
    let radix = radix.clamp(2, 36);
    if radix == 10 {
        return String::from(text);
    }
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let mut number: Vec<u32> = digits
        .bytes()
        .filter_map(|b| (b as char).to_digit(10))
        .collect();
    if number.is_empty() {
        return String::from("0");
    }
    let mut out = String::new();
    while !number.is_empty() {
        let mut carry = 0u32;
        let mut quotient = Vec::with_capacity(number.len());
        for &digit in &number {
            let current = carry * 10 + digit;
            let q = current / radix;
            carry = current % radix;
            if !quotient.is_empty() || q != 0 {
                quotient.push(q);
            }
        }
        out.push(core::char::from_digit(carry, radix).unwrap_or('?'));
        number = quotient;
    }
    let mut result: String = out.chars().rev().collect();
    if negative && result != "0" {
        result.insert(0, '-');
    }
    result
}

/// Create an instance of a big-number class carrying `text`.
pub(crate) fn create_big_number(
    vm: &mut Vm<'_>,
    class_name: &str,
    text: &str,
) -> Result<Value, VmError> {
    let class = vm.resolve_class(class_name)?;
    let instance = vm.new_instance(class)?;
    let string = vm.make_string(text)?;
    let (declaring, field) = vm.find_field(class, "value", "Ljava/lang/String;")?;
    vm.set_instance_ref(instance, declaring, field, string);
    object(instance)
}

/// Java's `String.hashCode`, used here as the big-number hash.
fn java_hash(text: &str) -> i32 {
    text.chars().fold(0_i32, |hash, ch| {
        hash.wrapping_mul(31).wrapping_add(ch as i32)
    })
}

fn parse_i128(text: &str) -> Option<i128> {
    text.parse::<i128>().ok()
}

fn write_big_text(vm: &mut Vm<'_>, this: ObjectRef, text: &str) -> Result<Value, VmError> {
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "value", "Ljava/lang/String;")?;
    let string = vm.make_string(text)?;
    vm.set_instance_ref(this, declaring, field, string);
    Ok(Value::Int(0))
}

fn bytes_array(vm: &mut Vm<'_>, bytes: &[u8]) -> Result<ObjectRef, VmError> {
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(
            array,
            index as i32,
            Value::Int(i32::from(*byte as i8)),
            false,
        )?;
    }
    Ok(array)
}

fn i128_to_twos_complement(value: i128) -> Vec<u8> {
    if value == 0 {
        return vec![0];
    }
    if value > 0 {
        let mut bytes = value.to_be_bytes().to_vec();
        while bytes.len() > 1 && bytes[0] == 0 {
            bytes.remove(0);
        }
        if bytes[0] & 0x80 != 0 {
            bytes.insert(0, 0);
        }
        return bytes;
    }
    let magnitude = value.unsigned_abs();
    let mut bytes = magnitude.to_be_bytes().to_vec();
    while bytes.len() > 1 && bytes[0] == 0 {
        bytes.remove(0);
    }
    for byte in &mut bytes {
        *byte = !*byte;
    }
    let mut carry = 1u16;
    for byte in bytes.iter_mut().rev() {
        let sum = u16::from(*byte) + carry;
        *byte = (sum & 0xFF) as u8;
        carry = sum >> 8;
    }
    while bytes.len() > 1 && bytes[0] == 0xFF && bytes[1] & 0x80 != 0 {
        bytes.remove(0);
    }
    bytes
}

/// Build a BigDecimal value text with an explicit scale. Negative scales
/// stay in `E` notation so `scale()` and the JDK identity rules survive
/// (`valueOf(1, -3)` is `1E+3`, not `1000`).
fn big_decimal_with_scale(text: &str, scale: i32) -> String {
    if scale < 0 {
        alloc::format!("{text}E+{}", -(scale as i64))
    } else {
        scale_decimal(text, -scale)
    }
}

fn scale_decimal(text: &str, power: i32) -> String {
    if power == 0 {
        return String::from(text);
    }
    if let Some(value) = parse_i128(text) {
        if power > 0 {
            if let Some(factor) = 10i128.checked_pow(power as u32) {
                if let Some(scaled) = value.checked_mul(factor) {
                    return scaled.to_string();
                }
            }
        } else if let Some(formatted) = format_scaled(value, (-power) as usize) {
            return formatted;
        }
    }
    let value = text.parse::<f64>().unwrap_or(0.0);
    let mut factor = 1.0f64;
    for _ in 0..power.unsigned_abs() {
        factor *= 10.0;
    }
    if power >= 0 {
        alloc::format!("{}", value * factor)
    } else {
        alloc::format!("{}", value / factor)
    }
}

fn format_scaled(value: i128, scale: usize) -> Option<String> {
    let negative = value < 0;
    let digits = value.unsigned_abs().to_string();
    let mut text = String::new();
    if negative {
        text.push('-');
    }
    if scale == 0 {
        text.push_str(&digits);
        return Some(text);
    }
    if digits.len() <= scale {
        text.push_str("0.");
        for _ in 0..(scale - digits.len()) {
            text.push('0');
        }
        text.push_str(&digits);
    } else {
        let point = digits.len() - scale;
        text.push_str(&digits[..point]);
        text.push('.');
        text.push_str(&digits[point..]);
    }
    Some(text)
}

/// Convert a non-negative decimal digit string to little-endian base-2^32
/// limbs by repeated multiply-add, so magnitudes beyond i128 stay exact.
fn decimal_to_limbs(digits: &str) -> Vec<u32> {
    let mut limbs: Vec<u32> = Vec::new();
    for byte in digits.bytes() {
        let mut carry = u64::from(byte - b'0');
        for limb in &mut limbs {
            let cur = u64::from(*limb) * 10 + carry;
            *limb = cur as u32;
            carry = cur >> 32;
        }
        if carry != 0 {
            limbs.push(carry as u32);
        }
    }
    while limbs.last() == Some(&0) {
        limbs.pop();
    }
    limbs
}

fn limbs_cmp(a: &[u32], b: &[u32]) -> Ordering {
    a.len()
        .cmp(&b.len())
        .then_with(|| a.iter().rev().cmp(b.iter().rev()))
}

fn limbs_add(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u64;
    for i in 0..a.len().max(b.len()) {
        let cur = u64::from(a.get(i).copied().unwrap_or(0))
            + u64::from(b.get(i).copied().unwrap_or(0))
            + carry;
        out.push(cur as u32);
        carry = cur >> 32;
    }
    if carry != 0 {
        out.push(carry as u32);
    }
    out
}

/// Subtract `b` from `a`, requiring `a >= b`.
fn limbs_sub(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::with_capacity(a.len());
    let mut borrow = 0i64;
    for (i, &word) in a.iter().enumerate() {
        let cur = i64::from(word) - i64::from(b.get(i).copied().unwrap_or(0)) - borrow;
        if cur < 0 {
            out.push((cur + (1i64 << 32)) as u32);
            borrow = 1;
        } else {
            out.push(cur as u32);
            borrow = 0;
        }
    }
    while out.last() == Some(&0) {
        out.pop();
    }
    out
}

fn limbs_mul(a: &[u32], b: &[u32]) -> Vec<u32> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let mut out = alloc::vec![0u32; a.len() + b.len()];
    for (i, &x) in a.iter().enumerate() {
        let mut carry = 0u64;
        for (j, &y) in b.iter().enumerate() {
            let cur = u64::from(x) * u64::from(y) + u64::from(out[i + j]) + carry;
            out[i + j] = cur as u32;
            carry = cur >> 32;
        }
        out[i + b.len()] = carry as u32;
    }
    while out.last() == Some(&0) {
        out.pop();
    }
    out
}

/// Render an unsigned digit string at the given scale, mirroring
/// `format_scaled` for magnitudes that no longer fit in i128.
fn format_scaled_digits(digits: &str, negative: bool, scale: usize) -> String {
    let mut text = String::new();
    if negative && digits != "0" {
        text.push('-');
    }
    if scale == 0 {
        text.push_str(digits);
        return text;
    }
    if digits.len() <= scale {
        text.push_str("0.");
        for _ in 0..(scale - digits.len()) {
            text.push('0');
        }
        text.push_str(digits);
    } else {
        let point = digits.len() - scale;
        text.push_str(&digits[..point]);
        text.push('.');
        text.push_str(&digits[point..]);
    }
    text
}

fn is_probable_prime(value: i128) -> bool {
    if value < 2 {
        return false;
    }
    if value < 4 {
        return true;
    }
    if value % 2 == 0 {
        return false;
    }
    let mut divisor = 3i128;
    while divisor.saturating_mul(divisor) <= value && divisor < 1_000_000 {
        if value % divisor == 0 {
            return false;
        }
        divisor += 2;
    }
    true
}

/// Validate and canonicalise a decimal integer string like `new BigInteger(String)`.
fn big_integer_parse(text: &str) -> Option<String> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    if digits.is_empty() {
        return None;
    }
    // JDK accepts any Unicode decimal digit (Character.digit); normalise to ASCII.
    let mut ascii = String::with_capacity(digits.len());
    for c in digits.chars() {
        let d = c
            .to_digit(10)
            .or_else(|| crate::lang::unicode_decimal_digit(c))?;
        ascii.push((b'0' + d as u8) as char);
    }
    let trimmed = ascii.trim_start_matches('0');
    if trimmed.is_empty() {
        return Some(String::from("0"));
    }
    Some(if negative {
        alloc::format!("-{trimmed}")
    } else {
        String::from(trimmed)
    })
}

/// Convert a big-endian magnitude byte array into little-endian base-2^32
/// limbs, dropping leading zero words.
fn bytes_to_limbs(bytes: &[u8]) -> Vec<u32> {
    let mut limbs: Vec<u32> = Vec::new();
    let mut end = bytes.len();
    while end > 0 {
        let start = end.saturating_sub(4);
        let mut word = 0u32;
        for &byte in &bytes[start..end] {
            word = (word << 8) | u32::from(byte);
        }
        limbs.push(word);
        end = start;
    }
    while limbs.last() == Some(&0) {
        limbs.pop();
    }
    limbs
}

/// Render little-endian base-2^32 limbs as a decimal string by repeated
/// division by 10^9, so magnitudes beyond i128 stay exact.
fn limbs_to_decimal(mut limbs: Vec<u32>) -> String {
    if limbs.is_empty() {
        return String::from("0");
    }
    let mut chunks: Vec<u32> = Vec::new();
    while !limbs.is_empty() {
        let mut rem = 0u64;
        for limb in limbs.iter_mut().rev() {
            let cur = (rem << 32) | u64::from(*limb);
            *limb = (cur / 1_000_000_000) as u32;
            rem = cur % 1_000_000_000;
        }
        chunks.push(rem as u32);
        while limbs.last() == Some(&0) {
            limbs.pop();
        }
    }
    let mut text = alloc::format!("{}", chunks.last().copied().unwrap_or(0));
    for chunk in chunks.iter().rev().skip(1) {
        text.push_str(&alloc::format!("{chunk:09}"));
    }
    text
}

/// Subtract one from a non-negative decimal digit string (no sign).
fn dec_sub_one(digits: &str) -> String {
    let mut bytes: Vec<u8> = digits.bytes().collect();
    let mut i = bytes.len();
    while i > 0 {
        i -= 1;
        if bytes[i] > b'0' {
            bytes[i] -= 1;
            break;
        }
        bytes[i] = b'9';
    }
    let text: String = bytes.iter().map(|&b| b as char).collect();
    let trimmed = text.trim_start_matches('0');
    String::from(if trimmed.is_empty() { "0" } else { trimmed })
}

/// Add one to a non-negative decimal digit string (no sign).
fn dec_add_one(digits: &str) -> String {
    let mut bytes: Vec<u8> = digits.bytes().collect();
    let mut i = bytes.len();
    let mut carry = true;
    while i > 0 && carry {
        i -= 1;
        if bytes[i] < b'9' {
            bytes[i] += 1;
            carry = false;
        } else {
            bytes[i] = b'0';
        }
    }
    let text: String = bytes.iter().map(|&b| b as char).collect();
    if carry {
        alloc::format!("1{text}")
    } else {
        text
    }
}

/// Arbitrary-precision `BigInteger.shiftLeft`/`shiftRight` on decimal text
/// via base-2^32 limbs. Returns `None` when a left shift would need more
/// than a million bits, where the JDK raises OutOfMemoryError allocating
/// the magnitude array. Negative right shifts use the floor identity
/// `-x >> n == -((x - 1) >> n) - 1`.
fn big_integer_shift(text: &str, shift: i32, left: bool) -> Option<String> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() || shift == 0 {
        return Some(String::from(text));
    }
    let n = shift.unsigned_abs();
    if left && n > 1_000_000 {
        return None;
    }
    let magnitude = if negative && !left {
        dec_sub_one(digits)
    } else {
        String::from(digits)
    };
    let mut limbs: Vec<u32> = Vec::new();
    for byte in magnitude.bytes() {
        let mut carry = u64::from(byte - b'0');
        for limb in &mut limbs {
            let cur = u64::from(*limb) * 10 + carry;
            *limb = cur as u32;
            carry = cur >> 32;
        }
        if carry != 0 {
            limbs.push(carry as u32);
        }
    }
    let bit_len =
        limbs.len() as u64 * 32 - u64::from(limbs.last().copied().unwrap_or(0).leading_zeros());
    let shifted: Vec<u32> = if left {
        let word_shift = (n / 32) as usize;
        let bit_shift = n % 32;
        let mut out: Vec<u32> = alloc::vec![0u32; word_shift];
        let mut carry = 0u64;
        for &limb in &limbs {
            let cur = (u64::from(limb) << bit_shift) | carry;
            out.push(cur as u32);
            carry = cur >> 32;
        }
        if carry != 0 {
            out.push(carry as u32);
        }
        out
    } else {
        if u64::from(n) >= bit_len {
            return Some(String::from(if negative { "-1" } else { "0" }));
        }
        let word_shift = (n / 32) as usize;
        let bit_shift = n % 32;
        let mut out: Vec<u32> = Vec::new();
        for i in word_shift..limbs.len() {
            let mut word = limbs[i] >> bit_shift;
            if bit_shift > 0 && i + 1 < limbs.len() {
                word |= limbs[i + 1] << (32 - bit_shift);
            }
            out.push(word);
        }
        while out.last() == Some(&0) {
            out.pop();
        }
        out
    };
    let decimal = limbs_to_decimal(shifted);
    if negative && !left {
        Some(alloc::format!("-{}", dec_add_one(&decimal)))
    } else if negative {
        Some(alloc::format!("-{decimal}"))
    } else {
        Some(decimal)
    }
}

/// Two's-complement byte array to an exact signed decimal string for
/// magnitudes of any length (the i128 helper wraps past 16 bytes).
fn twos_complement_to_decimal(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::from("0");
    }
    let negative = bytes[0] & 0x80 != 0;
    let mut magnitude: Vec<u8> = Vec::from(bytes);
    if negative {
        let mut carry = 1u16;
        for byte in magnitude.iter_mut().rev() {
            let cur = u16::from(!*byte) + carry;
            *byte = cur as u8;
            carry = cur >> 8;
        }
    }
    let text = limbs_to_decimal(bytes_to_limbs(&magnitude));
    if negative && text != "0" {
        alloc::format!("-{text}")
    } else {
        text
    }
}

/// JDK `BigInteger.hashCode`: fold the big-endian 32-bit magnitude words
/// with `31*h + word` using wrapping int arithmetic, then apply the sign.
/// The decimal text is converted to base-2^32 limbs by repeated
/// multiply-add, so magnitudes beyond i128 hash correctly too.
fn big_integer_hash(text: &str) -> i32 {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        return 0;
    }
    let mut limbs: Vec<u32> = Vec::new();
    for byte in digits.bytes() {
        let mut carry = u64::from(byte - b'0');
        for limb in &mut limbs {
            let cur = u64::from(*limb) * 10 + carry;
            *limb = cur as u32;
            carry = cur >> 32;
        }
        if carry != 0 {
            limbs.push(carry as u32);
        }
    }
    let mut hash = 0i32;
    for &limb in limbs.iter().rev() {
        hash = hash.wrapping_mul(31).wrapping_add(limb as i32);
    }
    if negative { hash.wrapping_neg() } else { hash }
}

/// Numeric comparison of two canonical integer strings.
fn big_integer_compare(a: &str, b: &str) -> i32 {
    let a_negative = a.starts_with('-');
    let b_negative = b.starts_with('-');
    if a_negative != b_negative {
        return if a_negative { -1 } else { 1 };
    }
    let a_digits = a.trim_start_matches('-');
    let b_digits = b.trim_start_matches('-');
    let magnitude = a_digits
        .len()
        .cmp(&b_digits.len())
        .then_with(|| a_digits.cmp(b_digits));
    let result = match magnitude {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    };
    if a_negative { -result } else { result }
}

/// Decompose a decimal string (plain or scientific notation) into a sign,
/// its significant digits (no leading/trailing zeros) and a power-of-ten
/// exponent, so `digits * 10^exp` is the exact magnitude.
fn bd_decompose(text: &str) -> (i32, String, i64) {
    let t = text.trim();
    let (neg, t) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let (mantissa, exp) = match t.find(['E', 'e']) {
        Some(i) => (&t[..i], t[i + 1..].parse::<i64>().unwrap_or(0)),
        None => (t, 0_i64),
    };
    let (int_part, frac_part) = match mantissa.find('.') {
        Some(i) => (&mantissa[..i], &mantissa[i + 1..]),
        None => (mantissa, ""),
    };
    let frac_len = frac_part.len() as i64;
    let mut digits: String = int_part
        .bytes()
        .chain(frac_part.bytes())
        .filter(u8::is_ascii_digit)
        .map(|b| b as char)
        .collect();
    let mut e = exp - frac_len;
    let lead = digits.len() - digits.trim_start_matches('0').len();
    digits.drain(..lead);
    while digits.ends_with('0') {
        digits.pop();
        e += 1;
    }
    if digits.is_empty() {
        return (0, String::new(), 0);
    }
    (if neg { -1 } else { 1 }, digits, e)
}

/// Compare two exact decimal magnitudes `d1 * 10^e1` and `d2 * 10^e2`.
fn bd_compare_magnitude(d1: &str, e1: i64, d2: &str, e2: i64) -> Ordering {
    let p1 = e1 + d1.len() as i64;
    let p2 = e2 + d2.len() as i64;
    if p1 != p2 {
        return p1.cmp(&p2);
    }
    let len = d1.len().max(d2.len());
    let mut a = String::from(d1);
    let mut b = String::from(d2);
    while a.len() < len {
        a.push('0');
    }
    while b.len() < len {
        b.push('0');
    }
    a.cmp(&b)
}

/// Exact `BigDecimal.compareTo` on the canonical value strings, avoiding the
/// precision loss of comparing through `f64` (bug: distinct values near
/// `Long.MAX_VALUE` collapsed to the same double).
fn compare_big_decimal_text(a: &str, b: &str) -> Ordering {
    let (sa, da, ea) = bd_decompose(a);
    let (sb, db, eb) = bd_decompose(b);
    if sa != sb {
        return sa.cmp(&sb);
    }
    if sa == 0 {
        return Ordering::Equal;
    }
    let magnitude = bd_compare_magnitude(&da, ea, &db, eb);
    if sa < 0 {
        magnitude.reverse()
    } else {
        magnitude
    }
}

/// Validate and canonicalise a decimal string like `new BigDecimal(String)`,
/// preserving the scale: negative scales stay in the JDK's `E` notation
/// (`1e1` -> `1E+1`, `0e13` -> `0E+13`) so equals, scale and
/// stripTrailingZeros behave like the JDK.
fn big_decimal_parse(text: &str) -> Option<String> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (mantissa, exponent) = match digits.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().ok()?),
        None => (digits, 0),
    };
    let (integer, fraction) = match mantissa.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (mantissa, None),
    };
    if integer.is_empty() && fraction.is_none_or(|part: &str| part.is_empty()) {
        return None;
    }
    if !integer.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.is_some_and(|part| !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    let scale = fraction.map(str::len).unwrap_or(0) as i64 - i64::from(exponent);
    let combined = alloc::format!("{}{}", integer, fraction.unwrap_or(""));
    let trimmed = combined.trim_start_matches('0');
    if trimmed.is_empty() {
        if scale < 0 {
            return Some(alloc::format!("0E+{}", -scale));
        }
        return expand_decimal(text);
    }
    if scale < 0 {
        let mut canonical = String::new();
        if negative {
            canonical.push('-');
        }
        canonical.push_str(&trimmed[..1]);
        if trimmed.len() > 1 {
            canonical.push('.');
            canonical.push_str(&trimmed[1..]);
        }
        let adjusted = trimmed.len() as i64 - 1 - scale;
        canonical.push('E');
        if adjusted >= 0 {
            canonical.push('+');
        }
        canonical.push_str(&adjusted.to_string());
        return Some(canonical);
    }
    expand_decimal(text)
}

/// Would-be length and scale of the plain decimal form, computed without
/// allocating it, so huge exponents can raise OutOfMemoryError pre-emptively
/// like the JDK instead of trying to build a multi-gigabyte string.
fn decimal_plain_len(text: &str) -> Option<(i64, i64)> {
    let t = text.strip_prefix(['-', '+']).unwrap_or(text);
    let (mantissa, exponent) = match t.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i64>().ok()?),
        None => (t, 0),
    };
    let (int_part, frac_part) = match mantissa.split_once('.') {
        Some((i, f)) => (i, f),
        None => (mantissa, ""),
    };
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    if !int_part.bytes().all(|b| b.is_ascii_digit())
        || !frac_part.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let digits = int_part.len() as i64 + frac_part.len() as i64;
    let scale = frac_part.len() as i64 - exponent;
    let len = if scale >= 0 {
        digits.max(scale + 1) + 1
    } else {
        digits - scale
    };
    Some((len, scale))
}

/// Expand a decimal string (plain or scientific notation) to plain decimal
/// form, dropping the exponent but keeping fractional trailing zeros.
fn expand_decimal(text: &str) -> Option<String> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (mantissa, exponent) = match digits.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().ok()?),
        None => (digits, 0),
    };
    if exponent.abs() > 1_000_000 {
        return None;
    }
    let (integer, fraction) = match mantissa.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (mantissa, None),
    };
    if integer.is_empty() && fraction.is_none_or(|part: &str| part.is_empty()) {
        return None;
    }
    if !integer.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.is_some_and(|part| !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    let mut combined = alloc::format!("{}{}", integer, fraction.unwrap_or(""));
    let mut point = integer.len() as i32 + exponent;
    while point <= 0 {
        combined.insert(0, '0');
        point += 1;
    }
    while point as usize > combined.len() {
        combined.push('0');
    }
    let point = point as usize;
    let integer = &combined[..point];
    let fraction = &combined[point..];
    let trimmed = integer.trim_start_matches('0');
    let mut canonical = String::new();
    if negative {
        canonical.push('-');
    }
    if trimmed.is_empty() {
        canonical.push('0');
    } else {
        canonical.push_str(trimmed);
    }
    if !fraction.is_empty() {
        canonical.push('.');
        canonical.push_str(fraction);
    }
    Some(canonical)
}

/// Scale of a canonical BigDecimal string, including the `E` notation that
/// `stripTrailingZeros` produces.
fn decimal_scale(text: &str) -> i32 {
    let (mantissa, exponent) = match text.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().unwrap_or(0)),
        None => (text, 0),
    };
    let frac_len = mantissa
        .split_once('.')
        .map(|(_, fraction)| fraction.len())
        .unwrap_or(0) as i32;
    frac_len - exponent
}

/// Shift the decimal point of a BigDecimal string without rounding.
fn big_decimal_move_point(text: &str, shift: i32) -> String {
    let plain = expand_decimal(text).unwrap_or_else(|| String::from(text));
    let (negative, int_part, frac_part) = decimal_parts(&plain);
    let mut digits = alloc::format!("{}{}", int_part, frac_part);
    let mut point = int_part.len() as i32 + shift;
    while point <= 0 {
        digits.insert(0, '0');
        point += 1;
    }
    while point as usize > digits.len() {
        digits.push('0');
    }
    let point = point as usize;
    let mut raw = String::new();
    if negative {
        raw.push('-');
    }
    raw.push_str(&digits[..point]);
    if point < digits.len() {
        raw.push('.');
        raw.push_str(&digits[point..]);
    }
    expand_decimal(&raw).unwrap_or(raw)
}

/// `BigDecimal.divide(divisor, scale, roundingMode)` with exact i128 arithmetic
/// on the unscaled values when they fit (f64 fallback otherwise).
fn big_decimal_divide_scaled(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let a_text = big_text(vm, receiver(args));
    let b_text = big_text(vm, ref_arg(args, 1));
    let new_scale = int_arg(args, 2);
    let mode_ref = ref_arg(args, 3);
    if mode_ref.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let mode = rounding_mode_ordinal(vm, mode_ref);
    let (neg_a, int_a, frac_a) = decimal_parts(&a_text);
    let (neg_b, int_b, frac_b) = decimal_parts(&b_text);
    let sa = frac_a.len() as i32;
    let sb = frac_b.len() as i32;
    let ua = parse_i128(&alloc::format!("{}{}", int_a, frac_a));
    let ub = parse_i128(&alloc::format!("{}{}", int_b, frac_b));
    if let (Some(ua), Some(ub)) = (ua, ub) {
        if ub == 0 {
            return Err(vm.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
        }
        let ua = if neg_a { -ua } else { ua };
        let ub = if neg_b { -ub } else { ub };
        let shift = new_scale + sb - sa;
        let pow10 = |n: i32| 10i128.checked_pow(n as u32);
        let scaled = if shift >= 0 {
            pow10(shift)
                .and_then(|factor| ua.checked_mul(factor))
                .map(|num| (num, ub))
        } else {
            pow10(-shift)
                .and_then(|factor| ub.checked_mul(factor))
                .map(|den| (ua, den))
        };
        if let Some((num, den)) = scaled {
            let negative = (num < 0) != (den < 0);
            let num = num.unsigned_abs();
            let den = den.unsigned_abs();
            let mut quotient = num / den;
            let remainder = num % den;
            let twice = 2 * remainder;
            let round_up = match mode {
                0 => remainder != 0,
                1 => false,
                2 => !negative && remainder != 0,
                3 => negative && remainder != 0,
                4 => twice >= den,
                5 => twice > den,
                6 => twice > den || (twice == den && quotient % 2 == 1),
                _ => {
                    if remainder != 0 {
                        return Err(vm.throw_new(
                            "java/lang/ArithmeticException",
                            Some("Rounding necessary"),
                        ));
                    }
                    false
                }
            };
            if round_up {
                quotient += 1;
            }
            let mut value = quotient as i128;
            if negative {
                value = -value;
            }
            let mut scale = new_scale;
            if scale < 0 {
                if let Some(factor) = 10i128.checked_pow((-scale) as u32) {
                    if let Some(scaled_value) = value.checked_mul(factor) {
                        value = scaled_value;
                        scale = 0;
                    }
                }
            }
            if scale >= 0 {
                if let Some(text) = format_scaled(value, scale as usize) {
                    return create_big_number(vm, "java/math/BigDecimal", &text);
                }
            }
        }
    }
    let a = a_text.parse::<f64>().unwrap_or(f64::NAN);
    let b = b_text.parse::<f64>().unwrap_or(f64::NAN);
    if b == 0.0 {
        return Err(vm.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
    }
    let scale = new_scale.max(0) as usize;
    create_big_number(
        vm,
        "java/math/BigDecimal",
        &alloc::format!("{:.*}", scale, a / b),
    )
}

/// `BigDecimal.divideToIntegralValue` / `remainder` on aligned unscaled values.
fn big_decimal_div_rem(vm: &mut Vm<'_>, args: &[Value], integral: bool) -> Result<Value, VmError> {
    let a_raw = big_text(vm, receiver(args));
    let b_raw = big_text(vm, ref_arg(args, 1));
    let a_text = expand_decimal(&a_raw).unwrap_or(a_raw);
    let b_text = expand_decimal(&b_raw).unwrap_or(b_raw);
    let (neg_a, int_a, frac_a) = decimal_parts(&a_text);
    let (neg_b, int_b, frac_b) = decimal_parts(&b_text);
    let sa = frac_a.len();
    let sb = frac_b.len();
    let scale = sa.max(sb);
    let mut fa = frac_a;
    let mut fb = frac_b;
    while fa.len() < scale {
        fa.push('0');
    }
    while fb.len() < scale {
        fb.push('0');
    }
    let ua = parse_i128(&alloc::format!("{}{}", int_a, fa));
    let ub = parse_i128(&alloc::format!("{}{}", int_b, fb));
    if let (Some(ua), Some(ub)) = (ua, ub) {
        if ub == 0 {
            return Err(vm.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
        }
        let ua = if neg_a { -ua } else { ua };
        let ub = if neg_b { -ub } else { ub };
        let quotient = ua / ub;
        if integral {
            let preferred = sa as i32 - sb as i32;
            let mut value = quotient;
            let mut out_scale = 0usize;
            if preferred > 0 {
                out_scale = preferred as usize;
            } else if preferred < 0 {
                if let Some(factor) = 10i128.checked_pow((-preferred) as u32) {
                    if let Some(scaled_value) = value.checked_mul(factor) {
                        value = scaled_value;
                    }
                }
            }
            if let Some(text) = format_scaled(value, out_scale) {
                return create_big_number(vm, "java/math/BigDecimal", &text);
            }
        } else {
            let remainder = ua - quotient * ub;
            if let Some(text) = format_scaled(remainder, scale) {
                return create_big_number(vm, "java/math/BigDecimal", &text);
            }
        }
    }
    let a = a_text.parse::<f64>().unwrap_or(f64::NAN);
    let b = b_text.parse::<f64>().unwrap_or(f64::NAN);
    if b == 0.0 {
        return Err(vm.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
    }
    let raw = a / b;
    let quotient = if raw.abs() >= 4501111111170496.0 {
        raw
    } else {
        raw as i64 as f64
    };
    let result = if integral { quotient } else { a - quotient * b };
    create_big_number(vm, "java/math/BigDecimal", &alloc::format!("{}", result))
}

fn big_number_narrow(text: &str, bits: u32) -> i64 {
    // BigDecimal.intValue/longValue truncate the fraction toward zero, then keep the low
    // bits; a floating-point round trip would round values near 2^63 to the wrong integer.
    let integer = text.split('.').next().unwrap_or("0");
    big_integer_narrow(integer, bits)
}

/// The low `bits` bits of a decimal big-integer string as a signed narrowing
/// cast (`BigInteger.intValue`/`longValue`): exact for any magnitude, unlike a
/// floating-point round trip. Computed with wrapping modular arithmetic.
fn big_integer_narrow(text: &str, bits: u32) -> i64 {
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    if bits == 32 {
        let mut acc = 0u32;
        for byte in digits.bytes() {
            if let Some(digit) = (byte as char).to_digit(10) {
                acc = acc.wrapping_mul(10).wrapping_add(digit);
            }
        }
        let signed = if negative { acc.wrapping_neg() } else { acc };
        i64::from(signed as i32)
    } else {
        let mut acc = 0u64;
        for byte in digits.bytes() {
            if let Some(digit) = (byte as char).to_digit(10) {
                acc = acc.wrapping_mul(10).wrapping_add(u64::from(digit));
            }
        }
        let signed = if negative { acc.wrapping_neg() } else { acc };
        signed as i64
    }
}

// -------------------------------------------------------------------------------------------
// BigInteger
// -------------------------------------------------------------------------------------------

fn big_integer_init(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let descriptor = vm.classes.get(context.class).methods[context.method as usize]
        .descriptor
        .clone();
    let text = if descriptor == "(J)V" {
        long_arg(args, 1).to_string()
    } else {
        string_arg(vm, args[1])?
    };
    let Some(canonical) = big_integer_parse(&text) else {
        return Err(vm.throw_new("java/lang/NumberFormatException", None));
    };
    let string = vm.make_string(&canonical)?;
    let (declaring, field) = vm.find_field(context.class, "value", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, string);
    void()
}

fn big_integer_binary(
    vm: &mut Vm<'_>,
    args: &[Value],
    operation: fn(i128, i128) -> i128,
) -> Result<Value, VmError> {
    let this = receiver(args);
    if ref_arg(args, 1).is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let a = parse_i128(&big_text(vm, this));
    let b = parse_i128(&big_text(vm, ref_arg(args, 1)));
    match (a, b) {
        (Some(a), Some(b)) => {
            create_big_number(vm, "java/math/BigInteger", &operation(a, b).to_string())
        }
        _ => Err(vm.throw_new("java/lang/ArithmeticException", None)),
    }
}

/// Multiply modulo `m` without overflowing i128 (Russian-peasant doubling).
fn mul_mod(mut a: i128, mut b: i128, m: i128) -> i128 {
    let mut result = 0i128;
    a %= m;
    b %= m;
    while b > 0 {
        if b & 1 == 1 {
            result = (result + a) % m;
        }
        a = (a + a) % m;
        b >>= 1;
    }
    result
}

const BIG_INTEGER_METHODS: [oxjvm_vm::NativeMethodDef; 53] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        big_integer_init,
    ),
    method("<init>", "(J)V", ACC_PUBLIC, big_integer_init),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = big_text(vm, receiver(args));
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toString",
        "(I)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = big_decimal_to_radix(&big_text(vm, receiver(args)), int_arg(args, 1) as u32);
            object(vm.make_string(&text)?)
        },
    ),
    method("intValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(big_integer_narrow(&big_text(vm, receiver(args)), 32) as i32)
    }),
    method("longValue", "()J", ACC_PUBLIC, |vm, _, args| {
        long(big_integer_narrow(&big_text(vm, receiver(args)), 64))
    }),
    method("intValueExact", "()I", ACC_PUBLIC, |vm, _, args| {
        let value = big_decimal_exact_integer(vm, receiver(args), "BigInteger out of int range")?;
        if value >= i128::from(i32::MIN) && value <= i128::from(i32::MAX) {
            int(value as i32)
        } else {
            Err(vm.throw_new(
                "java/lang/ArithmeticException",
                Some("BigInteger out of int range"),
            ))
        }
    }),
    method("longValueExact", "()J", ACC_PUBLIC, |vm, _, args| {
        let value = big_decimal_exact_integer(vm, receiver(args), "BigInteger out of long range")?;
        if value >= i128::from(i64::MIN) && value <= i128::from(i64::MAX) {
            long(value as i64)
        } else {
            Err(vm.throw_new(
                "java/lang/ArithmeticException",
                Some("BigInteger out of long range"),
            ))
        }
    }),
    method("shortValueExact", "()S", ACC_PUBLIC, |vm, _, args| {
        let value = big_decimal_exact_integer(vm, receiver(args), "BigInteger out of short range")?;
        if value >= i128::from(i16::MIN) && value <= i128::from(i16::MAX) {
            int(value as i16 as i32)
        } else {
            Err(vm.throw_new(
                "java/lang/ArithmeticException",
                Some("BigInteger out of short range"),
            ))
        }
    }),
    method("byteValueExact", "()B", ACC_PUBLIC, |vm, _, args| {
        let value = big_decimal_exact_integer(vm, receiver(args), "BigInteger out of byte range")?;
        if value >= i128::from(i8::MIN) && value <= i128::from(i8::MAX) {
            int(value as i8 as i32)
        } else {
            Err(vm.throw_new(
                "java/lang/ArithmeticException",
                Some("BigInteger out of byte range"),
            ))
        }
    }),
    method("doubleValue", "()D", ACC_PUBLIC, |vm, _, args| {
        let text = big_text(vm, receiver(args));
        double(text.parse::<f64>().unwrap_or(f64::NAN))
    }),
    method("signum", "()I", ACC_PUBLIC, |vm, _, args| {
        let text = big_text(vm, receiver(args));
        int(if text == "0" {
            0
        } else if text.starts_with('-') {
            -1
        } else {
            1
        })
    }),
    method(
        "abs",
        "()Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = big_text(vm, receiver(args))
                .trim_start_matches('-')
                .to_string();
            create_big_number(vm, "java/math/BigInteger", &text)
        },
    ),
    method(
        "negate",
        "()Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = big_text(vm, receiver(args));
            let negated = match text.strip_prefix('-') {
                Some(rest) => String::from(rest),
                None if text == "0" => String::from("0"),
                None => alloc::format!("-{text}"),
            };
            create_big_number(vm, "java/math/BigInteger", &negated)
        },
    ),
    method(
        "compareTo",
        "(Ljava/math/BigInteger;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let a = big_text(vm, receiver(args));
            let b = big_text(vm, ref_arg(args, 1));
            int(big_integer_compare(&a, &b))
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(other) != vm.class_of(receiver(args)) {
                return boolean(false);
            }
            boolean(big_text(vm, other) == big_text(vm, receiver(args)))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(big_integer_hash(&big_text(vm, receiver(args))))
    }),
    method(
        "add",
        "(Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| big_integer_binary(vm, args, i128::wrapping_add),
    ),
    method(
        "subtract",
        "(Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| big_integer_binary(vm, args, i128::wrapping_sub),
    ),
    method(
        "multiply",
        "(Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| big_integer_binary(vm, args, i128::wrapping_mul),
    ),
    method(
        "valueOf",
        "(J)Ljava/math/BigInteger;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = long_arg(args, 0).to_string();
            create_big_number(vm, "java/math/BigInteger", &text)
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = string_arg(vm, args[1])?;
            let radix = int_arg(args, 2);
            let value = i128::from_str_radix(&text, radix as u32)
                .map_err(|_| vm.throw_new("java/lang/NumberFormatException", Some(&text)))?;
            write_big_text(vm, receiver(args), &value.to_string())
        },
    ),
    method(
        "<init>",
        "(ILjava/util/Random;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let bits = int_arg(args, 1).max(0) as u32;
            let random = ref_arg(args, 2);
            let (class, method) = vm.resolve_virtual_method(random, "nextLong", "()J")?;
            let value = vm
                .invoke_method(class, method, vec![Value::Ref(random)])?
                .as_long();
            let mask = if bits == 0 {
                0
            } else if bits >= 63 {
                i64::MAX
            } else {
                (1i64 << bits) - 1
            };
            write_big_text(vm, receiver(args), &(value & mask).to_string())
        },
    ),
    method("<init>", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        let text = twos_complement_to_decimal(&bytes);
        write_big_text(vm, receiver(args), &text)
    }),
    method("<init>", "(I[B)V", ACC_PUBLIC, |vm, _, args| {
        let signum = int_arg(args, 1);
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 2))?;
        let magnitude = limbs_to_decimal(bytes_to_limbs(&bytes));
        let text = if signum == 0 || magnitude == "0" {
            String::from("0")
        } else if signum < 0 {
            alloc::format!("-{magnitude}")
        } else {
            magnitude
        };
        write_big_text(vm, receiver(args), &text)
    }),
    method("toByteArray", "()[B", ACC_PUBLIC, |vm, _, args| {
        let value = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
        let bytes = i128_to_twos_complement(value);
        object(bytes_array(vm, &bytes)?)
    }),
    method(
        "shiftLeft",
        "(I)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = big_text(vm, receiver(args));
            let shift = int_arg(args, 1);
            match big_integer_shift(&text, shift, shift >= 0) {
                Some(shifted) => create_big_number(vm, "java/math/BigInteger", &shifted),
                None => Err(vm.throw_new("java/lang/OutOfMemoryError", Some("Java heap space"))),
            }
        },
    ),
    method(
        "shiftRight",
        "(I)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = big_text(vm, receiver(args));
            let shift = int_arg(args, 1);
            match big_integer_shift(&text, shift, shift < 0) {
                Some(shifted) => create_big_number(vm, "java/math/BigInteger", &shifted),
                None => Err(vm.throw_new("java/lang/OutOfMemoryError", Some("Java heap space"))),
            }
        },
    ),
    method(
        "pow",
        "(I)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
            let exponent = int_arg(args, 1);
            if exponent < 0 {
                return Err(
                    vm.throw_new("java/lang/ArithmeticException", Some("negative exponent"))
                );
            }
            let powered = value.checked_pow(exponent as u32).unwrap_or(0);
            create_big_number(vm, "java/math/BigInteger", &powered.to_string())
        },
    ),
    method(
        "divide",
        "(Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let a = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
            let b = parse_i128(&big_text(vm, ref_arg(args, 1))).unwrap_or(0);
            if b == 0 {
                return Err(vm.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
            }
            create_big_number(vm, "java/math/BigInteger", &(a / b).to_string())
        },
    ),
    method(
        "remainder",
        "(Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let a = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
            let b = parse_i128(&big_text(vm, ref_arg(args, 1))).unwrap_or(0);
            if b == 0 {
                return Err(vm.throw_new(
                    "java/lang/ArithmeticException",
                    Some("BigInteger divide by zero"),
                ));
            }
            create_big_number(vm, "java/math/BigInteger", &(a % b).to_string())
        },
    ),
    method("bitLength", "()I", ACC_PUBLIC, |vm, _, args| {
        let value = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
        let magnitude = if value < 0 { !value } else { value } as u128;
        int(if magnitude == 0 {
            0
        } else {
            (128 - magnitude.leading_zeros()) as i32
        })
    }),
    method("longValueExact", "()J", ACC_PUBLIC, |vm, _, args| {
        let value = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
        if let Ok(value) = i64::try_from(value) {
            long(value)
        } else {
            Err(vm.throw_new("java/lang/ArithmeticException", Some("overflow")))
        }
    }),
    method("isProbablePrime", "(I)Z", ACC_PUBLIC, |vm, _, args| {
        let value = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
        boolean(is_probable_prime(value))
    }),
    method(
        "gcd",
        "(Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let a = parse_i128(&big_text(vm, receiver(args)));
            let b = parse_i128(&big_text(vm, ref_arg(args, 1)));
            match (a, b) {
                (Some(mut a), Some(mut b)) => {
                    a = a.abs();
                    b = b.abs();
                    while b != 0 {
                        let t = a % b;
                        a = b;
                        b = t;
                    }
                    create_big_number(vm, "java/math/BigInteger", &a.to_string())
                }
                _ => Err(vm.throw_new("java/lang/ArithmeticException", None)),
            }
        },
    ),
    method(
        "modPow",
        "(Ljava/math/BigInteger;Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let base = parse_i128(&big_text(vm, receiver(args)));
            let exponent = parse_i128(&big_text(vm, ref_arg(args, 1)));
            let modulus = parse_i128(&big_text(vm, ref_arg(args, 2)));
            match (base, exponent, modulus) {
                (Some(base), Some(mut exponent), Some(modulus)) => {
                    if modulus <= 0 {
                        return Err(vm.throw_new(
                            "java/lang/ArithmeticException",
                            Some("modulus not positive"),
                        ));
                    }
                    if exponent < 0 {
                        return Err(vm.throw_new(
                            "java/lang/ArithmeticException",
                            Some("negative exponent"),
                        ));
                    }
                    let mut result = 1i128 % modulus;
                    let mut b = base.rem_euclid(modulus);
                    while exponent > 0 {
                        if exponent & 1 == 1 {
                            result = mul_mod(result, b, modulus);
                        }
                        b = mul_mod(b, b, modulus);
                        exponent >>= 1;
                    }
                    create_big_number(vm, "java/math/BigInteger", &result.to_string())
                }
                _ => Err(vm.throw_new("java/lang/ArithmeticException", None)),
            }
        },
    ),
    method(
        "mod",
        "(Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let a = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
            let b = parse_i128(&big_text(vm, ref_arg(args, 1))).unwrap_or(0);
            if b <= 0 {
                return Err(vm.throw_new(
                    "java/lang/ArithmeticException",
                    Some("BigInteger: modulus not positive"),
                ));
            }
            create_big_number(vm, "java/math/BigInteger", &a.rem_euclid(b).to_string())
        },
    ),
    method(
        "modInverse",
        "(Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let a = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
            let m = parse_i128(&big_text(vm, ref_arg(args, 1))).unwrap_or(0);
            if m <= 0 {
                return Err(vm.throw_new(
                    "java/lang/ArithmeticException",
                    Some("BigInteger: modulus not positive"),
                ));
            }
            let (mut old_r, mut r) = (a.rem_euclid(m), m);
            let (mut old_s, mut s) = (1i128, 0i128);
            while r != 0 {
                let q = old_r / r;
                (old_r, r) = (r, old_r - q * r);
                (old_s, s) = (s, old_s - q * s);
            }
            if old_r != 1 {
                return Err(vm.throw_new(
                    "java/lang/ArithmeticException",
                    Some("BigInteger not invertible"),
                ));
            }
            create_big_number(vm, "java/math/BigInteger", &old_s.rem_euclid(m).to_string())
        },
    ),
    method("testBit", "(I)Z", ACC_PUBLIC, |vm, _, args| {
        let value = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
        let n = int_arg(args, 1);
        if n < 0 {
            return Err(vm.throw_new(
                "java/lang/ArithmeticException",
                Some("Negative bit address"),
            ));
        }
        boolean(if n >= 128 {
            value < 0
        } else {
            (value >> n) & 1 != 0
        })
    }),
    method(
        "setBit",
        "(I)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
            let n = int_arg(args, 1);
            if !(0..127).contains(&n) {
                return Err(vm.throw_new(
                    "java/lang/ArithmeticException",
                    Some("bit index out of range"),
                ));
            }
            create_big_number(
                vm,
                "java/math/BigInteger",
                &(value | (1i128 << n)).to_string(),
            )
        },
    ),
    method(
        "clearBit",
        "(I)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
            let n = int_arg(args, 1);
            if !(0..127).contains(&n) {
                return Err(vm.throw_new(
                    "java/lang/ArithmeticException",
                    Some("bit index out of range"),
                ));
            }
            create_big_number(
                vm,
                "java/math/BigInteger",
                &(value & !(1i128 << n)).to_string(),
            )
        },
    ),
    method(
        "flipBit",
        "(I)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
            let n = int_arg(args, 1);
            if !(0..127).contains(&n) {
                return Err(vm.throw_new(
                    "java/lang/ArithmeticException",
                    Some("bit index out of range"),
                ));
            }
            create_big_number(
                vm,
                "java/math/BigInteger",
                &(value ^ (1i128 << n)).to_string(),
            )
        },
    ),
    method("bitCount", "()I", ACC_PUBLIC, |vm, _, args| {
        let value = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
        let magnitude = if value < 0 { !value } else { value };
        int(magnitude.count_ones() as i32)
    }),
    method(
        "and",
        "(Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| big_integer_binary(vm, args, |a, b| a & b),
    ),
    method(
        "or",
        "(Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| big_integer_binary(vm, args, |a, b| a | b),
    ),
    method(
        "xor",
        "(Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| big_integer_binary(vm, args, |a, b| a ^ b),
    ),
    method(
        "not",
        "()Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
            create_big_number(vm, "java/math/BigInteger", &(!value).to_string())
        },
    ),
    method(
        "divideAndRemainder",
        "(Ljava/math/BigInteger;)[Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let a = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
            let b = parse_i128(&big_text(vm, ref_arg(args, 1))).unwrap_or(0);
            if b == 0 {
                return Err(vm.throw_new(
                    "java/lang/ArithmeticException",
                    Some("BigInteger divide by zero"),
                ));
            }
            let class = vm.resolve_class("java/math/BigInteger")?;
            let array = vm.allocate_object_array(class, 2)?;
            let quotient = create_big_number(vm, "java/math/BigInteger", &(a / b).to_string())?;
            let remainder = create_big_number(vm, "java/math/BigInteger", &(a % b).to_string())?;
            vm.array_set_ref(array, 0, quotient.as_ref())?;
            vm.array_set_ref(array, 1, remainder.as_ref())?;
            object(array)
        },
    ),
    method(
        "sqrt",
        "()Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
            if value < 0 {
                return Err(
                    vm.throw_new("java/lang/ArithmeticException", Some("Negative BigInteger"))
                );
            }
            let root = isqrt_u128(value as u128) as i128;
            create_big_number(vm, "java/math/BigInteger", &root.to_string())
        },
    ),
    method(
        "max",
        "(Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let a = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
            let b = parse_i128(&big_text(vm, ref_arg(args, 1))).unwrap_or(0);
            create_big_number(vm, "java/math/BigInteger", &a.max(b).to_string())
        },
    ),
    method(
        "min",
        "(Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let a = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
            let b = parse_i128(&big_text(vm, ref_arg(args, 1))).unwrap_or(0);
            create_big_number(vm, "java/math/BigInteger", &a.min(b).to_string())
        },
    ),
    method(
        "probablePrime",
        "(ILjava/util/Random;)Ljava/math/BigInteger;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let bit_length = int_arg(args, 0);
            let rnd = ref_arg(args, 1);
            if rnd.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            if !(2..=127).contains(&bit_length) {
                return Err(vm.throw_new(
                    "java/lang/ArithmeticException",
                    Some("bitLength must be in [2, 127]"),
                ));
            }
            for _ in 0..4096 {
                let mut candidate = 0i128;
                let mut remaining = bit_length;
                while remaining > 0 {
                    let take = remaining.min(31);
                    let bits =
                        i128::from(crate::util::next_random_int(vm, rnd, take, &[])?.as_int());
                    candidate = (candidate << take) | (bits & ((1i128 << take) - 1));
                    remaining -= take;
                }
                candidate |= 1i128 << (bit_length - 1);
                candidate |= 1;
                if is_probable_prime(candidate) {
                    return create_big_number(vm, "java/math/BigInteger", &candidate.to_string());
                }
            }
            Err(vm.throw_new("java/lang/ArithmeticException", Some("no prime found")))
        },
    ),
    method(
        "sqrtAndRemainder",
        "()[Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = parse_i128(&big_text(vm, receiver(args))).unwrap_or(0);
            if value < 0 {
                return Err(
                    vm.throw_new("java/lang/ArithmeticException", Some("Negative BigInteger"))
                );
            }
            let root = isqrt_u128(value as u128) as i128;
            let remainder = value - root * root;
            let class = vm.resolve_class("java/math/BigInteger")?;
            let array = vm.allocate_object_array(class, 2)?;
            let root_obj = create_big_number(vm, "java/math/BigInteger", &root.to_string())?;
            let rem_obj = create_big_number(vm, "java/math/BigInteger", &remainder.to_string())?;
            vm.array_set_ref(array, 0, root_obj.as_ref())?;
            vm.array_set_ref(array, 1, rem_obj.as_ref())?;
            object(array)
        },
    ),
];

fn big_integer_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/math/BigInteger")?;
    for (name, text) in [("ZERO", "0"), ("ONE", "1"), ("TWO", "2"), ("TEN", "10")] {
        let value = create_big_number(vm, "java/math/BigInteger", text)?;
        vm.set_static_value(class, name, "Ljava/math/BigInteger;", value);
    }
    Ok(())
}

/// `java.math.BigInteger`; values are carried as canonical decimal text.
pub(crate) const BIG_INTEGER: oxjvm_vm::NativeClass = class(
    "java/math/BigInteger",
    Some("java/lang/Number"),
    &["java/lang/Comparable"],
    ACC_PUBLIC,
    &[
        field("value", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "ZERO",
            "Ljava/math/BigInteger;",
            ACC_PUBLIC | ACC_STATIC,
            None,
        ),
        field(
            "ONE",
            "Ljava/math/BigInteger;",
            ACC_PUBLIC | ACC_STATIC,
            None,
        ),
        field(
            "TWO",
            "Ljava/math/BigInteger;",
            ACC_PUBLIC | ACC_STATIC,
            None,
        ),
        field(
            "TEN",
            "Ljava/math/BigInteger;",
            ACC_PUBLIC | ACC_STATIC,
            None,
        ),
    ],
    &BIG_INTEGER_METHODS,
    Some(big_integer_clinit),
);

// -------------------------------------------------------------------------------------------
// BigDecimal
// -------------------------------------------------------------------------------------------

fn big_decimal_init(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let descriptor = vm.classes.get(context.class).methods[context.method as usize]
        .descriptor
        .clone();
    let text = match descriptor.as_str() {
        "(J)V" => long_arg(args, 1).to_string(),
        "(I)V" => int_arg(args, 1).to_string(),
        "(D)V" => {
            let value = double_arg(args, 1);
            if value.is_nan() || value.is_infinite() {
                return Err(vm.throw_new("java/lang/NumberFormatException", None));
            }
            alloc::format!("{value}")
        }
        "(Ljava/math/BigInteger;)V" => big_text(vm, ref_arg(args, 1)),
        _ => string_arg(vm, args[1])?,
    };
    if let Some((len, scale)) = decimal_plain_len(&text) {
        if scale >= 0 && len > 100_000_000 {
            return Err(vm.throw_new("java/lang/OutOfMemoryError", Some("Java heap space")));
        }
    }
    let Some(canonical) = big_decimal_parse(&text) else {
        return Err(vm.throw_new("java/lang/NumberFormatException", None));
    };
    let string = vm.make_string(&canonical)?;
    let (declaring, field) = vm.find_field(context.class, "value", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, string);
    void()
}

/// `BigDecimal.add`/`subtract` preserving the larger operand scale, using exact
/// i128 arithmetic on the unscaled values when they fit (falling back to the
/// floating-point path otherwise).
/// `BigDecimal.multiply` preserving the sum of operand scales, using exact i128
/// arithmetic on the unscaled values when they fit.
fn big_decimal_multiply(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let a = big_text(vm, receiver(args));
    let b = big_text(vm, ref_arg(args, 1));
    let (neg_a, int_a, frac_a) = decimal_parts(&a);
    let (neg_b, int_b, frac_b) = decimal_parts(&b);
    let scale = frac_a.len() + frac_b.len();
    let ua = alloc::format!("{}{}", int_a, frac_a);
    let ub = alloc::format!("{}{}", int_b, frac_b);
    if let (Some(va), Some(vb)) = (parse_i128(&ua), parse_i128(&ub)) {
        let va = if neg_a { -va } else { va };
        let vb = if neg_b { -vb } else { vb };
        if let Some(product) = va.checked_mul(vb) {
            if let Some(text) = format_scaled(product, scale) {
                return create_big_number(vm, "java/math/BigDecimal", &text);
            }
        }
    }
    let digits = limbs_to_decimal(limbs_mul(&decimal_to_limbs(&ua), &decimal_to_limbs(&ub)));
    let text = format_scaled_digits(&digits, neg_a != neg_b, scale);
    create_big_number(vm, "java/math/BigDecimal", &text)
}

/// Round a decimal string to `precision` significant digits using `mode` (RoundingMode ordinal).
/// Apply a MathContext's precision and rounding mode to a BigDecimal value
/// text; UNLIMITED (precision 0) leaves the value untouched.
fn apply_math_context(vm: &mut Vm<'_>, text: &str, mc: ObjectRef) -> Result<String, VmError> {
    if mc.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let precision = crate::lang::read_int_field(vm, mc, "precision");
    if precision == 0 {
        return Ok(String::from(text));
    }
    let mode_ref = vm
        .read_ref_field(
            mc,
            vm.class_of(mc),
            "roundingMode",
            "Ljava/math/RoundingMode;",
        )
        .unwrap_or(ObjectRef::NULL);
    let mode = rounding_mode_ordinal(vm, mode_ref);
    round_to_precision(text, precision, mode)
        .ok_or_else(|| vm.throw_new("java/lang/ArithmeticException", Some("Rounding necessary")))
}

/// Round a decimal value text to `precision` significant digits using the
/// given RoundingMode ordinal. Pure digit-string arithmetic, so values
/// beyond i128 (e.g. a 38-digit unscaled value with scale 177) round
/// correctly instead of overflowing the parse.
fn round_to_precision(text: &str, precision: i32, mode: i32) -> Option<String> {
    if precision <= 0 {
        return Some(String::from(text));
    }
    let (negative, int_part, frac_part) = decimal_parts(text);
    let digits = alloc::format!("{}{}", int_part, frac_part);
    let scale = frac_part.len() as i32;
    let sig_start = digits
        .bytes()
        .position(|b| b != b'0')
        .unwrap_or(digits.len());
    let current_sig = (digits.len() - sig_start) as i32;
    if current_sig <= precision {
        return Some(String::from(text));
    }
    let sig = &digits[sig_start..];
    let keep = precision as usize;
    let dropped = &sig[keep..];
    let first_drop = dropped.as_bytes()[0];
    let rest_nonzero = dropped[1..].bytes().any(|b| b != b'0');
    let any_nonzero = first_drop != b'0' || rest_nonzero;
    let round_up = match mode {
        0 => any_nonzero,
        1 => false,
        2 => !negative && any_nonzero,
        3 => negative && any_nonzero,
        4 => first_drop >= b'5',
        5 => first_drop > b'5' || (first_drop == b'5' && rest_nonzero),
        6 => {
            first_drop > b'5'
                || (first_drop == b'5' && (rest_nonzero || sig.as_bytes()[keep - 1] % 2 != 0))
        }
        _ => {
            if any_nonzero {
                return None;
            }
            false
        }
    };
    let mut kept = String::from(&sig[..keep]);
    let mut drop = current_sig - precision;
    if round_up {
        kept = dec_add_one(&kept);
        if kept.len() > keep {
            // 999.. -> 1000..0: carry into a new exponent, keep p digits.
            kept.pop();
            drop += 1;
        }
    }
    let new_scale = scale - drop;
    let rendered = if new_scale > 0 {
        if kept.len() as i32 <= new_scale {
            let zeros = (new_scale - kept.len() as i32) as usize;
            alloc::format!("0.{}{}", "0".repeat(zeros), kept)
        } else {
            let point = kept.len() as i32 - new_scale;
            alloc::format!("{}.{}", &kept[..point as usize], &kept[point as usize..])
        }
    } else if new_scale == 0 {
        kept
    } else {
        big_decimal_with_scale(&kept, new_scale)
    };
    let mut result = String::new();
    if negative {
        result.push('-');
    }
    result.push_str(&rendered);
    Some(result)
}

/// Divide two decimal strings to `precision` significant digits via i128 long division.
fn divide_to_precision(a: &str, b: &str, precision: i32, mode: i32) -> Option<String> {
    let (neg_a, int_a, frac_a) = decimal_parts(a);
    let (neg_b, int_b, frac_b) = decimal_parts(b);
    let num = parse_i128(&alloc::format!("{}{}", int_a, frac_a))?;
    let den = parse_i128(&alloc::format!("{}{}", int_b, frac_b))?;
    if den == 0 {
        return None;
    }
    let negative = neg_a != neg_b;
    let num = num.abs();
    let den = den.abs();
    let sa = frac_a.len() as i32;
    let sb = frac_b.len() as i32;
    let int_q = num / den;
    let mut rem = num % den;
    let mut frac = String::new();
    let limit = (precision + 20).max(20) as usize;
    while frac.len() < limit && rem != 0 {
        rem = rem.checked_mul(10)?;
        let digit = rem / den;
        frac.push(char::from(b'0' + digit as u8));
        rem %= den;
    }
    let mut raw = alloc::format!("{}", int_q);
    if !frac.is_empty() {
        raw.push('.');
        raw.push_str(&frac);
    }
    let nonzero = int_q != 0 || frac.bytes().any(|b| b != b'0');
    if negative && nonzero {
        raw = alloc::format!("-{raw}");
    }
    let shifted = scale_decimal(&raw, sb - sa);
    round_to_precision(&shifted, precision, mode)
}

/// A cached `RoundingMode` enum constant by name.
fn rounding_mode_constant(vm: &mut Vm<'_>, name: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/math/RoundingMode")?;
    vm.ensure_initialized(class)?;
    vm.read_static_ref_if_set(class, name, "Ljava/math/RoundingMode;")
        .ok_or_else(|| VmError::internal("RoundingMode constant missing"))
}

fn big_decimal_add_sub(vm: &mut Vm<'_>, args: &[Value], subtract: bool) -> Result<Value, VmError> {
    let a = big_text(vm, receiver(args));
    let b = big_text(vm, ref_arg(args, 1));
    let (neg_a, int_a, frac_a) = decimal_parts(&a);
    let (neg_b, int_b, frac_b) = decimal_parts(&b);
    let scale = frac_a.len().max(frac_b.len());
    let mut fa = frac_a;
    let mut fb = frac_b;
    while fa.len() < scale {
        fa.push('0');
    }
    while fb.len() < scale {
        fb.push('0');
    }
    let ua = alloc::format!("{}{}", int_a, fa);
    let ub = alloc::format!("{}{}", int_b, fb);
    if let (Some(va), Some(vb)) = (parse_i128(&ua), parse_i128(&ub)) {
        let va = if neg_a { -va } else { va };
        let vb = if neg_b { -vb } else { vb };
        let result = if subtract {
            va.checked_sub(vb)
        } else {
            va.checked_add(vb)
        };
        if let Some(value) = result {
            if let Some(text) = format_scaled(value, scale) {
                return create_big_number(vm, "java/math/BigDecimal", &text);
            }
        }
    }
    let la = decimal_to_limbs(&ua);
    let lb = decimal_to_limbs(&ub);
    let sign_b = neg_b ^ subtract;
    let (magnitude, negative) = if neg_a == sign_b {
        (limbs_add(&la, &lb), neg_a)
    } else {
        match limbs_cmp(&la, &lb) {
            Ordering::Equal => (Vec::new(), false),
            Ordering::Greater => (limbs_sub(&la, &lb), neg_a),
            Ordering::Less => (limbs_sub(&lb, &la), sign_b),
        }
    };
    let digits = limbs_to_decimal(magnitude);
    let text = format_scaled_digits(&digits, negative, scale);
    create_big_number(vm, "java/math/BigDecimal", &text)
}

/// Split a decimal string into (negative, integer digits, fraction digits).
/// Shared check for BigDecimal.{int,long,short,byte}ValueExact and
/// toBigIntegerExact: the fraction must be all zeros, then the integer part
/// is parsed exactly (E notation expanded first).
fn big_decimal_exact_integer(vm: &mut Vm<'_>, this: ObjectRef, msg: &str) -> Result<i128, VmError> {
    let raw = big_text(vm, this);
    let (sign, _, _) = bd_decompose(&raw);
    if sign == 0 {
        return Ok(0);
    }
    let scale = decimal_scale(&raw);
    if scale < -38 {
        return Err(vm.throw_new("java/lang/ArithmeticException", Some("Overflow")));
    }
    if scale > 10_000 {
        return Err(vm.throw_new("java/lang/ArithmeticException", Some("Rounding necessary")));
    }
    let text = plain_text(&raw);
    let (negative, int_part, frac_part) = decimal_parts(&text);
    if !frac_part.bytes().all(|byte| byte == b'0') {
        return Err(vm.throw_new("java/lang/ArithmeticException", Some("Rounding necessary")));
    }
    let digits = if negative {
        alloc::format!("-{int_part}")
    } else {
        int_part
    };
    digits
        .parse::<i128>()
        .map_err(|_| vm.throw_new("java/lang/ArithmeticException", Some(msg)))
}

/// Plain (non-scientific) decimal form of a value text, expanding the `E`
/// notation that negative-scale BigDecimal values carry.
fn plain_text(text: &str) -> String {
    if text.contains(['e', 'E']) {
        expand_decimal(text).unwrap_or_else(|| String::from(text))
    } else {
        String::from(text)
    }
}

fn decimal_parts(text: &str) -> (bool, String, String) {
    let text = &plain_text(text);
    let (negative, digits) = if let Some(stripped) = text.strip_prefix('-') {
        (true, stripped)
    } else {
        (false, text.strip_prefix('+').unwrap_or(text))
    };
    let (int_part, frac_part) = match digits.split_once('.') {
        Some((i, f)) => (i, f),
        None => (digits, ""),
    };
    (negative, int_part.to_string(), frac_part.to_string())
}

/// `BigDecimal.setScale`: rescale the decimal string, rounding dropped digits.
fn big_decimal_set_scale(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    new_scale: i32,
    mode: i32,
) -> Result<Value, VmError> {
    if new_scale < 0 {
        return Err(vm.throw_new("java/lang/ArithmeticException", Some("Negative scale")));
    }
    let text = big_text(vm, this);
    let (negative, int_part, frac_part) = decimal_parts(&text);
    let new_scale = new_scale as usize;
    let mut int_digits = int_part;
    let mut frac_digits = frac_part;
    if new_scale >= frac_digits.len() {
        while frac_digits.len() < new_scale {
            frac_digits.push('0');
        }
    } else {
        let dropped = &frac_digits[new_scale..];
        let kept = frac_digits[..new_scale].to_string();
        let nonzero = dropped.bytes().any(|b| b != b'0');
        let round_up = match mode {
            0 => nonzero,
            1 => false,
            2 => !negative && nonzero,
            3 => negative && nonzero,
            4..=6 => {
                let first = dropped.as_bytes()[0];
                if first > b'5' {
                    true
                } else if first < b'5' {
                    false
                } else {
                    let rest_nonzero = dropped[1..].bytes().any(|b| b != b'0');
                    match mode {
                        4 => true,
                        5 => rest_nonzero,
                        _ => {
                            let last_kept = kept.bytes().last().unwrap_or(b'0');
                            rest_nonzero || last_kept % 2 == 1
                        }
                    }
                }
            }
            _ => {
                if nonzero {
                    return Err(
                        vm.throw_new("java/lang/ArithmeticException", Some("Rounding necessary"))
                    );
                }
                false
            }
        };
        frac_digits = kept;
        if round_up {
            let mut carry = true;
            let mut frac: Vec<u8> = frac_digits.into_bytes();
            for index in (0..frac.len()).rev() {
                if carry {
                    if frac[index] == b'9' {
                        frac[index] = b'0';
                    } else {
                        frac[index] += 1;
                        carry = false;
                    }
                }
            }
            frac_digits = String::from_utf8(frac).unwrap_or_default();
            if carry {
                let mut int_bytes: Vec<u8> = int_digits.into_bytes();
                for index in (0..int_bytes.len()).rev() {
                    if carry {
                        if int_bytes[index] == b'9' {
                            int_bytes[index] = b'0';
                        } else {
                            int_bytes[index] += 1;
                            carry = false;
                        }
                    }
                }
                if carry {
                    int_bytes.insert(0, b'1');
                }
                int_digits = String::from_utf8(int_bytes).unwrap_or_default();
            }
        }
    }
    let mut result = String::new();
    if negative && (int_digits.bytes().any(|b| b != b'0') || frac_digits.bytes().any(|b| b != b'0'))
    {
        result.push('-');
    }
    result.push_str(&int_digits);
    if new_scale > 0 {
        result.push('.');
        result.push_str(&frac_digits);
    }
    create_big_number(vm, "java/math/BigDecimal", &result)
}

const BIG_DECIMAL_METHODS: [oxjvm_vm::NativeMethodDef; 60] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        big_decimal_init,
    ),
    method("<init>", "(J)V", ACC_PUBLIC, big_decimal_init),
    method("<init>", "(I)V", ACC_PUBLIC, big_decimal_init),
    method("<init>", "(D)V", ACC_PUBLIC, big_decimal_init),
    method(
        "<init>",
        "(Ljava/math/BigInteger;)V",
        ACC_PUBLIC,
        big_decimal_init,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = big_text(vm, receiver(args));
            object(vm.make_string(&text)?)
        },
    ),
    method("intValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(big_number_narrow(&plain_text(&big_text(vm, receiver(args))), 32) as i32)
    }),
    method("longValue", "()J", ACC_PUBLIC, |vm, _, args| {
        long(big_number_narrow(
            &plain_text(&big_text(vm, receiver(args))),
            64,
        ))
    }),
    method("doubleValue", "()D", ACC_PUBLIC, |vm, _, args| {
        let text = big_text(vm, receiver(args));
        double(text.parse::<f64>().unwrap_or(f64::NAN))
    }),
    method("floatValue", "()F", ACC_PUBLIC, |vm, _, args| {
        let text = big_text(vm, receiver(args));
        float(text.parse::<f32>().unwrap_or(f32::NAN))
    }),
    method("intValueExact", "()I", ACC_PUBLIC, |vm, _, args| {
        let value = big_decimal_exact_integer(vm, receiver(args), "BigInteger out of int range")?;
        if value >= i128::from(i32::MIN) && value <= i128::from(i32::MAX) {
            int(value as i32)
        } else {
            Err(vm.throw_new(
                "java/lang/ArithmeticException",
                Some("BigInteger out of int range"),
            ))
        }
    }),
    method("longValueExact", "()J", ACC_PUBLIC, |vm, _, args| {
        let value = big_decimal_exact_integer(vm, receiver(args), "BigInteger out of long range")?;
        if value >= i128::from(i64::MIN) && value <= i128::from(i64::MAX) {
            long(value as i64)
        } else {
            Err(vm.throw_new(
                "java/lang/ArithmeticException",
                Some("BigInteger out of long range"),
            ))
        }
    }),
    method("shortValueExact", "()S", ACC_PUBLIC, |vm, _, args| {
        let value = big_decimal_exact_integer(vm, receiver(args), "BigInteger out of short range")?;
        if value >= i128::from(i16::MIN) && value <= i128::from(i16::MAX) {
            int(value as i16 as i32)
        } else {
            Err(vm.throw_new(
                "java/lang/ArithmeticException",
                Some("BigInteger out of short range"),
            ))
        }
    }),
    method("byteValueExact", "()B", ACC_PUBLIC, |vm, _, args| {
        let value = big_decimal_exact_integer(vm, receiver(args), "BigInteger out of byte range")?;
        if value >= i128::from(i8::MIN) && value <= i128::from(i8::MAX) {
            int(value as i8 as i32)
        } else {
            Err(vm.throw_new(
                "java/lang/ArithmeticException",
                Some("BigInteger out of byte range"),
            ))
        }
    }),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(other) != vm.class_of(receiver(args)) {
                return boolean(false);
            }
            boolean(big_text(vm, other) == big_text(vm, receiver(args)))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(java_hash(&big_text(vm, receiver(args))))
    }),
    method(
        "compareTo",
        "(Ljava/math/BigDecimal;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let a = big_text(vm, receiver(args));
            let b = big_text(vm, ref_arg(args, 1));
            int(match compare_big_decimal_text(&a, &b) {
                Ordering::Less => -1,
                Ordering::Equal => 0,
                Ordering::Greater => 1,
            })
        },
    ),
    method(
        "add",
        "(Ljava/math/BigDecimal;)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| big_decimal_add_sub(vm, args, false),
    ),
    method(
        "subtract",
        "(Ljava/math/BigDecimal;)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| big_decimal_add_sub(vm, args, true),
    ),
    method(
        "multiply",
        "(Ljava/math/BigDecimal;)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| big_decimal_multiply(vm, args),
    ),
    method(
        "valueOf",
        "(J)Ljava/math/BigDecimal;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = long_arg(args, 0).to_string();
            create_big_number(vm, "java/math/BigDecimal", &text)
        },
    ),
    method(
        "<init>",
        "(Ljava/math/BigInteger;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = big_text(vm, ref_arg(args, 1));
            let scale = int_arg(args, 2);
            let scaled = big_decimal_with_scale(&text, scale);
            write_big_text(vm, receiver(args), &scaled)
        },
    ),
    method(
        "<init>",
        "(Ljava/math/BigInteger;ILjava/math/MathContext;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = big_text(vm, ref_arg(args, 1));
            let scale = int_arg(args, 2);
            let scaled = big_decimal_with_scale(&text, scale);
            let rounded = apply_math_context(vm, &scaled, ref_arg(args, 3))?;
            write_big_text(vm, receiver(args), &rounded)
        },
    ),
    method(
        "<init>",
        "(JLjava/math/MathContext;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = long_arg(args, 1).to_string();
            let rounded = apply_math_context(vm, &text, ref_arg(args, 2))?;
            write_big_text(vm, receiver(args), &rounded)
        },
    ),
    method(
        "<init>",
        "(ILjava/math/MathContext;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = int_arg(args, 1).to_string();
            let rounded = apply_math_context(vm, &text, ref_arg(args, 2))?;
            write_big_text(vm, receiver(args), &rounded)
        },
    ),
    method(
        "<init>",
        "(DLjava/math/MathContext;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = double_arg(args, 1);
            if value.is_nan() || value.is_infinite() {
                return Err(vm.throw_new("java/lang/NumberFormatException", None));
            }
            let text = alloc::format!("{value}");
            let rounded = apply_math_context(vm, &text, ref_arg(args, 2))?;
            write_big_text(vm, receiver(args), &rounded)
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/math/MathContext;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = string_arg(vm, args[1])?;
            if let Some((len, scale)) = decimal_plain_len(&text) {
                if scale >= 0 && len > 100_000_000 {
                    return Err(vm.throw_new("java/lang/OutOfMemoryError", Some("Java heap space")));
                }
            }
            let Some(canonical) = big_decimal_parse(&text) else {
                return Err(vm.throw_new("java/lang/NumberFormatException", None));
            };
            let rounded = apply_math_context(vm, &canonical, ref_arg(args, 2))?;
            write_big_text(vm, receiver(args), &rounded)
        },
    ),
    method(
        "valueOf",
        "(JI)Ljava/math/BigDecimal;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = long_arg(args, 0).to_string();
            let scale = int_arg(args, 1);
            let scaled = big_decimal_with_scale(&text, scale);
            create_big_number(vm, "java/math/BigDecimal", &scaled)
        },
    ),
    method(
        "valueOf",
        "(D)Ljava/math/BigDecimal;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = alloc::format!("{}", double_arg(args, 0));
            create_big_number(vm, "java/math/BigDecimal", &text)
        },
    ),
    method(
        "toPlainString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let raw = big_text(vm, receiver(args));
            if let Some((len, _)) = decimal_plain_len(&raw) {
                if len > 100_000_000 {
                    return Err(vm.throw_new("java/lang/OutOfMemoryError", Some("Java heap space")));
                }
            }
            let text = plain_text(&raw);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toBigInteger",
        "()Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = plain_text(&big_text(vm, receiver(args)));
            let integer = text.split('.').next().unwrap_or("0");
            create_big_number(vm, "java/math/BigInteger", integer)
        },
    ),
    method(
        "toBigIntegerExact",
        "()Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let raw = big_text(vm, this);
            let (sign, _, _) = bd_decompose(&raw);
            if sign != 0 {
                let scale = decimal_scale(&raw);
                if scale < -38 {
                    return Err(vm.throw_new("java/lang/ArithmeticException", Some("Overflow")));
                }
                if scale > 10_000 {
                    return Err(
                        vm.throw_new("java/lang/ArithmeticException", Some("Rounding necessary"))
                    );
                }
            }
            let text = plain_text(&raw);
            let (negative, int_part, frac_part) = decimal_parts(&text);
            if !frac_part.bytes().all(|byte| byte == b'0') {
                return Err(
                    vm.throw_new("java/lang/ArithmeticException", Some("Rounding necessary"))
                );
            }
            let integer = if negative && int_part != "0" {
                alloc::format!("-{int_part}")
            } else {
                int_part
            };
            create_big_number(vm, "java/math/BigInteger", &integer)
        },
    ),
    method("signum", "()I", ACC_PUBLIC, |vm, _, args| {
        let text = big_text(vm, receiver(args));
        let value = text.parse::<f64>().unwrap_or(0.0);
        int(if value < 0.0 {
            -1
        } else if value > 0.0 {
            1
        } else {
            0
        })
    }),
    method(
        "scaleByPowerOfTen",
        "(I)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = big_text(vm, receiver(args));
            let scaled = scale_decimal(&text, int_arg(args, 1));
            create_big_number(vm, "java/math/BigDecimal", &scaled)
        },
    ),
    method("precision", "()I", ACC_PUBLIC, |vm, _, args| {
        let text = plain_text(&big_text(vm, receiver(args)));
        int(text
            .bytes()
            .filter(|byte| byte.is_ascii_digit())
            .count()
            .max(1) as i32)
    }),
    method(
        "negate",
        "()Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = big_text(vm, receiver(args));
            let negated = match text.strip_prefix('-') {
                Some(rest) => String::from(rest),
                None if plain_text(&text) == "0" => text,
                None => alloc::format!("-{text}"),
            };
            create_big_number(vm, "java/math/BigDecimal", &negated)
        },
    ),
    method(
        "divide",
        "(Ljava/math/BigDecimal;)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let a = big_text(vm, receiver(args))
                .parse::<f64>()
                .unwrap_or(f64::NAN);
            let b = big_text(vm, ref_arg(args, 1))
                .parse::<f64>()
                .unwrap_or(f64::NAN);
            if b == 0.0 {
                return Err(vm.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
            }
            create_big_number(vm, "java/math/BigDecimal", &alloc::format!("{}", a / b))
        },
    ),
    method(
        "setScale",
        "(I)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| big_decimal_set_scale(vm, receiver(args), int_arg(args, 1), 7),
    ),
    method(
        "setScale",
        "(ILjava/math/RoundingMode;)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let mode = ref_arg(args, 2);
            if mode.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            big_decimal_set_scale(
                vm,
                receiver(args),
                int_arg(args, 1),
                rounding_mode_ordinal(vm, mode),
            )
        },
    ),
    method(
        "stripTrailingZeros",
        "()Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = big_text(vm, this);
            let (negative, int_part, frac_part) = decimal_parts(&text);
            let all_zero =
                int_part.bytes().all(|b| b == b'0') && frac_part.bytes().all(|b| b == b'0');
            if all_zero {
                return create_big_number(vm, "java/math/BigDecimal", "0");
            }
            let mut result = String::new();
            if negative {
                result.push('-');
            }
            if frac_part.is_empty() {
                let trimmed = int_part.trim_end_matches('0');
                let zeros = int_part.len() - trimmed.len();
                if zeros > 0 {
                    result.push_str(if trimmed.is_empty() { "0" } else { trimmed });
                    result.push_str(&alloc::format!("E+{zeros}"));
                } else {
                    result.push_str(&int_part);
                }
            } else {
                let trimmed = frac_part.trim_end_matches('0');
                result.push_str(&int_part);
                if !trimmed.is_empty() {
                    result.push('.');
                    result.push_str(trimmed);
                }
            }
            create_big_number(vm, "java/math/BigDecimal", &result)
        },
    ),
    method(
        "divide",
        "(Ljava/math/BigDecimal;Ljava/math/RoundingMode;)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let scale = decimal_scale(&big_text(vm, this));
            let scaled_args = [
                Value::Ref(this),
                Value::Ref(ref_arg(args, 1)),
                Value::Int(scale),
                Value::Ref(ref_arg(args, 2)),
            ];
            big_decimal_divide_scaled(vm, &scaled_args)
        },
    ),
    method(
        "divide",
        "(Ljava/math/BigDecimal;ILjava/math/RoundingMode;)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| big_decimal_divide_scaled(vm, args),
    ),
    method(
        "movePointLeft",
        "(I)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let n = int_arg(args, 1);
            let text = big_text(vm, this);
            if n == 0 && decimal_scale(&text) >= 0 {
                return object(this);
            }
            let moved = big_decimal_move_point(&text, n.wrapping_neg());
            create_big_number(vm, "java/math/BigDecimal", &moved)
        },
    ),
    method(
        "movePointRight",
        "(I)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let n = int_arg(args, 1);
            let text = big_text(vm, this);
            if n == 0 && decimal_scale(&text) >= 0 {
                return object(this);
            }
            let moved = big_decimal_move_point(&text, n);
            create_big_number(vm, "java/math/BigDecimal", &moved)
        },
    ),
    method(
        "pow",
        "(I)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let n = int_arg(args, 1);
            if n < 0 {
                return Err(
                    vm.throw_new("java/lang/ArithmeticException", Some("Invalid Operation"))
                );
            }
            let text = big_text(vm, receiver(args));
            let (negative, int_part, frac_part) = decimal_parts(&text);
            let scale = frac_part.len() as i32;
            if let Some(value) = parse_i128(&alloc::format!("{}{}", int_part, frac_part)) {
                let value = if negative { -value } else { value };
                if let Some(powered) = value.checked_pow(n as u32) {
                    if let Some(new_scale) = scale.checked_mul(n) {
                        if new_scale >= 0 {
                            if let Some(text) = format_scaled(powered, new_scale as usize) {
                                return create_big_number(vm, "java/math/BigDecimal", &text);
                            }
                        }
                    }
                }
            }
            let mut base = text.parse::<f64>().unwrap_or(f64::NAN);
            let mut exponent = n;
            let mut result = 1.0f64;
            while exponent > 0 {
                if exponent & 1 == 1 {
                    result *= base;
                }
                base *= base;
                exponent >>= 1;
            }
            create_big_number(vm, "java/math/BigDecimal", &alloc::format!("{}", result))
        },
    ),
    method(
        "unscaledValue",
        "()Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let raw = big_text(vm, receiver(args));
            let text = expand_decimal(&raw).unwrap_or(raw);
            let (negative, int_part, frac_part) = decimal_parts(&text);
            let digits = alloc::format!("{}{}", int_part, frac_part);
            let trimmed = digits.trim_start_matches('0');
            let mut result = String::new();
            if negative && !trimmed.is_empty() {
                result.push('-');
            }
            result.push_str(if trimmed.is_empty() { "0" } else { trimmed });
            create_big_number(vm, "java/math/BigInteger", &result)
        },
    ),
    method("scale", "()I", ACC_PUBLIC, |vm, _, args| {
        int(decimal_scale(&big_text(vm, receiver(args))))
    }),
    method(
        "abs",
        "()Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = big_text(vm, receiver(args));
            let stripped = text.strip_prefix('-').unwrap_or(&text);
            create_big_number(vm, "java/math/BigDecimal", stripped)
        },
    ),
    method(
        "divideToIntegralValue",
        "(Ljava/math/BigDecimal;)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| big_decimal_div_rem(vm, args, true),
    ),
    method(
        "remainder",
        "(Ljava/math/BigDecimal;)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| big_decimal_div_rem(vm, args, false),
    ),
    method(
        "round",
        "(Ljava/math/MathContext;)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = big_text(vm, receiver(args));
            let rounded = apply_math_context(vm, &text, ref_arg(args, 1))?;
            create_big_number(vm, "java/math/BigDecimal", &rounded)
        },
    ),
    method(
        "divide",
        "(Ljava/math/BigDecimal;Ljava/math/MathContext;)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let divisor = ref_arg(args, 1);
            let mc = ref_arg(args, 2);
            if mc.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let precision = crate::lang::read_int_field(vm, mc, "precision");
            let mode_ref = vm
                .read_ref_field(
                    mc,
                    vm.class_of(mc),
                    "roundingMode",
                    "Ljava/math/RoundingMode;",
                )
                .unwrap_or(ObjectRef::NULL);
            let mode = rounding_mode_ordinal(vm, mode_ref);
            let a = big_text(vm, this);
            let b = big_text(vm, divisor);
            let bval = b.parse::<f64>().unwrap_or(0.0);
            if bval == 0.0 {
                return Err(vm.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
            }
            if precision == 0 {
                let av = a.parse::<f64>().unwrap_or(f64::NAN);
                return create_big_number(
                    vm,
                    "java/math/BigDecimal",
                    &alloc::format!("{}", av / bval),
                );
            }
            match divide_to_precision(&a, &b, precision, mode) {
                Some(result) => create_big_number(vm, "java/math/BigDecimal", &result),
                None => {
                    Err(vm.throw_new("java/lang/ArithmeticException", Some("Rounding necessary")))
                }
            }
        },
    ),
    method(
        "min",
        "(Ljava/math/BigDecimal;)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let a = big_text(vm, this).parse::<f64>().unwrap_or(f64::NAN);
            let b = big_text(vm, other).parse::<f64>().unwrap_or(f64::NAN);
            object(if a <= b { this } else { other })
        },
    ),
    method(
        "max",
        "(Ljava/math/BigDecimal;)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let a = big_text(vm, this).parse::<f64>().unwrap_or(f64::NAN);
            let b = big_text(vm, other).parse::<f64>().unwrap_or(f64::NAN);
            object(if a >= b { this } else { other })
        },
    ),
    method(
        "ulp",
        "()Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = big_text(vm, receiver(args));
            let (_, _, frac) = decimal_parts(&text);
            match format_scaled(1, frac.len()) {
                Some(value) => create_big_number(vm, "java/math/BigDecimal", &value),
                None => Err(vm.throw_new("java/lang/ArithmeticException", None)),
            }
        },
    ),
    method(
        "plus",
        "()Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "plus",
        "(Ljava/math/MathContext;)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = big_text(vm, receiver(args));
            let rounded = apply_math_context(vm, &text, ref_arg(args, 1))?;
            create_big_number(vm, "java/math/BigDecimal", &rounded)
        },
    ),
    method(
        "negate",
        "(Ljava/math/MathContext;)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = big_text(vm, receiver(args));
            let negated = match text.strip_prefix('-') {
                Some(rest) => String::from(rest),
                None if plain_text(&text) == "0" => text,
                None => alloc::format!("-{text}"),
            };
            let rounded = apply_math_context(vm, &negated, ref_arg(args, 1))?;
            create_big_number(vm, "java/math/BigDecimal", &rounded)
        },
    ),
    method(
        "abs",
        "(Ljava/math/MathContext;)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = big_text(vm, receiver(args));
            let stripped = text.strip_prefix('-').unwrap_or(&text);
            let rounded = apply_math_context(vm, stripped, ref_arg(args, 1))?;
            create_big_number(vm, "java/math/BigDecimal", &rounded)
        },
    ),
    method(
        "divideAndRemainder",
        "(Ljava/math/BigDecimal;)[Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let quotient = big_decimal_div_rem(vm, args, true)?;
            let remainder = big_decimal_div_rem(vm, args, false)?;
            let class = vm.resolve_class("java/math/BigDecimal")?;
            let array = vm.allocate_object_array(class, 2)?;
            vm.array_set_ref(array, 0, quotient.as_ref())?;
            vm.array_set_ref(array, 1, remainder.as_ref())?;
            object(array)
        },
    ),
];

fn big_decimal_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/math/BigDecimal")?;
    for (name, text) in [("ZERO", "0"), ("ONE", "1"), ("TEN", "10")] {
        let value = create_big_number(vm, "java/math/BigDecimal", text)?;
        vm.set_static_value(class, name, "Ljava/math/BigDecimal;", value);
    }
    Ok(())
}

/// `java.math.BigDecimal`; values are carried as canonical decimal text.
pub(crate) const BIG_DECIMAL: oxjvm_vm::NativeClass = class(
    "java/math/BigDecimal",
    Some("java/lang/Number"),
    &["java/lang/Comparable"],
    ACC_PUBLIC,
    &[
        field("value", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "ZERO",
            "Ljava/math/BigDecimal;",
            ACC_PUBLIC | ACC_STATIC,
            None,
        ),
        field(
            "ONE",
            "Ljava/math/BigDecimal;",
            ACC_PUBLIC | ACC_STATIC,
            None,
        ),
        field(
            "TEN",
            "Ljava/math/BigDecimal;",
            ACC_PUBLIC | ACC_STATIC,
            None,
        ),
    ],
    &BIG_DECIMAL_METHODS,
    Some(big_decimal_clinit),
);
#[cfg(test)]
mod tests {
    use super::sqrt;

    #[test]
    fn sqrt_matches_ieee_reference() {
        // (input bits, correctly rounded result bits); results cross-checked against
        // `Math.sqrt` on OpenJDK 25 (Temurin 25.0.4.1).
        let cases: [(u64, u64); 29] = [
            (0x0000_0000_0000_0000, 0x0000_0000_0000_0000), // +0.0
            (0x8000_0000_0000_0000, 0x8000_0000_0000_0000), // -0.0
            (0x3ff0_0000_0000_0000, 0x3ff0_0000_0000_0000), // 1.0
            (0x4000_0000_0000_0000, 0x3ff6_a09e_667f_3bcd), // 2.0
            (0x4008_0000_0000_0000, 0x3ffb_b67a_e858_4caa), // 3.0
            (0x4010_0000_0000_0000, 0x4000_0000_0000_0000), // 4.0
            (0x4014_0000_0000_0000, 0x4001_e377_9b97_f4a8), // 5.0
            (0x3fd0_0000_0000_0000, 0x3fe0_0000_0000_0000), // 0.25
            (0x4202_a05f_2000_0000, 0x40f8_6a00_0000_0000), // 1.0e10
            (0x3ddb_7cdf_d9d7_bdbb, 0x3ee4_f8b5_88e3_68f1), // 1.0e-10
            (0x0000_0000_0000_0001, 0x1e60_0000_0000_0000), // MIN_VALUE (subnormal)
            (0x0000_0000_0000_0002, 0x1e66_a09e_667f_3bcd), // 2 * MIN_VALUE
            (0x0010_0000_0000_0000, 0x2000_0000_0000_0000), // MIN_POSITIVE
            (0x7fef_ffff_ffff_ffff, 0x5fef_ffff_ffff_ffff), // MAX_VALUE
            (0x4000_0000_0000_0001, 0x3ff6_a09e_667f_3bcd), // just above 2
            (0x3ff0_0000_0000_0001, 0x3ff0_0000_0000_0000), // just above 1
            (0x3fef_ffff_ffff_ffff, 0x3fef_ffff_ffff_ffff), // just below 1
            (0x4330_0000_0000_0000, 0x4190_0000_0000_0000), // 2^52
            (0x4330_0000_0000_0001, 0x4190_0000_0000_0000),
            (0x4330_0000_8000_0000, 0x4190_0000_3fff_ff80),
            (0x40bf_f000_0000_0000, 0x4056_9af5_89b3_5963),
            (0x40f8_69f0_0000_0000, 0x4073_c39e_7407_cea8),
            (0x4080_0000_0000_0001, 0x4036_a09e_667f_3bcd),
            (0x3ff6_a09e_667f_3bcd, 0x3ff3_06fe_0a31_b715),
            (0x3fe6_a09e_667f_3bcd, 0x3fea_e89f_995a_d3ae),
            (0x0123_4567_89ab_cdef, 0x2088_d53c_68b0_12b2),
            (0x0fed_cba9_8765_4321, 0x27ee_e0cb_ec79_2028),
            (0x7ff0_0000_0000_0000, 0x7ff0_0000_0000_0000), // +infinity
            (0xbff0_0000_0000_0000, 0x7ff8_0000_0000_0000), // -1.0 -> NaN
        ];
        for (input, expected) in cases {
            let actual = sqrt(f64::from_bits(input));
            assert_eq!(
                actual.to_bits(),
                expected,
                "sqrt({input:#018x}) = {:#018x}, expected {expected:#018x}",
                actual.to_bits()
            );
        }
    }

    #[test]
    fn sqrt_matches_hardware_sqrt_on_random_inputs() {
        // The test host's `f64::sqrt` is the IEEE-754 hardware instruction, so it provides
        // an independent oracle for a wide sweep of normal and subnormal inputs.
        let mut state = 0x1234_5678_9abc_def0_u64;
        for _ in 0..100_000 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let x = f64::from_bits(state);
            if x.is_nan() || x < 0.0 || x.is_infinite() {
                continue;
            }
            assert_eq!(
                sqrt(x).to_bits(),
                x.sqrt().to_bits(),
                "sqrt({:#018x}) disagrees",
                x.to_bits()
            );
        }
    }
}

// -------------------------------------------------------------------------------------------
