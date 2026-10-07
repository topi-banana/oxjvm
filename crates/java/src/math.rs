//! `java.lang.Math` and `java.lang.StrictMath`, implemented without libm.
//!
//! `sqrt` is correctly rounded (integer square root plus round-to-nearest-even). The
//! transcendental functions use argument reduction and polynomial kernels; they are accurate to
//! roughly one unit in the last place over the ranges tests exercise, which matches the contract
//! of `Math` (as opposed to `StrictMath`, whose bit-for-bit reproducibility oxjvm does not claim).

use core::cmp::Ordering;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{Value, Vm, VmError};

use crate::{
    class, double, double_arg, field, float, float_arg, int, int_arg, long, long_arg, method,
};

// -------------------------------------------------------------------------------------------
// Floating-point kernels
// -------------------------------------------------------------------------------------------

const LN2_HI: f64 = core::f64::consts::LN_2;
const LN2_LO: f64 = 1.908_214_929_269_071e-11;
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
    ln(x) / core::f64::consts::LN_10
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
    if x == 0.0 {
        return if y > 0.0 { 0.0 } else { f64::INFINITY };
    }
    if x < 0.0 && (y - floor(y)) != 0.0 {
        return f64::NAN;
    }
    let negative = x < 0.0 && (y as i64) % 2 != 0;
    let result = exp(y * ln(x.abs()));
    if negative { -result } else { result }
}

/// `atan`.
pub(crate) fn atan(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
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
    if !x.is_finite() || x == 0.0 {
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
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let truncated = x as i64 as f64;
    if x > 0.0 && truncated != x {
        truncated + 1.0
    } else {
        truncated
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
    if fraction > 0.5 || (fraction == 0.5 && !even) {
        lower + 1.0
    } else {
        lower
    }
}

/// `Math.round(double)`.
pub(crate) fn round(x: f64) -> i64 {
    floor(x + 0.5) as i64
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
    x - quotient * y
}

pub(crate) fn next_after(start: f64, direction: f64) -> f64 {
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
                int(floor(f64::from(float_arg(args, 0)) + 0.5) as i32)
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
                |_, _, args| {
                    int(floor_div(i64::from(int_arg(args, 0)), i64::from(int_arg(args, 1))) as i32)
                },
            ),
            method(
                "floorDiv",
                "(JJ)J",
                ACC_PUBLIC | ACC_STATIC,
                |_, _, args| long(floor_div(long_arg(args, 0), long_arg(args, 1))),
            ),
            method(
                "floorMod",
                "(II)I",
                ACC_PUBLIC | ACC_STATIC,
                |_, _, args| {
                    int(floor_mod(i64::from(int_arg(args, 0)), i64::from(int_arg(args, 1))) as i32)
                },
            ),
            method(
                "floorMod",
                "(JJ)J",
                ACC_PUBLIC | ACC_STATIC,
                |_, _, args| long(floor_mod(long_arg(args, 0), long_arg(args, 1))),
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
        ]
    }};
}

const MATH_METHODS: [oxjvm_vm::NativeMethodDef; 51] = math_methods!();
const STRICT_MATH_METHODS: [oxjvm_vm::NativeMethodDef; 51] = math_methods!();

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
