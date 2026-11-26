//! `java.lang.Math` and `java.lang.StrictMath`, implemented without libm.
//!
//! `sqrt` is correctly rounded (integer square root plus round-to-nearest-even). `sin`, `cos`
//! and `tan` are a faithful port of fdlibm and reproduce the JDK bit-for-bit. The remaining
//! transcendental functions use argument reduction and polynomial kernels; they are accurate to
//! roughly one unit in the last place over the ranges tests exercise, which matches the contract
//! of `Math` (as opposed to `StrictMath`, whose bit-for-bit reproducibility oxjvm does not claim
//! for those).

use core::cmp::Ordering;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{
    class, double, double_arg, field, float, float_arg, int, int_arg, long, long_arg, method,
};

// -------------------------------------------------------------------------------------------
// Floating-point kernels
// -------------------------------------------------------------------------------------------

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

// Trigonometric functions: a faithful port of Sun's fdlibm, the same code the
// JDK runs. The three-part reduction for medium arguments and the Payne-Hanek
// reduction against 24-bit chunks of 2/pi for large ones reproduce the JDK's
// results bit-for-bit, including the worst-case arguments (up to 2^1023) that
// the jtreg trigonometry tests exercise.

/// The high 32 bits of a double, as a signed word (fdlibm's `__HI`).
fn hi_word(x: f64) -> i32 {
    (x.to_bits() >> 32) as i32
}

/// The value with its low 32 bits cleared (fdlibm's `__LO(x) = 0`).
fn lo_zero(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & 0xffff_ffff_0000_0000)
}

/// fdlibm idiom: `x - x` yields NaN for an infinity or NaN input.
#[allow(clippy::eq_op)]
fn inf_nan(x: f64) -> f64 {
    x - x
}

/// fdlibm idiom: `x + x` propagates NaN and keeps an infinity's sign.
#[allow(clippy::eq_op)]
fn inf_add(x: f64) -> f64 {
    x + x
}

/// fdlibm idiom: `(x-x)/(x-x)` yields NaN. On the JDK, finite operands
/// give 0/0 = the negative indefinite NaN, while NaN operands propagate
/// their sign and payload.
#[allow(clippy::eq_op)]
fn nan_nan(x: f64) -> f64 {
    let d = x - x;
    d / d
}

/// The value with its high 32 bits replaced (fdlibm's `__HI(x, v)`).
fn set_hi_word(x: f64, hi: i32) -> f64 {
    f64::from_bits(((hi as u32 as u64) << 32) | (x.to_bits() & 0xffff_ffff))
}

// fdlibm's documented decimal constants; the hex comments are the authoritative
// bit patterns, verified against the JDK's FdlibmTranslit.
#[allow(clippy::approx_constant)]
const INVPIO2: f64 = 0.636_619_772_367_581_4; // 0x3FE45F30, 0x6DC9C883
const PIO2_1: f64 = 1.570_796_326_734_125_6; // 0x3FF921FB, 0x54400000
const PIO2_1T: f64 = 6.077_100_506_506_192e-11; // 0x3DD0B461, 0x1A626331
const PIO2_2: f64 = 6.077_100_506_303_966e-11; // 0x3DD0B461, 0x1A600000
const PIO2_2T: f64 = 2.022_266_248_795_950_6e-21; // 0x3BA3198A, 0x2E037073
const PIO2_3: f64 = 2.022_266_248_711_166_5e-21; // 0x3BA3198A, 0x2E000000
const PIO2_3T: f64 = 8.478_427_660_368_9e-32; // 0x397B839A, 0x252049C1
const TWO24: f64 = 16_777_216.0; // 0x41700000, 0x00000000
#[allow(clippy::excessive_precision)]
const TWON24: f64 = 5.960_464_477_539_062_5e-8; // 0x3E700000, 0x00000000

/// High words of `n * pi/2` for `n` in `1..=32`, used to detect cancellation.
const NPIO2_HW: [i32; 32] = [
    0x3FF921FB, 0x400921FB, 0x4012D97C, 0x401921FB, 0x401F6A7A, 0x4022D97C, 0x4025FDBB, 0x402921FB,
    0x402C463A, 0x402F6A7A, 0x4031475C, 0x4032D97C, 0x40346B9C, 0x4035FDBB, 0x40378FDB, 0x403921FB,
    0x403AB41B, 0x403C463A, 0x403DD85A, 0x403F6A7A, 0x40407E4C, 0x4041475C, 0x4042106C, 0x4042D97C,
    0x4043A28C, 0x40446B9C, 0x404534AC, 0x4045FDBB, 0x4046C6CB, 0x40478FDB, 0x404858EB, 0x404921FB,
];

/// The first 66 24-bit chunks of the fractional part of 2/pi. Enough for every
/// finite double: the largest exponent needs fewer than 52 chunks.
const TWO_OVER_PI: [i32; 66] = [
    0xA2F983, 0x6E4E44, 0x1529FC, 0x2757D1, 0xF534DD, 0xC0DB62, 0x95993C, 0x439041, 0xFE5163,
    0xABDEBB, 0xC561B7, 0x246E3A, 0x424DD2, 0xE00649, 0x2EEA09, 0xD1921C, 0xFE1DEB, 0x1CB129,
    0xA73EE8, 0x8235F5, 0x2EBB44, 0x84E99C, 0x7026B4, 0x5F7E41, 0x3991D6, 0x398353, 0x39F49C,
    0x845F8B, 0xBDF928, 0x3B1FF8, 0x97FFDE, 0x05980F, 0xEF2F11, 0x8B5A0A, 0x6D1F6D, 0x367ECF,
    0x27CB09, 0xB74F46, 0x3F669E, 0x5FEA2D, 0x7527BA, 0xC7EBE5, 0xF17B3D, 0x0739F7, 0x8A5292,
    0xEA6BFB, 0x5FB11F, 0x8D5D08, 0x560330, 0x46FC7B, 0x6BABF0, 0xCFBC20, 0x9AF436, 0x1DA9E3,
    0x91615E, 0xE61B08, 0x659985, 0x5F14A0, 0x68408D, 0xFFD880, 0x4D7327, 0x310606, 0x1556CA,
    0x73A8C9, 0x60E27B, 0xC08C6B,
];

/// `pi/2` cut into 24-bit chunks, for the Payne-Hanek recombination.
const PIO2_CHUNKS: [f64; 8] = [
    1.570_796_251_296_997,
    7.549_789_415_861_596e-8,
    5.390_302_529_957_765e-15,
    3.282_003_415_807_913e-22,
    1.270_655_753_080_676e-29,
    1.229_333_089_811_113_3e-36,
    2.733_700_538_164_645_6e-44,
    2.167_416_838_778_048_2e-51,
];

/// `sin` (fdlibm).
pub(crate) fn sin(x: f64) -> f64 {
    let ix = hi_word(x) & 0x7fff_ffff;
    // |x| ~< pi/4.
    if ix <= 0x3fe9_21fb {
        return kernel_sin(x, 0.0, false);
    }
    // sin(Inf or NaN) is NaN.
    if ix >= 0x7ff0_0000 {
        return inf_nan(x);
    }
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kernel_sin(y0, y1, true),
        1 => kernel_cos(y0, y1),
        2 => -kernel_sin(y0, y1, true),
        _ => -kernel_cos(y0, y1),
    }
}

/// `cos` (fdlibm).
pub(crate) fn cos(x: f64) -> f64 {
    let ix = hi_word(x) & 0x7fff_ffff;
    if ix <= 0x3fe9_21fb {
        return kernel_cos(x, 0.0);
    }
    if ix >= 0x7ff0_0000 {
        return inf_nan(x);
    }
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kernel_cos(y0, y1),
        1 => -kernel_sin(y0, y1, true),
        2 => -kernel_cos(y0, y1),
        _ => kernel_sin(y0, y1, true),
    }
}

/// `tan` (fdlibm).
pub(crate) fn tan(x: f64) -> f64 {
    let ix = hi_word(x) & 0x7fff_ffff;
    if ix <= 0x3fe9_21fb {
        return kernel_tan(x, 0.0, 1);
    }
    if ix >= 0x7ff0_0000 {
        return inf_nan(x);
    }
    let (n, y0, y1) = rem_pio2(x);
    // 1 when n is even (tan), -1 when odd (-1/tan).
    kernel_tan(y0, y1, 1 - ((n & 1) << 1))
}

/// Kernel sin on `[-pi/4, pi/4]`; `y` is the tail of the reduced argument and
/// `iy` tells whether that tail is present.
fn kernel_sin(x: f64, y: f64, iy: bool) -> f64 {
    const S1: f64 = -0.166_666_666_666_666_32; // 0xBFC55555, 0x55555549
    const S2: f64 = 0.008_333_333_333_322_49; // 0x3F811111, 0x1110F8A6
    const S3: f64 = -0.000_198_412_698_298_579_5; // 0xBF2A01A0, 0x19C161D5
    #[allow(clippy::excessive_precision)]
    const S4: f64 = 2.755_731_370_707_006_767_89e-6; // 0x3EC71DE3, 0x57B1FE7D
    const S5: f64 = -2.505_076_025_340_686_3e-8; // 0xBE5AE5E6, 0x8A2B9CEB
    const S6: f64 = 1.589_690_995_211_55e-10; // 0x3DE5D93A, 0x5ACFD57C
    let ix = hi_word(x) & 0x7fff_ffff;
    // |x| < 2^-27: sin(x) = x (preserves the sign of zero).
    if ix < 0x3e40_0000 {
        return x;
    }
    let z = x * x;
    let v = z * x;
    let r = S2 + z * (S3 + z * (S4 + z * (S5 + z * S6)));
    if !iy {
        x + v * (S1 + z * r)
    } else {
        x - ((z * (0.5 * y - v * r) - y) - v * S1)
    }
}

/// Kernel cos on `[-pi/4, pi/4]`; `y` is the tail of the reduced argument.
fn kernel_cos(x: f64, y: f64) -> f64 {
    const C1: f64 = 0.041_666_666_666_666_6; // 0x3FA55555, 0x5555554C
    const C2: f64 = -0.001_388_888_888_887_411; // 0xBF56C16C, 0x16C15177
    const C3: f64 = 2.480_158_728_947_673e-5; // 0x3EFA01A0, 0x19CB1590
    const C4: f64 = -2.755_731_435_139_066_3e-7; // 0xBE927E4F, 0x809C52AD
    const C5: f64 = 2.087_572_321_298_175e-9; // 0x3E21EE9E, 0xBDB4B1C4
    const C6: f64 = -1.135_964_755_778_819_5e-11; // 0xBDA8FAE9, 0xBE8838D4
    let ix = hi_word(x) & 0x7fff_ffff;
    // |x| < 2^-27: cos(x) = 1.
    if ix < 0x3e40_0000 {
        return 1.0;
    }
    let z = x * x;
    let r = z * (C1 + z * (C2 + z * (C3 + z * (C4 + z * (C5 + z * C6)))));
    if ix < 0x3fd3_3333 {
        // |x| < 0.3.
        1.0 - (0.5 * z - (z * r - x * y))
    } else {
        // qx = |x|/4 with the low 32 bits masked off, or 0.28125 near pi/4;
        // both 1-qx and x*x/2-qx are then exact.
        let qx = if ix > 0x3fe9_0000 {
            0.28125
        } else {
            f64::from_bits(((ix - 0x0020_0000) as u64) << 32)
        };
        let hz = 0.5 * z - qx;
        let a = 1.0 - qx;
        a - (hz - (z * r - x * y))
    }
}

/// Kernel tan on `[-pi/4, pi/4]`; `iy` is 1 for tan and -1 for -1/tan.
fn kernel_tan(x: f64, y: f64, iy: i32) -> f64 {
    #[allow(clippy::approx_constant)]
    const PIO4: f64 = 0.785_398_163_397_448_3; // 0x3FE921FB, 0x54442D18
    const PIO4LO: f64 = 3.061_616_997_868_383e-17; // 0x3C81A626, 0x33145C07
    const T: [f64; 13] = [
        0.333_333_333_333_334_1,
        0.133_333_333_333_201_24,
        0.053_968_253_976_226_05,
        0.021_869_488_294_859_542,
        0.008_863_239_823_599_3,
        0.003_592_079_107_591_312_4,
        0.001_456_209_454_325_290_3,
        0.000_588_041_240_820_264_1,
        0.000_246_463_134_818_469_9,
        7.817_944_429_395_571e-5,
        7.140_724_913_826_082e-5,
        -1.855_863_748_552_754_6e-5,
        2.590_730_518_636_337e-5,
    ];
    let hx = hi_word(x);
    let ix = hx & 0x7fff_ffff;
    let mut x = x;
    let mut y = y;
    // |x| < 2^-28.
    if ix < 0x3e30_0000 {
        if (ix | (x.to_bits() as u32 as i32) | (iy + 1)) == 0 {
            // x is zero and we want -1/tan: an infinity.
            return 1.0 / x.abs();
        }
        if iy == 1 {
            return x;
        }
        // -1/(x+y), carefully.
        let w = x + y;
        let z = lo_zero(w);
        let v = y - (z - x);
        let a = -1.0 / w;
        let t = lo_zero(a);
        let s = 1.0 + t * z;
        return t + a * (s + t * v);
    }
    // |x| >= 0.6744: tan(x) = 1 - 2*(tan(pi/4-x) - tan(pi/4-x)^2/(1+tan(pi/4-x))).
    let large = ix >= 0x3fe5_9428;
    if large {
        if hx < 0 {
            x = -x;
            y = -y;
        }
        let z = PIO4 - x;
        let w = PIO4LO - y;
        x = z + w;
        y = 0.0;
    }
    let z = x * x;
    let w = z * z;
    // Split the odd polynomial into even and odd coefficient halves.
    let r = T[1] + w * (T[3] + w * (T[5] + w * (T[7] + w * (T[9] + w * T[11]))));
    let v = z * (T[2] + w * (T[4] + w * (T[6] + w * (T[8] + w * (T[10] + w * T[12])))));
    let s = z * x;
    let mut r = y + z * (s * (r + v) + y);
    r += T[0] * s;
    let w = x + r;
    if large {
        let v = f64::from(iy);
        return f64::from(1 - ((hx >> 30) & 2)) * (v - 2.0 * (x - (w * w / (w + v) - r)));
    }
    if iy == 1 {
        return w;
    }
    // -1/(x+r), accurately.
    let z = lo_zero(w);
    let v = r - (z - x);
    let a = -1.0 / w;
    let t = lo_zero(a);
    let s = 1.0 + t * z;
    t + a * (s + t * v)
}

/// fdlibm `__ieee754_rem_pio2`: reduce `x` to `y0 + y1 = x - n*pi/2` with
/// `|y0 + y1| <= pi/4`, returning `n` (only `n & 3` matters to the callers).
fn rem_pio2(x: f64) -> (i32, f64, f64) {
    let hx = hi_word(x);
    let ix = hx & 0x7fff_ffff;
    // |x| ~<= pi/4: no reduction needed.
    if ix <= 0x3fe9_21fb {
        return (0, x, 0.0);
    }
    // |x| < 3pi/4: special case with n = +-1.
    if ix < 0x4002_d97c {
        if hx > 0 {
            let mut z = x - PIO2_1;
            if ix != 0x3ff9_21fb {
                // 33+53 bits of pi are good enough.
                let y0 = z - PIO2_1T;
                return (1, y0, (z - y0) - PIO2_1T);
            }
            // Near pi/2: use 33+33+53 bits of pi.
            z -= PIO2_2;
            let y0 = z - PIO2_2T;
            return (1, y0, (z - y0) - PIO2_2T);
        }
        let mut z = x + PIO2_1;
        if ix != 0x3ff9_21fb {
            let y0 = z + PIO2_1T;
            return (-1, y0, (z - y0) + PIO2_1T);
        }
        z += PIO2_2;
        let y0 = z + PIO2_2T;
        return (-1, y0, (z - y0) + PIO2_2T);
    }
    // Medium size: |x| ~<= 2^19 * (pi/2).
    if ix <= 0x4139_21fb {
        let t = x.abs();
        let n = (t * INVPIO2 + 0.5) as i32;
        let fn_ = f64::from(n);
        let mut r = t - fn_ * PIO2_1;
        let mut w = fn_ * PIO2_1T; // first round good to 85 bits
        let mut y0 = r - w;
        if n >= 32 || ix == NPIO2_HW[(n - 1) as usize] {
            let j = ix >> 20;
            let mut i = j - ((hi_word(y0) >> 20) & 0x7ff);
            if i > 16 {
                // Second iteration needed, good to 118 bits.
                let t = r;
                w = fn_ * PIO2_2;
                r = t - w;
                w = fn_ * PIO2_2T - ((t - r) - w);
                y0 = r - w;
                i = j - ((hi_word(y0) >> 20) & 0x7ff);
                if i > 49 {
                    // Third iteration needed, 151 bits of accuracy.
                    let t = r;
                    w = fn_ * PIO2_3;
                    r = t - w;
                    w = fn_ * PIO2_3T - ((t - r) - w);
                    y0 = r - w;
                }
            }
        }
        let y1 = (r - y0) - w;
        return if hx < 0 { (-n, -y0, -y1) } else { (n, y0, y1) };
    }
    // Inf or NaN.
    if ix >= 0x7ff0_0000 {
        return (0, inf_nan(x), inf_nan(x));
    }
    // All other (large) arguments: break |x| into three 24-bit chunks and run
    // the Payne-Hanek kernel.
    let e0 = (ix >> 20) - 1046; // ilogb(|x|) - 23
    let mut z =
        f64::from_bits((((ix - (e0 << 20)) as u32) as u64) << 32 | (x.to_bits() & 0xffff_ffff));
    let mut tx = [0.0; 3];
    for slot in tx.iter_mut().take(2) {
        *slot = f64::from(z as i32);
        z = (z - *slot) * TWO24;
    }
    tx[2] = z;
    let mut nx = 3;
    while tx[nx - 1] == 0.0 {
        nx -= 1;
    }
    let (n, y0, y1) = kernel_rem_pio2(&tx[..nx], e0);
    if hx < 0 { (-n, -y0, -y1) } else { (n, y0, y1) }
}

/// fdlibm `__kernel_rem_pio2` at double precision (prec = 2, jk = 4): compute
/// the integer (mod 8) and fraction parts of `(2/pi) * x` without doing the
/// full multiplication, where `x` is given as `nx` 24-bit chunks with leading
/// exponent `e0`.
fn kernel_rem_pio2(x: &[f64], e0: i32) -> (i32, f64, f64) {
    let jk = 4;
    let jp = jk;
    let jx = x.len() - 1;
    let jv = ((e0 - 3) / 24).max(0);
    let mut q0 = e0 - 24 * (jv + 1);
    let mut f = [0.0; 20];
    let mut fq = [0.0; 20];
    let mut q = [0.0; 20];
    let mut iq = [0i32; 20];
    // f[jx+jk] = ipio2[jv+jk].
    let j_start = jv - jx as i32;
    for (k, slot) in f.iter_mut().take(jx + jk + 1).enumerate() {
        let j = j_start + k as i32;
        *slot = if j < 0 {
            0.0
        } else {
            f64::from(TWO_OVER_PI[j as usize])
        };
    }
    // q[0]..q[jk]: the product chunks.
    for i in 0..=jk {
        let mut fw = 0.0;
        for jj in 0..=jx {
            fw += x[jj] * f[jx + i - jj];
        }
        q[i] = fw;
    }
    let mut jz = jk;
    let mut z;
    let mut n;
    let mut ih;
    loop {
        // Distill q[] into iq[] reversingly.
        z = q[jz];
        let mut i = 0;
        let mut jj = jz;
        while jj > 0 {
            let fw = f64::from((TWON24 * z) as i32);
            iq[i] = (z - TWO24 * fw) as i32;
            z = q[jj - 1] + fw;
            i += 1;
            jj -= 1;
        }
        // Trim off the integer part >= 8.
        z *= exp2(q0);
        z -= 8.0 * floor(z * 0.125);
        n = z as i32;
        z -= f64::from(n);
        ih = 0;
        if q0 > 0 {
            // Need iq[jz-1] to determine n.
            let ii = iq[jz - 1] >> (24 - q0);
            n += ii;
            iq[jz - 1] -= ii << (24 - q0);
            ih = iq[jz - 1] >> (23 - q0);
        } else if q0 == 0 {
            ih = iq[jz - 1] >> 23;
        } else if z >= 0.5 {
            ih = 2;
        }
        if ih > 0 {
            // q > 0.5: compute 1-q.
            n += 1;
            let mut carry = 0;
            for slot in iq.iter_mut().take(jz) {
                let jj = *slot;
                if carry == 0 {
                    if jj != 0 {
                        carry = 1;
                        *slot = 0x100_0000 - jj;
                    }
                } else {
                    *slot = 0xff_ffff - jj;
                }
            }
            if q0 > 0 {
                // Rare case: chance is 1 in 12.
                match q0 {
                    1 => iq[jz - 1] &= 0x7f_ffff,
                    2 => iq[jz - 1] &= 0x3f_ffff,
                    _ => {}
                }
            }
            if ih == 2 {
                z = 1.0 - z;
                if carry != 0 {
                    z -= exp2(q0);
                }
            }
        }
        // Recompute with more terms of 2/pi if the fraction distilled to zero.
        if z == 0.0 {
            let mut jj = 0;
            let mut i = jz as i32 - 1;
            while i >= jk as i32 {
                jj |= iq[i as usize];
                i -= 1;
            }
            if jj == 0 {
                let mut k = 1;
                while iq[jk - k] == 0 {
                    k += 1;
                }
                for i in (jz + 1)..=(jz + k) {
                    f[jx + i] = f64::from(TWO_OVER_PI[(jv + i as i32) as usize]);
                    let mut fw = 0.0;
                    for jj in 0..=jx {
                        fw += x[jj] * f[jx + i - jj];
                    }
                    q[i] = fw;
                }
                jz += k;
                continue;
            }
        }
        break;
    }
    // Chop off zero terms, or break z into 24-bit chunks.
    if z == 0.0 {
        jz -= 1;
        q0 -= 24;
        while iq[jz] == 0 {
            jz -= 1;
            q0 -= 24;
        }
    } else {
        z *= exp2(-q0);
        if z >= TWO24 {
            let fw = f64::from((TWON24 * z) as i32);
            iq[jz] = (z - TWO24 * fw) as i32;
            jz += 1;
            q0 += 24;
            iq[jz] = fw as i32;
        } else {
            iq[jz] = z as i32;
        }
    }
    // Convert the integer chunks back to floating point.
    let mut fw = exp2(q0);
    for i in (0..=jz).rev() {
        q[i] = fw * f64::from(iq[i]);
        fw *= TWON24;
    }
    // Multiply by the pi/2 chunks.
    for i in (0..=jz).rev() {
        let mut fw = 0.0;
        let mut k = 0;
        while k <= jp && k <= jz - i {
            fw += PIO2_CHUNKS[k] * q[i + k];
            k += 1;
        }
        fq[jz - i] = fw;
    }
    // Compress fq[] into the two-double result.
    let mut fw = 0.0;
    for i in (0..=jz).rev() {
        fw += fq[i];
    }
    let y0 = if ih == 0 { fw } else { -fw };
    let mut fw = fq[0] - fw;
    // The running accumulation order is part of fdlibm's error bound; do not
    // reassociate it (e.g. via iterator sum()).
    #[allow(clippy::needless_range_loop)]
    for i in 1..=jz {
        fw += fq[i];
    }
    let y1 = if ih == 0 { fw } else { -fw };
    (n & 7, y0, y1)
}

/// `exp`, ported from fdlibm (the JDK test suite's FdlibmTranslit.Exp).
// The decimal literals are fdlibm's documented constants; the hex comments are
// the authoritative bit patterns.
#[allow(clippy::approx_constant, clippy::excessive_precision)]
pub(crate) fn exp(x: f64) -> f64 {
    const HUGE: f64 = 1.0e+300;
    const TWOM1000: f64 = 9.332_636_185_032_188_789_90e-302; // 0x01700000, 0x00000000
    const O_THRESHOLD: f64 = 7.097_827_128_933_839_730_96e+02; // 0x40862E42, 0xFEFA39EF
    const U_THRESHOLD: f64 = -7.451_332_191_019_411_084_20e+02; // 0xC0874910, 0xD52D3051
    const LN2HI: f64 = 6.931_471_803_691_238_164_90e-01; // 0x3FE62E42, 0xFEE00000
    const LN2LO: f64 = 1.908_214_929_270_587_700_02e-10; // 0x3DEA39EF, 0x35793C76
    const INVLN2: f64 = 1.442_695_040_888_963_387_00e+00; // 0x3FF71547, 0x652B82FE
    const P1: f64 = 1.666_666_666_666_660_190_37e-01; // 0x3FC55555, 0x5555553E
    const P2: f64 = -2.777_777_777_701_559_338_42e-03; // 0xBF66C16C, 0x16BEBD93
    const P3: f64 = 6.613_756_321_437_934_361_17e-05; // 0x3F11566A, 0xAF25DE2C
    const P4: f64 = -1.653_390_220_546_525_153_90e-06; // 0xBEBBBD41, 0xC5D26BF1
    const P5: f64 = 4.138_136_797_057_238_460_39e-08; // 0x3E663769, 0x72BEA4D0
    let hx = hi_word(x);
    let xsb = (hx >> 31) & 1; // sign bit of x
    let hx = hx & 0x7fff_ffff; // high word of |x|
    // Filter out non-finite arguments.
    if hx >= 0x4086_2e42 {
        // |x| >= 709.78...
        if hx >= 0x7ff0_0000 {
            return if (hx & 0x000f_ffff) | (x.to_bits() as u32 as i32) != 0 {
                inf_add(x) // NaN
            } else if xsb == 0 {
                x // exp(+inf) = +inf
            } else {
                0.0 // exp(-inf) = 0
            };
        }
        if x > O_THRESHOLD {
            return HUGE * HUGE; // overflow
        }
        if x < U_THRESHOLD {
            return TWOM1000 * TWOM1000; // underflow
        }
    }
    // Argument reduction.
    let mut x = x;
    let mut hi = 0.0;
    let mut lo = 0.0;
    let mut k = 0i32;
    if hx > 0x3fd6_2e42 {
        // |x| > 0.5 ln2
        if hx < 0x3ff0_a2b2 {
            // |x| < 1.5 ln2
            if xsb == 0 {
                hi = x - LN2HI;
                lo = LN2LO;
                k = 1;
            } else {
                hi = x + LN2HI;
                lo = -LN2LO;
                k = -1;
            }
        } else {
            k = (INVLN2 * x + if xsb == 0 { 0.5 } else { -0.5 }) as i32;
            let t = f64::from(k);
            hi = x - t * LN2HI; // t*LN2HI is exact here
            lo = t * LN2LO;
        }
        x = hi - lo;
    } else if hx < 0x3e30_0000 {
        // |x| < 2^-28: exp(x) = 1 + x (raises inexact).
        return 1.0 + x;
    }
    // x is now in the primary range.
    let t = x * x;
    let c = x - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
    if k == 0 {
        return 1.0 - ((x * c) / (c - 2.0) - x);
    }
    let mut y = 1.0 - ((lo - (x * c) / (2.0 - c)) - hi);
    if k >= -1021 {
        // Add k to y's exponent.
        set_hi_word(y, hi_word(y) + (k << 20))
    } else {
        y = set_hi_word(y, hi_word(y) + ((k + 1000) << 20));
        y * TWOM1000
    }
}

/// `ln`, ported from fdlibm (the JDK test suite's FdlibmTranslit.Log).
#[allow(clippy::approx_constant, clippy::excessive_precision)]
pub(crate) fn ln(x: f64) -> f64 {
    const LN2_HI: f64 = 6.931_471_803_691_238_164_90e-01; // 0x3FE62E42, 0xFEE00000
    const LN2_LO: f64 = 1.908_214_929_270_587_700_02e-10; // 0x3DEA39EF, 0x35793C76
    const TWO54: f64 = 1.801_439_850_948_198_400_00e+16; // 0x43500000, 0x00000000
    const LG1: f64 = 6.666_666_666_666_735_130e-01; // 0x3FE55555, 0x55555593
    const LG2: f64 = 3.999_999_999_940_941_908e-01; // 0x3FD99999, 0x9997FA04
    const LG3: f64 = 2.857_142_874_366_239_149e-01; // 0x3FD24924, 0x94229359
    const LG4: f64 = 2.222_219_843_214_978_396e-01; // 0x3FCC71C5, 0x1D8E78AF
    const LG5: f64 = 1.818_357_216_161_805_012e-01; // 0x3FC74664, 0x96CB03DE
    const LG6: f64 = 1.531_383_769_920_937_332e-01; // 0x3FC39A09, 0xD078C69F
    const LG7: f64 = 1.479_819_860_511_658_591e-01; // 0x3FC2F112, 0xDF3E5244
    let mut hx = hi_word(x);
    let lx = x.to_bits() as u32 as i32;
    let mut k = 0i32;
    let mut x = x;
    if hx < 0x0010_0000 {
        // x < 2^-1022
        if (hx & 0x7fff_ffff) | lx == 0 {
            return f64::NEG_INFINITY; // log(+-0) = -inf
        }
        if hx < 0 {
            // fdlibm's (x-x)/0 yields a negative NaN on the JDK.
            return f64::from_bits(0xfff8_0000_0000_0000); // log(-#) = NaN
        }
        k -= 54;
        x *= TWO54; // subnormal number: scale up x
        hx = hi_word(x);
    }
    if hx >= 0x7ff0_0000 {
        return inf_add(x); // log(NaN) = NaN, log(+inf) = +inf
    }
    k += (hx >> 20) - 1023;
    let hx = hx & 0x000f_ffff;
    let i = (hx + 0x9_5f64) & 0x0010_0000;
    x = set_hi_word(x, hx | (i ^ 0x3ff0_0000)); // normalize x or x/2
    k += i >> 20;
    let f = x - 1.0;
    let dk = f64::from(k);
    if (0x000f_ffff & (2 + hx)) < 3 {
        // |f| < 2^-20
        if f == 0.0 {
            return if k == 0 {
                0.0
            } else {
                dk * LN2_HI + dk * LN2_LO
            };
        }
        let r = f * f * (0.5 - 0.333_333_333_333_333_33 * f);
        return if k == 0 {
            f - r
        } else {
            dk * LN2_HI - ((r - dk * LN2_LO) - f)
        };
    }
    let s = f / (2.0 + f);
    let z = s * s;
    let mut i = hx - 0x6_147a;
    let w = z * z;
    let j = 0x6_b851 - hx;
    let t1 = w * (LG2 + w * (LG4 + w * LG6));
    let t2 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
    i |= j;
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

/// `log10`, ported from fdlibm (the JDK test suite's FdlibmTranslit.Log10).
#[allow(clippy::approx_constant, clippy::excessive_precision)]
pub(crate) fn log10(x: f64) -> f64 {
    const TWO54: f64 = 1.801_439_850_948_198_400_00e+16; // 0x43500000, 0x00000000
    const IVLN10: f64 = 4.342_944_819_032_518_166_68e-01; // 0x3FDBCB7B, 0x1526E50E
    const LOG10_2HI: f64 = 3.010_299_956_636_117_713_06e-01; // 0x3FD34413, 0x509F6000
    const LOG10_2LO: f64 = 3.694_239_077_158_930_786_16e-13; // 0x3D59FEF3, 0x11F12B36
    let mut hx = hi_word(x);
    let lx = x.to_bits() as u32 as i32;
    let mut k = 0i32;
    let mut x = x;
    if hx < 0x0010_0000 {
        // x < 2^-1022
        if (hx & 0x7fff_ffff) | lx == 0 {
            return f64::NEG_INFINITY; // log10(+-0) = -inf
        }
        if hx < 0 {
            // fdlibm's (x-x)/0 yields a negative NaN on the JDK.
            return f64::from_bits(0xfff8_0000_0000_0000); // log10(-#) = NaN
        }
        k -= 54;
        x *= TWO54; // subnormal number: scale up x
        hx = hi_word(x);
    }
    if hx >= 0x7ff0_0000 {
        return inf_add(x); // log10(NaN) = NaN, log10(+inf) = +inf
    }
    k += (hx >> 20) - 1023;
    let i = (k as u32 >> 31) as i32; // sign bit of k
    let hx = (hx & 0x000f_ffff) | ((0x3ff - i) << 20);
    let y = f64::from(k + i);
    x = set_hi_word(x, hx);
    let z = y * LOG10_2LO + IVLN10 * ln(x);
    z + y * LOG10_2HI
}

/// `expm1`: `exp(x) - 1`, accurate for small `x`.
/// `expm1`, ported from fdlibm (the JDK test suite's FdlibmTranslit.Expm1).
// The decimal literals are fdlibm's documented constants; the hex comments are
// the authoritative bit patterns.
#[allow(clippy::approx_constant, clippy::excessive_precision)]
pub(crate) fn expm1(x: f64) -> f64 {
    const HUGE: f64 = 1.0e+300;
    const TINY: f64 = 1.0e-300;
    const O_THRESHOLD: f64 = 7.097_827_128_933_839_730_96e+02; // 0x40862E42, 0xFEFA39EF
    const LN2HI: f64 = 6.931_471_803_691_238_164_90e-01; // 0x3FE62E42, 0xFEE00000
    const LN2LO: f64 = 1.908_214_929_270_587_700_02e-10; // 0x3DEA39EF, 0x35793C76
    const INVLN2: f64 = 1.442_695_040_888_963_387_00e+00; // 0x3FF71547, 0x652B82FE
    const Q1: f64 = -3.333_333_333_333_313_164_28e-02; // 0xBFA11111, 0x111110F4
    const Q2: f64 = 1.587_301_587_254_814_601_65e-03; // 0x3F5A01A0, 0x19FE5585
    const Q3: f64 = -7.936_507_578_674_879_424_73e-05; // 0xBF14CE19, 0x9EAADBB7
    const Q4: f64 = 4.008_217_827_329_362_395_52e-06; // 0x3ED0CFCA, 0x86E65239
    const Q5: f64 = -2.010_992_181_836_243_713_26e-07; // 0xBE8AFDB7, 0x6E09C32D
    let mut hx = hi_word(x);
    let xsb = hx & i32::MIN; // sign bit of x
    hx &= 0x7fff_ffff; // high word of |x|
    // Filter out huge and non-finite arguments.
    if hx >= 0x4043_687a {
        // |x| >= 56 ln2
        if hx >= 0x4086_2e42 {
            // |x| >= 709.78...
            if hx >= 0x7ff0_0000 {
                return if (hx & 0x000f_ffff) | (x.to_bits() as u32 as i32) != 0 {
                    inf_add(x) // NaN
                } else if xsb == 0 {
                    x // expm1(+inf) = +inf
                } else {
                    -1.0 // expm1(-inf) = -1
                };
            }
            if x > O_THRESHOLD {
                return HUGE * HUGE; // overflow
            }
        }
        if xsb != 0 && x + TINY < 0.0 {
            return TINY - 1.0; // x < -56 ln2: -1 with inexact
        }
    }
    // Argument reduction.
    let mut x = x;
    let mut c = 0.0;
    let mut k = 0i32;
    if hx > 0x3fd6_2e42 {
        // |x| > 0.5 ln2
        let (hi, lo) = if hx < 0x3ff0_a2b2 {
            // |x| < 1.5 ln2
            if xsb == 0 {
                k = 1;
                (x - LN2HI, LN2LO)
            } else {
                k = -1;
                (x + LN2HI, -LN2LO)
            }
        } else {
            k = (INVLN2 * x + if xsb == 0 { 0.5 } else { -0.5 }) as i32;
            let t = f64::from(k);
            (x - t * LN2HI, t * LN2LO) // t*LN2HI is exact here
        };
        x = hi - lo;
        c = (hi - x) - lo;
    } else if hx < 0x3c90_0000 {
        // |x| < 2^-54: return x (with inexact when x != 0).
        let t = HUGE + x;
        return x - (t - (HUGE + x));
    }
    // x is now in the primary range.
    let hfx = 0.5 * x;
    let hxs = x * hfx;
    let r1 = 1.0 + hxs * (Q1 + hxs * (Q2 + hxs * (Q3 + hxs * (Q4 + hxs * Q5))));
    let t = 3.0 - r1 * hfx;
    let mut e = hxs * ((r1 - t) / (6.0 - x * t));
    if k == 0 {
        return x - (x * e - hxs); // c is 0
    }
    e = x * (e - c) - c;
    e -= hxs;
    if k == -1 {
        return 0.5 * (x - e) - 0.5;
    }
    if k == 1 {
        return if x < -0.25 {
            -2.0 * (e - (x + 0.5))
        } else {
            1.0 + 2.0 * (x - e)
        };
    }
    let mut y;
    if k <= -2 || k > 56 {
        // Suffices to return exp(x)-1.
        y = 1.0 - (e - x);
        y = set_hi_word(y, hi_word(y) + (k << 20)); // add k to y's exponent
        return y - 1.0;
    }
    let mut t = 1.0;
    if k < 20 {
        t = set_hi_word(t, 0x3ff0_0000 - (0x20_0000 >> k)); // t = 1-2^-k
        y = t - (e - x);
    } else {
        t = set_hi_word(t, (0x3ff - k) << 20); // t = 2^-k
        y = x - (e + t);
        y += 1.0;
    }
    set_hi_word(y, hi_word(y) + (k << 20)) // add k to y's exponent
}

/// `log1p`: `ln(1 + x)`, ported from fdlibm (FdlibmTranslit.Log1p).
#[allow(clippy::approx_constant, clippy::excessive_precision)]
pub(crate) fn log1p(x: f64) -> f64 {
    const LN2_HI: f64 = 6.931_471_803_691_238_164_90e-01; // 0x3FE62E42, 0xFEE00000
    const LN2_LO: f64 = 1.908_214_929_270_587_700_02e-10; // 0x3DEA39EF, 0x35793C76
    const TWO54: f64 = 1.801_439_850_948_198_400_00e+16; // 0x43500000, 0x00000000
    const LP1: f64 = 6.666_666_666_666_735_130e-01; // 0x3FE55555, 0x55555593
    const LP2: f64 = 3.999_999_999_940_941_908e-01; // 0x3FD99999, 0x9997FA04
    const LP3: f64 = 2.857_142_874_366_239_149e-01; // 0x3FD24924, 0x94229359
    const LP4: f64 = 2.222_219_843_214_978_396e-01; // 0x3FCC71C5, 0x1D8E78AF
    const LP5: f64 = 1.818_357_216_161_805_012e-01; // 0x3FC74664, 0x96CB03DE
    const LP6: f64 = 1.531_383_769_920_937_332e-01; // 0x3FC39A09, 0xD078C69F
    const LP7: f64 = 1.479_819_860_511_658_591e-01; // 0x3FC2F112, 0xDF3E5244
    let hx = hi_word(x);
    let ax = hx & 0x7fff_ffff;
    let mut k = 1i32;
    let mut f = 0.0;
    let mut c = 0.0;
    let mut hu = 0i32;
    if hx < 0x3fda_827a {
        // x < 0.41422
        if ax >= 0x3ff0_0000 {
            // x <= -1.0
            if x == -1.0 {
                return f64::NEG_INFINITY; // log1p(-1) = -inf
            }
            // fdlibm's (x-x)/(x-x) yields a positive NaN on the JDK.
            return f64::NAN; // log1p(x < -1) = NaN
        }
        if ax < 0x3e20_0000 {
            // |x| < 2^-29
            if TWO54 + x > 0.0 && ax < 0x3c90_0000 {
                return x; // |x| < 2^-54
            }
            return x - x * x * 0.5;
        }
        if hx > 0 || hx <= 0xbfd2_bec3u32 as i32 {
            // -0.2929 < x < 0.41422
            k = 0;
            f = x;
            hu = 1;
        }
    }
    if hx >= 0x7ff0_0000 {
        return inf_add(x); // log1p(NaN) = NaN, log1p(+inf) = +inf
    }
    let mut u;
    if k != 0 {
        if hx < 0x4340_0000 {
            u = 1.0 + x;
            hu = hi_word(u);
            k = (hu >> 20) - 1023;
            c = if k > 0 { 1.0 - (u - x) } else { x - (u - 1.0) }; // correction term
            c /= u;
        } else {
            u = x;
            hu = hi_word(u);
            k = (hu >> 20) - 1023;
            c = 0.0;
        }
        hu &= 0x000f_ffff;
        if hu < 0x6_a09e {
            u = set_hi_word(u, hu | 0x3ff0_0000); // normalize u
        } else {
            k += 1;
            u = set_hi_word(u, hu | 0x3fe0_0000); // normalize u/2
            hu = (0x0010_0000 - hu) >> 2;
        }
        f = u - 1.0;
    }
    let hfsq = 0.5 * f * f;
    let dk = f64::from(k);
    if hu == 0 {
        // |f| < 2^-20
        if f == 0.0 {
            return if k == 0 {
                0.0
            } else {
                c += dk * LN2_LO;
                dk * LN2_HI + c
            };
        }
        let r = hfsq * (1.0 - 0.666_666_666_666_666_66 * f);
        return if k == 0 {
            f - r
        } else {
            dk * LN2_HI - ((r - (dk * LN2_LO + c)) - f)
        };
    }
    let s = f / (2.0 + f);
    let z = s * s;
    let r = z * (LP1 + z * (LP2 + z * (LP3 + z * (LP4 + z * (LP5 + z * (LP6 + z * LP7))))));
    if k == 0 {
        f - (hfsq - s * (hfsq + r))
    } else {
        dk * LN2_HI - ((hfsq - (s * (hfsq + r) + (dk * LN2_LO + c))) - f)
    }
}

/// `sinh`, ported from fdlibm (the JDK test suite's FdlibmTranslit.Sinh).
pub(crate) fn sinh(x: f64) -> f64 {
    const SHUGE: f64 = 1.0e307;
    let jx = hi_word(x);
    let ix = jx & 0x7fff_ffff;
    if ix >= 0x7ff0_0000 {
        return inf_add(x); // x is inf or NaN
    }
    let h = if jx < 0 { -0.5 } else { 0.5 };
    if ix < 0x4036_0000 {
        // |x| < 22
        if ix < 0x3e30_0000 && SHUGE + x > 1.0 {
            return x; // sinh(tiny) = tiny with inexact
        }
        let t = expm1(x.abs());
        if ix < 0x3ff0_0000 {
            return h * (2.0 * t - t * t / (t + 1.0));
        }
        return h * (t + t / (t + 1.0));
    }
    if ix < 0x4086_2e42 {
        return h * exp(x.abs());
    }
    // Up to ln(2*MAX), sinh(x) = exp(x/2)/2 * exp(x/2); overflow beyond.
    if ix < 0x4086_33ce || (ix == 0x4086_33ce && (x.to_bits() as u32) <= 0x8fb9_f87d) {
        let w = exp(0.5 * x.abs());
        let t = h * w;
        return t * w;
    }
    x * SHUGE
}

/// `cosh`, ported from fdlibm (the JDK test suite's FdlibmTranslit.Cosh).
pub(crate) fn cosh(x: f64) -> f64 {
    const HUGE: f64 = 1.0e300;
    let ix = hi_word(x) & 0x7fff_ffff;
    if ix >= 0x7ff0_0000 {
        return x * x; // x is inf or NaN
    }
    if ix < 0x3fd6_2e43 {
        let t = expm1(x.abs());
        let w = 1.0 + t;
        if ix < 0x3c80_0000 {
            return w; // cosh(tiny) = 1
        }
        return 1.0 + (t * t) / (w + w);
    }
    if ix < 0x4036_0000 {
        let t = exp(x.abs());
        return 0.5 * t + 0.5 / t;
    }
    if ix < 0x4086_2e42 {
        return 0.5 * exp(x.abs());
    }
    if ix < 0x4086_33ce || (ix == 0x4086_33ce && (x.to_bits() as u32) <= 0x8fb9_f87d) {
        let w = exp(0.5 * x.abs());
        let t = 0.5 * w;
        return t * w;
    }
    HUGE * HUGE
}

/// `tanh`, ported from fdlibm (the JDK test suite's FdlibmTranslit.Tanh).
pub(crate) fn tanh(x: f64) -> f64 {
    const TINY: f64 = 1.0e-300;
    let jx = hi_word(x);
    let ix = jx & 0x7fff_ffff;
    if ix >= 0x7ff0_0000 {
        // tanh(+-inf) = +-1; tanh(NaN) = NaN (sign preserved via 1/x).
        return if jx >= 0 {
            1.0 / x + 1.0
        } else {
            1.0 / x - 1.0
        };
    }
    let z = if ix < 0x4036_0000 {
        // |x| < 22
        if ix < 0x3c80_0000 {
            return x * (1.0 + x); // tanh(small) = small
        }
        if ix >= 0x3ff0_0000 {
            // |x| >= 1
            let t = expm1(2.0 * x.abs());
            1.0 - 2.0 / (t + 2.0)
        } else {
            let t = expm1(-2.0 * x.abs());
            -t / (t + 2.0)
        }
    } else {
        1.0 - TINY // raised inexact flag
    };
    if jx >= 0 { z } else { -z }
}

/// `asinh`, ported from fdlibm (the JDK test suite's FdlibmTranslit.Asinh).
#[allow(clippy::approx_constant, clippy::excessive_precision)]
pub(crate) fn asinh(x: f64) -> f64 {
    const LN2: f64 = 6.931_471_805_599_452_862_27e-01; // 0x3FE62E42, 0xFEFA39EF
    const HUGE: f64 = 1.0e300;
    let hx = hi_word(x);
    let ix = hx & 0x7fff_ffff;
    if ix >= 0x7ff0_0000 {
        return inf_add(x); // x is inf or NaN
    }
    if ix < 0x3e30_0000 && HUGE + x > 1.0 {
        return x; // |x| < 2^-28, inexact except 0
    }
    let w = if ix > 0x41b0_0000 {
        // |x| > 2^28
        ln(x.abs()) + LN2
    } else if ix > 0x4000_0000 {
        // 2^28 > |x| > 2
        let t = x.abs();
        ln(2.0 * t + 1.0 / (sqrt(x * x + 1.0) + t))
    } else {
        // 2 >= |x| >= 2^-28
        let t = x * x;
        log1p(x.abs() + t / (1.0 + sqrt(1.0 + t)))
    };
    if hx > 0 { w } else { -w }
}

/// `acosh`, ported from fdlibm (the JDK test suite's FdlibmTranslit.Acosh).
#[allow(clippy::approx_constant, clippy::excessive_precision)]
pub(crate) fn acosh(x: f64) -> f64 {
    const LN2: f64 = 6.931_471_805_599_452_862_27e-01; // 0x3FE62E42, 0xFEFA39EF
    let hx = hi_word(x);
    if hx < 0x3ff0_0000 {
        return nan_nan(x); // x < 1
    }
    if hx >= 0x41b0_0000 {
        // x > 2^28
        return if hx >= 0x7ff0_0000 {
            inf_add(x) // x is inf or NaN
        } else {
            ln(x) + LN2 // acosh(huge) = log(2x)
        };
    }
    if ((hx - 0x3ff0_0000) | (x.to_bits() as u32 as i32)) == 0 {
        return 0.0; // acosh(1) = 0
    }
    if hx > 0x4000_0000 {
        // 2^28 > x > 2
        let t = x * x;
        return ln(2.0 * x - 1.0 / (x + sqrt(t - 1.0)));
    }
    // 1 < x <= 2
    let t = x - 1.0;
    log1p(t + sqrt(2.0 * t + t * t))
}

/// `atanh`, ported from fdlibm (the JDK test suite's FdlibmTranslit.Atanh).
pub(crate) fn atanh(x: f64) -> f64 {
    const HUGE: f64 = 1.0e300;
    let hx = hi_word(x);
    let lx = x.to_bits() as u32 as i32;
    let ix = hx & 0x7fff_ffff;
    // |x| > 1 (the low-word trick also catches NaN).
    let nz = ((lx | lx.wrapping_neg()) as u32 >> 31) as i32;
    if (ix | nz) > 0x3ff0_0000 {
        return nan_nan(x);
    }
    if ix == 0x3ff0_0000 {
        return x / 0.0; // atanh(+-1) = +-inf
    }
    if ix < 0x3e30_0000 && HUGE + x > 0.0 {
        return x; // |x| < 2^-28
    }
    let ax = set_hi_word(x, ix); // x <- |x|
    let t = if ix < 0x3fe0_0000 {
        // x < 0.5
        let t = ax + ax;
        0.5 * log1p(t + t * ax / (1.0 - ax))
    } else {
        0.5 * log1p((ax + ax) / (1.0 - ax))
    };
    if hx >= 0 { t } else { -t }
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
    // Integer exponents via repeated squaring: for integer powers of small
    // integer bases (e.g. pow(5, 3) == 125) every intermediate is exact,
    // while exp(y*ln(x)) can land one ulp low and truncate to the wrong int.
    if y.abs() <= 1024.0 && (y - floor(y)) == 0.0 {
        let mut n = y.abs() as u32;
        let mut base = x.abs();
        let mut acc = 1.0f64;
        while n > 0 {
            if n & 1 != 0 {
                acc *= base;
            }
            base *= base;
            n >>= 1;
        }
        let negative = x < 0.0 && (y as i64) % 2 != 0;
        let result = if y < 0.0 { 1.0 / acc } else { acc };
        return if negative { -result } else { result };
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
    let pi = core::f64::consts::PI;
    let half_pi = core::f64::consts::FRAC_PI_2;
    let y_neg = y.is_sign_negative();
    if x == 0.0 {
        if y == 0.0 {
            // atan2(±0, +0) = ±0; atan2(±0, -0) = ±π.
            return if x.is_sign_negative() {
                if y_neg { -pi } else { pi }
            } else {
                y
            };
        }
        // x == ±0 with y nonzero (including ±∞): ±π/2 by the sign of y.
        return if y_neg { -half_pi } else { half_pi };
    }
    if x.is_infinite() {
        if y.is_infinite() {
            // atan2(±∞, +∞) = ±π/4; atan2(±∞, -∞) = ±3π/4.
            let magnitude = if x > 0.0 {
                core::f64::consts::FRAC_PI_4
            } else {
                3.0 * core::f64::consts::FRAC_PI_4
            };
            return if y_neg { -magnitude } else { magnitude };
        }
        // x == +∞ with y finite: ±0; x == -∞ with y finite: ±π.
        return if x > 0.0 {
            if y_neg { -0.0 } else { 0.0 }
        } else if y_neg {
            -pi
        } else {
            pi
        };
    }
    if y.is_infinite() {
        return if y_neg { -half_pi } else { half_pi };
    }
    if y == 0.0 {
        // y == ±0 with x finite and nonzero: ±0 for x > 0, ±π for x < 0.
        return if x > 0.0 {
            y
        } else if y_neg {
            -pi
        } else {
            pi
        };
    }
    if x > 0.0 {
        atan(y / x)
    } else if y_neg {
        atan(y / x) - pi
    } else {
        atan(y / x) + pi
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
    // An infinite argument dominates everything, including NaN; check this
    // before the max/min comparison, which NaN would otherwise poison.
    if x.is_infinite() || y.is_infinite() {
        return f64::INFINITY;
    }
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    let max = if x > y { x } else { y };
    let min = if x > y { y } else { x };
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
    // A NaN sign is treated as positive (StrictMath requires this; Math permits
    // it), so ignore the sign bit of a NaN rather than propagating NaN.
    let sign_bit = if sign.is_nan() {
        0
    } else {
        sign.to_bits() & (1u64 << 63)
    };
    let bits = (magnitude.to_bits() & !(1u64 << 63)) | sign_bit;
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
    // Construct 2^exponent directly from the IEEE-754 encoding instead of a
    // loop, so extreme scale factors (e.g. Integer.MAX_VALUE) are O(1) rather
    // than billions of iterations.
    if exponent >= 1024 {
        return f64::INFINITY;
    }
    if exponent <= -1075 {
        return 0.0;
    }
    if exponent >= -1022 {
        // Normal: biased exponent field is exponent + 1023.
        f64::from_bits(((exponent + 1023) as u64) << 52)
    } else {
        // Subnormal range -1074..-1023: 2^-1074 has encoding 1.
        f64::from_bits(1u64 << (exponent + 1074))
    }
}

fn scalb(value: f64, scale_factor: i32) -> f64 {
    // Zero, infinity and NaN are returned unchanged regardless of the scale
    // factor (avoids 0 * inf and inf * 0 producing NaN for extreme factors).
    if value.is_nan() || value == 0.0 || value.is_infinite() {
        return value;
    }
    // Adjust the exponent directly rather than multiplying by 2^n: an
    // independent 2^n over/underflows for extreme n, and staged multiplication
    // double-rounds subnormal results.  Work on the IEEE-754 encoding with a
    // single round-to-nearest-even step.
    let bits = value.to_bits();
    let sign = bits & (1u64 << 63);
    let mut exp = ((bits >> 52) & 0x7ff) as i32;
    let mut mant = bits & 0xf_ffff_ffff_ffff;
    if exp == 0 {
        // Subnormal input: normalize the significand to [2^52, 2^53).
        let shift = mant.leading_zeros() as i32 - 11;
        mant <<= shift;
        mant &= 0xf_ffff_ffff_ffff;
        exp = 1 - shift;
    }
    let new_exp = exp as i64 + scale_factor as i64;
    if new_exp >= 0x7ff {
        return f64::from_bits(sign | (0x7ffu64 << 52));
    }
    if new_exp <= 0 {
        // Subnormal or zero result: shift the 53-bit significand right with
        // round-to-nearest-even.
        let full_mant = mant | (1u64 << 52);
        let shift = (1 - new_exp) as u32;
        if shift >= 54 {
            return f64::from_bits(sign);
        }
        let shifted = full_mant >> shift;
        let rem = full_mant & ((1u64 << shift) - 1);
        let halfway = 1u64 << (shift - 1);
        let mut rounded = shifted;
        if rem > halfway || (rem == halfway && (shifted & 1) == 1) {
            rounded += 1;
        }
        return f64::from_bits(sign | rounded);
    }
    f64::from_bits(sign | ((new_exp as u64) << 52) | mant)
}

fn ulp(value: f64) -> f64 {
    if value.is_nan() {
        return f64::NAN;
    }
    let value = value.abs();
    if value.is_infinite() {
        return f64::INFINITY;
    }
    let exp = ((value.to_bits() >> 52) & 0x7ff) as i32;
    if exp == 0 {
        // Zero or subnormal: ulp is the smallest subnormal.
        return f64::from_bits(1);
    }
    // Normal: ulp = 2^(unbiased_exponent - 52).
    pow2(exp - 1023 - 52)
}

fn ulp_float(value: f32) -> f32 {
    if value.is_nan() {
        return f32::NAN;
    }
    let value = value.abs();
    if value.is_infinite() {
        return f32::INFINITY;
    }
    let exp = ((value.to_bits() >> 23) & 0xff) as i32;
    if exp == 0 {
        return f32::from_bits(1);
    }
    // Normal: ulp = 2^(unbiased_exponent - 23), exactly representable as f32.
    pow2(exp - 127 - 23) as f32
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

fn next_after_float(start: f32, direction: f64) -> f32 {
    if start.is_nan() || direction.is_nan() {
        return f32::NAN;
    }
    let start64 = f64::from(start);
    if start64 == direction {
        // Equal arguments return the direction (preserving its zero sign).
        return direction as f32;
    }
    if start == 0.0 {
        return if direction > 0.0 {
            f32::from_bits(1)
        } else {
            f32::from_bits((1u32 << 31) | 1)
        };
    }
    let bits = start.to_bits();
    let towards_more = (direction > start64) == (start > 0.0);
    f32::from_bits(if towards_more { bits + 1 } else { bits - 1 })
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
            method(
                "nextAfter",
                "(FD)F",
                ACC_PUBLIC | ACC_STATIC,
                |_, _, args| float(next_after_float(float_arg(args, 0), double_arg(args, 1))),
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
                double(ulp(double_arg(args, 0)))
            }),
            method("ulp", "(F)F", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                float(ulp_float(float_arg(args, 0)))
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
                        // Zero and subnormal both yield MIN_EXPONENT - 1 (-127).
                        -127
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
                        // Zero and subnormal both yield MIN_EXPONENT - 1 (-1023).
                        -1023
                    } else {
                        exponent - 1023
                    })
                },
            ),
            method("scalb", "(FI)F", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                float(scalb(f64::from(float_arg(args, 0)), int_arg(args, 1)) as f32)
            }),
            method("scalb", "(DI)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(scalb(double_arg(args, 0), int_arg(args, 1)))
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
            method("asinh", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(asinh(double_arg(args, 0)))
            }),
            method("acosh", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(acosh(double_arg(args, 0)))
            }),
            method("atanh", "(D)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
                double(atanh(double_arg(args, 0)))
            }),
        ]
    }};
}

const MATH_METHODS: [oxjvm_vm::NativeMethodDef; 103] = math_methods!();
const STRICT_MATH_METHODS: [oxjvm_vm::NativeMethodDef; 103] = math_methods!();

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
pub(crate) fn decimal_scale(text: &str) -> i32 {
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
/// Exact `BigDecimal.divide(BigDecimal)`: the quotient must terminate,
/// otherwise ArithmeticException like the JDK. The result scale is the
/// natural terminating scale reduced by trailing zeros down to the
/// preferred scale (dividend scale minus divisor scale).
pub(crate) fn big_decimal_divide_exact(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let a_text = big_text(vm, receiver(args));
    let b_text = big_text(vm, ref_arg(args, 1));
    let (neg_a, int_a, frac_a) = decimal_parts(&a_text);
    let (neg_b, int_b, frac_b) = decimal_parts(&b_text);
    let preferred = frac_a.len() as i32 - frac_b.len() as i32;
    let ua = parse_i128(&alloc::format!("{int_a}{frac_a}"));
    let ub = parse_i128(&alloc::format!("{int_b}{frac_b}"));
    let (Some(ua), Some(ub)) = (ua, ub) else {
        return Err(vm.throw_new("java/lang/ArithmeticException", Some("Overflow")));
    };
    if ub == 0 {
        let msg = if ua == 0 {
            "Division undefined"
        } else {
            "Division by zero"
        };
        return Err(vm.throw_new("java/lang/ArithmeticException", Some(msg)));
    }
    if ua == 0 {
        let text = big_decimal_with_scale("0", preferred);
        return create_big_number(vm, "java/math/BigDecimal", &text);
    }
    let negative = neg_a != neg_b;
    let mut num = ua.unsigned_abs();
    let mut den = ub.unsigned_abs();
    let g = gcd_u128(num, den);
    num /= g;
    den /= g;
    let mut twos = 0u32;
    let mut fives = 0u32;
    while den % 2 == 0 {
        den /= 2;
        twos += 1;
    }
    while den % 5 == 0 {
        den /= 5;
        fives += 1;
    }
    if den != 1 {
        return Err(vm.throw_new(
            "java/lang/ArithmeticException",
            Some("Non-terminating decimal expansion; no exact representable decimal result"),
        ));
    }
    let factor = 2u128
        .checked_pow(fives)
        .and_then(|v| v.checked_mul(5u128.checked_pow(twos)?));
    let unscaled = factor.and_then(|f| num.checked_mul(f));
    let Some(mut u) = unscaled else {
        return Err(vm.throw_new("java/lang/ArithmeticException", Some("Overflow")));
    };
    let mut scale = i64::from(preferred) + i64::from(twos + fives);
    while scale > i64::from(preferred) && u % 10 == 0 {
        u /= 10;
        scale -= 1;
    }
    if scale > i64::from(i32::MAX) || scale < i64::from(i32::MIN) {
        return Err(vm.throw_new("java/lang/ArithmeticException", Some("Overflow")));
    }
    let digits = u.to_string();
    let text = if scale >= 0 {
        format_scaled_digits(&digits, negative, scale as usize)
    } else {
        let signed = if negative && digits != "0" {
            alloc::format!("-{digits}")
        } else {
            digits
        };
        big_decimal_with_scale(&signed, scale as i32)
    };
    create_big_number(vm, "java/math/BigDecimal", &text)
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

pub(crate) fn big_decimal_divide_scaled(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let a_text = big_text(vm, receiver(args));
    let b_text = big_text(vm, ref_arg(args, 1));
    let new_scale = int_arg(args, 2);
    let mode = match args[3] {
        Value::Int(m) => {
            if !(0..=7).contains(&m) {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Invalid rounding mode"),
                ));
            }
            m
        }
        _ => {
            let mode_ref = ref_arg(args, 3);
            if mode_ref.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            rounding_mode_ordinal(vm, mode_ref)
        }
    };
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
            if new_scale < 0 {
                let text = big_decimal_with_scale(&value.to_string(), new_scale);
                return create_big_number(vm, "java/math/BigDecimal", &text);
            }
            if let Some(text) = format_scaled(value, new_scale as usize) {
                return create_big_number(vm, "java/math/BigDecimal", &text);
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
/// Sign, significant digit string (leading zeros trimmed) and E-aware scale
/// of a BigDecimal text, without ever expanding the exponent.
fn big_decimal_mantissa(text: &str) -> (bool, String, i64) {
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (mantissa, exponent) = match body.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i64>().unwrap_or(0)),
        None => (body, 0),
    };
    let (int_part, frac_part) = match mantissa.split_once('.') {
        Some((i, f)) => (i, f),
        None => (mantissa, ""),
    };
    let digits = alloc::format!("{int_part}{frac_part}");
    let trimmed = digits.trim_start_matches('0');
    let digits = if trimmed.is_empty() {
        String::from("0")
    } else {
        String::from(trimmed)
    };
    (negative, digits, frac_part.len() as i64 - exponent)
}

/// Render an unscaled digit string at the given scale in the canonical
/// storage form: plain for non-negative scales, d.dddE+adj notation
/// (matching big_decimal_parse) for negative scales.
fn render_big_decimal(digits: &str, negative: bool, scale: i64) -> String {
    if scale >= 0 {
        return format_scaled_digits(digits, negative, scale as usize);
    }
    let mut canonical = String::new();
    if negative && digits != "0" {
        canonical.push('-');
    }
    canonical.push_str(&digits[..1]);
    if digits.len() > 1 {
        canonical.push('.');
        canonical.push_str(&digits[1..]);
    }
    let adjusted = digits.len() as i64 - 1 - scale;
    canonical.push('E');
    if adjusted >= 0 {
        canonical.push('+');
    }
    canonical.push_str(&adjusted.to_string());
    canonical
}

/// Magnitude comparison of two mantissa digit strings with scales.
fn mantissa_cmp(da: &str, sa: i64, db: &str, sb: i64) -> Ordering {
    let da = da.trim_start_matches('0');
    let db = db.trim_start_matches('0');
    if da.is_empty() || db.is_empty() {
        return match (da.is_empty(), db.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Less,
            _ => Ordering::Greater,
        };
    }
    let adj_a = da.len() as i64 - 1 - sa;
    let adj_b = db.len() as i64 - 1 - sb;
    if adj_a != adj_b {
        return adj_a.cmp(&adj_b);
    }
    let len = da.len().max(db.len());
    let mut pa = String::from(da);
    let mut pb = String::from(db);
    while pa.len() < len {
        pa.push('0');
    }
    while pb.len() < len {
        pb.push('0');
    }
    pa.cmp(&pb)
}

/// a - b on sign-magnitude limbs.
fn limbs_signed_sub(la: &[u32], neg_a: bool, lb: &[u32], neg_b: bool) -> (Vec<u32>, bool) {
    let neg_b = !neg_b;
    if neg_a == neg_b {
        (limbs_add(la, lb), neg_a)
    } else {
        match limbs_cmp(la, lb) {
            Ordering::Equal => (Vec::new(), false),
            Ordering::Greater => (limbs_sub(la, lb), neg_a),
            Ordering::Less => (limbs_sub(lb, la), neg_b),
        }
    }
}

/// Integer part of the quotient with the JDK's preferred-scale semantics:
/// the result scale moves toward (dividend scale - divisor scale), adding
/// trailing zeros upward or stripping them downward, never past the digits.
fn big_decimal_divide_integral(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let a_text = big_text(vm, receiver(args));
    let b_text = big_text(vm, ref_arg(args, 1));
    let (neg_a, da, sa) = big_decimal_mantissa(&a_text);
    let (neg_b, db, sb) = big_decimal_mantissa(&b_text);
    if db == "0" {
        return Err(vm.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
    }
    let preferred = sa - sb;
    let (digits, negative) = if da == "0" {
        (String::from("0"), false)
    } else {
        let common = sa.max(sb);
        let pad_a = common - sa;
        let pad_b = common - sb;
        if pad_a > 1_000_000 || pad_b > 1_000_000 {
            return Err(vm.throw_new("java/lang/OutOfMemoryError", Some("Java heap space")));
        }
        let a_digits = alloc::format!("{da}{}", "0".repeat(pad_a as usize));
        let b_digits = alloc::format!("{db}{}", "0".repeat(pad_b as usize));
        let quotient = match (parse_i128(&a_digits), parse_i128(&b_digits)) {
            (Some(ua), Some(ub)) => (ua / ub).unsigned_abs().to_string(),
            _ => {
                if a_digits.len() > 100_000 {
                    return Err(vm.throw_new("java/lang/OutOfMemoryError", Some("Java heap space")));
                }
                // Decimal long division with a base-2^32 remainder; each
                // quotient digit needs at most nine trial subtractions.
                let lb = decimal_to_limbs(&b_digits);
                let mut rem: Vec<u32> = Vec::new();
                let mut out = String::with_capacity(a_digits.len());
                for byte in a_digits.bytes() {
                    let mut carry = u64::from(byte - b'0');
                    for limb in &mut rem {
                        let cur = u64::from(*limb) * 10 + carry;
                        *limb = cur as u32;
                        carry = cur >> 32;
                    }
                    if carry != 0 {
                        rem.push(carry as u32);
                    }
                    let mut digit = 0u8;
                    while limbs_cmp(&rem, &lb) != Ordering::Less {
                        rem = limbs_sub(&rem, &lb);
                        digit += 1;
                    }
                    out.push(char::from(b'0' + digit));
                }
                let trimmed = out.trim_start_matches('0');
                if trimmed.is_empty() {
                    String::from("0")
                } else {
                    String::from(trimmed)
                }
            }
        };
        (quotient, neg_a != neg_b)
    };
    let negative = negative && digits != "0";
    if preferred > 100_000_000 {
        return Err(vm.throw_new("java/lang/OutOfMemoryError", Some("Java heap space")));
    }
    let mut digits = digits;
    let mut scale = 0i64;
    if preferred > 0 {
        if digits != "0" {
            digits.push_str(&"0".repeat(preferred as usize));
        }
        scale = preferred;
    } else {
        while scale > preferred && digits.ends_with('0') && digits != "0" {
            digits.pop();
            scale -= 1;
        }
    }
    let text = render_big_decimal(&digits, negative, scale);
    create_big_number(vm, "java/math/BigDecimal", &text)
}

/// divideToIntegralValue(divisor, mc): divide to mc.precision digits with
/// ROUND_DOWN (the mode is ignored like the JDK), verify the integer part
/// fits, then adjust the scale toward the preferred scale.
fn big_decimal_divide_integral_mc(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let mc = ref_arg(args, 2);
    if mc.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let precision = crate::lang::read_int_field(vm, mc, "precision");
    let a_text = big_text(vm, receiver(args));
    let b_text = big_text(vm, ref_arg(args, 1));
    let (neg_a, da, sa) = big_decimal_mantissa(&a_text);
    let (neg_b, db, sb) = big_decimal_mantissa(&b_text);
    if precision == 0 || mantissa_cmp(&da, sa, &db, sb) == Ordering::Less {
        return big_decimal_divide_integral(vm, args);
    }
    let divided = divide_to_precision(&a_text, &b_text, precision, 1).ok_or_else(|| {
        vm.throw_new("java/lang/ArithmeticException", Some("Division impossible"))
    })?;
    let (neg_r, mut digits, mut scale) = big_decimal_mantissa(&divided);
    if db == "0" {
        return Err(vm.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
    }
    if scale < 0 {
        // The rounded-down quotient must cover the full integer part:
        // |dividend - quotient*divisor| < |divisor|.
        let product = limbs_mul(&decimal_to_limbs(&digits), &decimal_to_limbs(&db));
        let product_digits = limbs_to_decimal(product);
        let scale_p = scale + sb;
        let neg_p = neg_r != neg_b;
        let common = sa.max(scale_p);
        let pad_a = (common - sa) as usize;
        let pad_p = (common - scale_p) as usize;
        if pad_a > 1_000_000 || pad_p > 1_000_000 {
            return Err(vm.throw_new("java/lang/OutOfMemoryError", Some("Java heap space")));
        }
        let da_aligned = alloc::format!("{da}{}", "0".repeat(pad_a));
        let dp_aligned = alloc::format!("{product_digits}{}", "0".repeat(pad_p));
        let (diff, _) = limbs_signed_sub(
            &decimal_to_limbs(&da_aligned),
            neg_a,
            &decimal_to_limbs(&dp_aligned),
            neg_p,
        );
        let diff_digits = limbs_to_decimal(diff);
        let common_b = common.max(sb);
        let diff_aligned =
            alloc::format!("{diff_digits}{}", "0".repeat((common_b - common) as usize));
        let db_aligned = alloc::format!("{db}{}", "0".repeat((common_b - sb) as usize));
        if mantissa_cmp(&diff_aligned, common_b, &db_aligned, common_b) != Ordering::Less {
            return Err(vm.throw_new("java/lang/ArithmeticException", Some("Division impossible")));
        }
    } else if scale > 0 {
        // Drop fractional digits (ROUND_DOWN to scale 0).
        let plain = plain_text(&divided);
        let int_part = plain.split_once('.').map_or(plain.as_str(), |(i, _)| i);
        let int_part = int_part.strip_prefix('-').unwrap_or(int_part);
        let trimmed = int_part.trim_start_matches('0');
        digits = if trimmed.is_empty() {
            String::from("0")
        } else {
            String::from(trimmed)
        };
        scale = 0;
    }
    let preferred = sa - sb;
    let result_precision = digits.len().max(1) as i64;
    let precision_diff = i64::from(precision) - result_precision;
    if preferred > scale && precision_diff > 0 {
        let delta = precision_diff.min(preferred - scale);
        if digits != "0" {
            digits.push_str(&"0".repeat(delta as usize));
        }
        scale += delta;
    } else {
        while scale > preferred && digits.ends_with('0') && digits != "0" {
            digits.pop();
            scale -= 1;
        }
    }
    if scale > 100_000_000 {
        return Err(vm.throw_new("java/lang/OutOfMemoryError", Some("Java heap space")));
    }
    let text = render_big_decimal(&digits, neg_r && digits != "0", scale);
    create_big_number(vm, "java/math/BigDecimal", &text)
}

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

#[derive(Clone, Copy)]
enum BigOp {
    Add,
    Sub,
    Mul,
    And,
    Or,
    Xor,
}

fn big_integer_binary(vm: &mut Vm<'_>, args: &[Value], op: BigOp) -> Result<Value, VmError> {
    let this = receiver(args);
    if ref_arg(args, 1).is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let a_text = big_text(vm, this);
    let b_text = big_text(vm, ref_arg(args, 1));
    if let (Some(a), Some(b)) = (parse_i128(&a_text), parse_i128(&b_text)) {
        let result = match op {
            BigOp::Add => a.checked_add(b),
            BigOp::Sub => a.checked_sub(b),
            BigOp::Mul => a.checked_mul(b),
            BigOp::And => Some(a & b),
            BigOp::Or => Some(a | b),
            BigOp::Xor => Some(a ^ b),
        };
        if let Some(value) = result {
            return create_big_number(vm, "java/math/BigInteger", &value.to_string());
        }
    }
    let (neg_a, mag_a) = big_sign_magnitude(&a_text);
    let (neg_b, mag_b) = big_sign_magnitude(&b_text);
    let la = decimal_to_limbs(mag_a);
    let lb = decimal_to_limbs(mag_b);
    let (magnitude, negative) = match op {
        BigOp::And | BigOp::Or | BigOp::Xor => limbs_bitwise(&la, neg_a, &lb, neg_b, op),
        BigOp::Mul => (limbs_mul(&la, &lb), neg_a != neg_b),
        _ => {
            let neg_b = neg_b ^ matches!(op, BigOp::Sub);
            if neg_a == neg_b {
                (limbs_add(&la, &lb), neg_a)
            } else {
                match limbs_cmp(&la, &lb) {
                    Ordering::Equal => (Vec::new(), false),
                    Ordering::Greater => (limbs_sub(&la, &lb), neg_a),
                    Ordering::Less => (limbs_sub(&lb, &la), neg_b),
                }
            }
        }
    };
    let digits = limbs_to_decimal(magnitude);
    let text = if negative && digits != "0" {
        alloc::format!("-{digits}")
    } else {
        digits
    };
    create_big_number(vm, "java/math/BigInteger", &text)
}

/// Bitwise and/or/xor on sign-magnitude limbs via infinite-precision two's
/// complement: negatives are inverted-plus-one and sign-extended with all-1
/// words, the result sign follows the operation, then converted back.
fn limbs_bitwise(a: &[u32], neg_a: bool, b: &[u32], neg_b: bool, op: BigOp) -> (Vec<u32>, bool) {
    fn to_twos_complement(magnitude: &[u32], negative: bool) -> Vec<u32> {
        if !negative {
            return magnitude.to_vec();
        }
        let mut out: Vec<u32> = magnitude.iter().map(|word| !word).collect();
        let mut carry = 1u64;
        for word in &mut out {
            let cur = u64::from(*word) + carry;
            *word = cur as u32;
            carry = cur >> 32;
        }
        if carry != 0 {
            out.push(carry as u32);
        }
        out
    }
    let ta = to_twos_complement(a, neg_a);
    let tb = to_twos_complement(b, neg_b);
    let ext_a = if neg_a { u32::MAX } else { 0 };
    let ext_b = if neg_b { u32::MAX } else { 0 };
    let negative = match op {
        BigOp::And => neg_a && neg_b,
        BigOp::Or => neg_a || neg_b,
        _ => neg_a != neg_b,
    };
    let mut out: Vec<u32> = Vec::with_capacity(ta.len().max(tb.len()) + 1);
    for i in 0..ta.len().max(tb.len()) {
        let wa = ta.get(i).copied().unwrap_or(ext_a);
        let wb = tb.get(i).copied().unwrap_or(ext_b);
        out.push(match op {
            BigOp::And => wa & wb,
            BigOp::Or => wa | wb,
            _ => wa ^ wb,
        });
    }
    let ext_out = if negative { u32::MAX } else { 0 };
    if out.last().copied().unwrap_or(ext_out) != ext_out {
        out.push(ext_out);
    }
    if negative {
        let mut borrow = 1i64;
        for word in &mut out {
            let cur = i64::from(*word) - borrow;
            if cur < 0 {
                *word = (cur + (1i64 << 32)) as u32;
                borrow = 1;
            } else {
                *word = cur as u32;
                borrow = 0;
            }
        }
        for word in &mut out {
            *word = !*word;
        }
    }
    while out.last() == Some(&0) {
        out.pop();
    }
    (out, negative)
}

/// Split an optionally signed decimal string into (negative, magnitude digits).
fn big_sign_magnitude(text: &str) -> (bool, &str) {
    match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
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

/// Quotient and remainder of two base-2^32 magnitudes via decimal long
/// division (at most nine trial subtractions per quotient digit).
fn limbs_divmod(a: &[u32], b: &[u32]) -> (Vec<u32>, Vec<u32>) {
    let a_digits = limbs_to_decimal(a.to_vec());
    let mut rem: Vec<u32> = Vec::new();
    let mut out = String::with_capacity(a_digits.len());
    for byte in a_digits.bytes() {
        let mut carry = u64::from(byte - b'0');
        for limb in &mut rem {
            let cur = u64::from(*limb) * 10 + carry;
            *limb = cur as u32;
            carry = cur >> 32;
        }
        if carry != 0 {
            rem.push(carry as u32);
        }
        let mut digit = 0u8;
        while limbs_cmp(&rem, b) != Ordering::Less {
            rem = limbs_sub(&rem, b);
            digit += 1;
        }
        out.push(char::from(b'0' + digit));
    }
    (decimal_to_limbs(&out), rem)
}

/// Shared `mod` (euclidean, non-negative) and `remainder` (sign of the
/// dividend) with an i128 fast path and a limb fallback.
fn big_integer_mod_like(vm: &mut Vm<'_>, args: &[Value], euclid: bool) -> Result<Value, VmError> {
    if ref_arg(args, 1).is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let a_text = big_text(vm, receiver(args));
    let b_text = big_text(vm, ref_arg(args, 1));
    if let (Some(a), Some(b)) = (parse_i128(&a_text), parse_i128(&b_text)) {
        if b == 0 {
            return Err(vm.throw_new(
                "java/lang/ArithmeticException",
                Some("BigInteger divide by zero"),
            ));
        }
        if euclid {
            if b < 0 {
                return Err(vm.throw_new(
                    "java/lang/ArithmeticException",
                    Some("BigInteger: modulus not positive"),
                ));
            }
            return create_big_number(vm, "java/math/BigInteger", &a.rem_euclid(b).to_string());
        }
        return create_big_number(vm, "java/math/BigInteger", &(a % b).to_string());
    }
    let (neg_a, mag_a) = big_sign_magnitude(&a_text);
    let (neg_b, mag_b) = big_sign_magnitude(&b_text);
    let mag_a = mag_a.trim_start_matches('0');
    let mag_b = mag_b.trim_start_matches('0');
    if mag_b.is_empty() {
        return Err(vm.throw_new(
            "java/lang/ArithmeticException",
            Some("BigInteger divide by zero"),
        ));
    }
    if euclid && neg_b {
        return Err(vm.throw_new(
            "java/lang/ArithmeticException",
            Some("BigInteger: modulus not positive"),
        ));
    }
    if mag_a.len() > 100_000 || mag_b.len() > 100_000 {
        return Err(vm.throw_new("java/lang/OutOfMemoryError", Some("Java heap space")));
    }
    let b = decimal_to_limbs(mag_b);
    let (_, mut rem) = limbs_divmod(&decimal_to_limbs(mag_a), &b);
    let negative = if euclid {
        if neg_a && !rem.is_empty() {
            rem = limbs_sub(&b, &rem);
        }
        false
    } else {
        neg_a
    };
    let digits = limbs_to_decimal(rem);
    let text = if negative && digits != "0" {
        alloc::format!("-{digits}")
    } else {
        digits
    };
    create_big_number(vm, "java/math/BigInteger", &text)
}

fn big_integer_mod_inverse(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let a_text = big_text(vm, receiver(args));
    let m_text = big_text(vm, ref_arg(args, 1));
    if let (Some(a), Some(m)) = (parse_i128(&a_text), parse_i128(&m_text)) {
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
        return create_big_number(vm, "java/math/BigInteger", &old_s.rem_euclid(m).to_string());
    }
    let (neg_a, mag_a) = big_sign_magnitude(&a_text);
    let (neg_m, mag_m) = big_sign_magnitude(&m_text);
    let mag_a = mag_a.trim_start_matches('0');
    let mag_m = mag_m.trim_start_matches('0');
    if neg_m || mag_m.is_empty() {
        return Err(vm.throw_new(
            "java/lang/ArithmeticException",
            Some("BigInteger: modulus not positive"),
        ));
    }
    if mag_a.len() > 100_000 || mag_m.len() > 100_000 {
        return Err(vm.throw_new("java/lang/OutOfMemoryError", Some("Java heap space")));
    }
    let m = decimal_to_limbs(mag_m);
    let (_, mut rem) = limbs_divmod(&decimal_to_limbs(mag_a), &m);
    if neg_a && !rem.is_empty() {
        rem = limbs_sub(&m, &rem);
    }
    // Extended Euclid with signed base-2^32 coefficients.
    let (mut old_r, mut r) = (rem, m.clone());
    let (mut old_s, mut s) = ((false, decimal_to_limbs("1")), (false, Vec::new()));
    while !r.is_empty() {
        let (q, new_r) = limbs_divmod(&old_r, &r);
        old_r = r;
        r = new_r;
        let qs_mag = limbs_mul(&q, &s.1);
        let qs_neg = s.0 && !qs_mag.is_empty();
        let (new_mag, new_neg) = limbs_signed_sub(&old_s.1, old_s.0, &qs_mag, qs_neg);
        old_s = s;
        s = (new_neg, new_mag);
    }
    if limbs_cmp(&old_r, &decimal_to_limbs("1")) != Ordering::Equal {
        return Err(vm.throw_new(
            "java/lang/ArithmeticException",
            Some("BigInteger not invertible"),
        ));
    }
    let (_, mut result) = limbs_divmod(&old_s.1, &m);
    if old_s.0 && !result.is_empty() {
        result = limbs_sub(&m, &result);
    }
    create_big_number(vm, "java/math/BigInteger", &limbs_to_decimal(result))
}

const BIG_INTEGER_METHODS: [oxjvm_vm::NativeMethodDef; 54] = [
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
    method("floatValue", "()F", ACC_PUBLIC, |vm, _, args| {
        let text = big_text(vm, receiver(args));
        float(text.parse::<f32>().unwrap_or(f32::NAN))
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
        |vm, _, args| big_integer_binary(vm, args, BigOp::Add),
    ),
    method(
        "subtract",
        "(Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| big_integer_binary(vm, args, BigOp::Sub),
    ),
    method(
        "multiply",
        "(Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| big_integer_binary(vm, args, BigOp::Mul),
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
            let bits = int_arg(args, 1);
            let random = ref_arg(args, 2);
            if bits < 0 {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("numBits must be non-negative"),
                ));
            }
            if random.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            if bits == 0 {
                return write_big_text(vm, receiver(args), "0");
            }
            if bits > 1_000_000 {
                return Err(vm.throw_new("java/lang/OutOfMemoryError", Some("Java heap space")));
            }
            let nwords = (bits as u32).div_ceil(32) as usize;
            let mut limbs: Vec<u32> = Vec::with_capacity(nwords);
            for _ in 0..nwords {
                let word = match crate::util::next_random_int(vm, random, 32, &[])? {
                    Value::Int(value) => value as u32,
                    _ => 0,
                };
                limbs.insert(0, word);
            }
            let excess = nwords as u32 * 32 - bits as u32;
            if excess > 0 {
                if let Some(top) = limbs.last_mut() {
                    *top &= u32::MAX >> excess;
                }
            }
            while limbs.last() == Some(&0) {
                limbs.pop();
            }
            let text = limbs_to_decimal(limbs);
            write_big_text(vm, receiver(args), &text)
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
            if shift == i32::MIN {
                // Negating MIN_VALUE overflows; the JDK rejects the shift
                // outright unless the value is zero.
                if text.bytes().all(|b| b == b'0' || b == b'-' || b == b'+') {
                    return create_big_number(vm, "java/math/BigInteger", "0");
                }
                return Err(vm.throw_new(
                    "java/lang/ArithmeticException",
                    Some("Shift distance too large"),
                ));
            }
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
        |vm, _, args| big_integer_mod_like(vm, args, false),
    ),
    method("bitLength", "()I", ACC_PUBLIC, |vm, _, args| {
        let text = big_text(vm, receiver(args));
        let (negative, magnitude) = big_sign_magnitude(&text);
        let magnitude = magnitude.trim_start_matches('0');
        if magnitude.is_empty() {
            return int(0);
        }
        // For negatives the JDK measures the minimal two's-complement
        // representation, which is bitlen(magnitude - 1).
        let digits = if negative {
            dec_sub_one(magnitude)
        } else {
            String::from(magnitude)
        };
        let limbs = decimal_to_limbs(&digits);
        let Some(&top) = limbs.last() else {
            return int(0);
        };
        int(((limbs.len() - 1) * 32 + (32 - top.leading_zeros()) as usize) as i32)
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
        let certainty = int_arg(args, 1);
        if certainty <= 0 {
            return boolean(true);
        }
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
        |vm, _, args| big_integer_mod_like(vm, args, true),
    ),
    method(
        "modInverse",
        "(Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| big_integer_mod_inverse(vm, args),
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
        |vm, _, args| big_integer_binary(vm, args, BigOp::And),
    ),
    method(
        "or",
        "(Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| big_integer_binary(vm, args, BigOp::Or),
    ),
    method(
        "xor",
        "(Ljava/math/BigInteger;)Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| big_integer_binary(vm, args, BigOp::Xor),
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
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (mantissa, exponent) = match body.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i64>().ok()?),
        None => (body, 0),
    };
    let (int_part, frac_part) = match mantissa.split_once('.') {
        Some((i, f)) => (i, f),
        None => (mantissa, ""),
    };
    let digits = alloc::format!("{int_part}{frac_part}");
    let scale = frac_part.len() as i64 - exponent;
    let sig_start = digits
        .bytes()
        .position(|b| b != b'0')
        .unwrap_or(digits.len());
    let current_sig = (digits.len() - sig_start) as i64;
    if current_sig <= i64::from(precision) {
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
    let mut drop = current_sig - i64::from(precision);
    if round_up {
        kept = dec_add_one(&kept);
        if kept.len() > keep {
            // 999.. -> 1000..0: carry into a new exponent, keep p digits.
            kept.pop();
            drop += 1;
        }
    }
    let new_scale = scale - drop;
    // The JDK raises ArithmeticException when the rounded scale or adjusted
    // exponent leaves the int range (e.g. valueOf(11, Integer.MIN_VALUE)
    // rounded to one digit would need exponent 2147483648).
    if new_scale > i64::from(i32::MAX) || new_scale < i64::from(i32::MIN) {
        return None;
    }
    if kept.len() as i64 - 1 - new_scale > i64::from(i32::MAX) {
        return None;
    }
    let rendered = if new_scale > 0 {
        if kept.len() as i64 <= new_scale {
            let zeros = (new_scale - kept.len() as i64) as usize;
            alloc::format!("0.{}{}", "0".repeat(zeros), kept)
        } else {
            let point = kept.len() as i64 - new_scale;
            alloc::format!("{}.{}", &kept[..point as usize], &kept[point as usize..])
        }
    } else if new_scale == 0 {
        kept
    } else {
        big_decimal_with_scale(&kept, new_scale as i32)
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
pub(crate) fn plain_text(text: &str) -> String {
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

/// The unscaled value and scale a BigDecimal text denotes, the pair its equality and
/// hash code are defined over.
fn decimal_key(text: &str) -> (String, i32) {
    let (mantissa, exponent) = match text.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().unwrap_or(0)),
        None => (text, 0),
    };
    let (negative, digits) = if let Some(stripped) = mantissa.strip_prefix('-') {
        (true, stripped)
    } else {
        (false, mantissa.strip_prefix('+').unwrap_or(mantissa))
    };
    let (int_part, frac_part) = match digits.split_once('.') {
        Some((i, f)) => (i, f),
        None => (digits, ""),
    };
    let digits = alloc::format!("{int_part}{frac_part}");
    let trimmed = digits.trim_start_matches('0');
    let mut unscaled = if trimmed.is_empty() {
        String::from("0")
    } else {
        trimmed.to_string()
    };
    if negative && unscaled != "0" {
        unscaled = alloc::format!("-{unscaled}");
    }
    (unscaled, frac_part.len() as i32 - exponent)
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

const BIG_DECIMAL_METHODS: [oxjvm_vm::NativeMethodDef; 62] = [
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
            // BigDecimal equality is numeric at a fixed scale: same unscaled value and
            // same scale, regardless of how the text was written (`1E1` versus `10`).
            let a = big_text(vm, receiver(args));
            let b = big_text(vm, other);
            boolean(decimal_key(&a) == decimal_key(&b))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let (unscaled, scale) = decimal_key(&big_text(vm, receiver(args)));
        int(java_hash(&unscaled).wrapping_mul(31).wrapping_add(scale))
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
            let n = int_arg(args, 1);
            // Keep the unscaled mantissa and only adjust the scale, like the
            // JDK: ONE.scaleByPowerOfTen(1) is 1E+1, not the expanded "10".
            let (negative, body) = match text.strip_prefix('-') {
                Some(rest) => (true, rest),
                None => (false, text.strip_prefix('+').unwrap_or(&text)),
            };
            let (mantissa, exponent) = match body.split_once(['e', 'E']) {
                Some((m, e)) => (m, e.parse::<i64>().unwrap_or(0)),
                None => (body, 0),
            };
            let (int_part, frac_part) = match mantissa.split_once('.') {
                Some((i, f)) => (i, f),
                None => (mantissa, ""),
            };
            let digits = alloc::format!("{int_part}{frac_part}");
            let digits = digits.trim_start_matches('0');
            let digits = if digits.is_empty() { "0" } else { digits };
            let scale = frac_part.len() as i64 - exponent;
            let new_scale = scale - i64::from(n);
            if new_scale > i64::from(i32::MAX) {
                return Err(vm.throw_new("java/lang/ArithmeticException", Some("Overflow")));
            }
            if new_scale < i64::from(i32::MIN) {
                return Err(vm.throw_new("java/lang/ArithmeticException", Some("Underflow")));
            }
            let rendered = if new_scale >= 0 {
                format_scaled_digits(digits, negative, new_scale as usize)
            } else {
                let signed = if negative && digits != "0" {
                    alloc::format!("-{digits}")
                } else {
                    String::from(digits)
                };
                big_decimal_with_scale(&signed, new_scale as i32)
            };
            create_big_number(vm, "java/math/BigDecimal", &rendered)
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
        |vm, _, args| big_decimal_divide_exact(vm, args),
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
        "divide",
        "(Ljava/math/BigDecimal;II)Ljava/math/BigDecimal;",
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
            // The unscaled value is the mantissa digit string, never the
            // exponent-expanded form: valueOf(4, -5).unscaledValue() is 4.
            let (negative, body) = match raw.strip_prefix('-') {
                Some(rest) => (true, rest),
                None => (false, raw.strip_prefix('+').unwrap_or(&raw)),
            };
            let mantissa = body.split_once(['e', 'E']).map_or(body, |(m, _)| m);
            let (int_part, frac_part) = match mantissa.split_once('.') {
                Some((i, f)) => (i, f),
                None => (mantissa, ""),
            };
            let digits = alloc::format!("{int_part}{frac_part}");
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
        |vm, _, args| big_decimal_divide_integral(vm, args),
    ),
    method(
        "divideToIntegralValue",
        "(Ljava/math/BigDecimal;Ljava/math/MathContext;)Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| big_decimal_divide_integral_mc(vm, args),
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
