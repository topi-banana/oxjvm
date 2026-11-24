//! Java's primitive-to-string and string-to-primitive conversions.
//!
//! These mirror `Integer.toString`, `Long.toString`, `Float.toString`, `Double.toString`, and the
//! `parseX` families. Rust's shortest-round-trip float formatting matches Java's shortest-decimal
//! contract; the only Java-specific work is the plain/scientific notation split and the exponent
//! spelling (`E`, always at least one fractional digit).
//!
//! The parsers return `Option`: `None` is exactly the input class for which the JDK throws
//! `NumberFormatException`, and native code maps it to that exception.

use alloc::string::{String, ToString};

/// Format an `int` exactly like `Integer.toString`.
#[must_use]
pub fn int_to_string(value: i32) -> String {
    value.to_string()
}

/// Format a `long` exactly like `Long.toString`.
#[must_use]
pub fn long_to_string(value: i64) -> String {
    value.to_string()
}

/// Format a `float` like `Float.toString`.
#[must_use]
pub fn float_to_string(value: f32) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value == f32::INFINITY {
        return "Infinity".into();
    }
    if value == f32::NEG_INFINITY {
        return "-Infinity".into();
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0".into()
        } else {
            "0.0".into()
        };
    }
    // Subnormals use the JDK's Schubfach rendering, whose decimal grid is
    // pinned to the minimum exponent rather than the value's magnitude.
    let bits = value.to_bits();
    let mantissa = u64::from(bits & 0x007F_FFFF);
    if bits & 0x7F80_0000 == 0 && mantissa != 0 {
        let magnitude = subnormal_to_string(mantissa, false);
        return if value.is_sign_negative() {
            alloc::format!("-{magnitude}")
        } else {
            magnitude
        };
    }
    let magnitude = value.abs();
    if (1e-3..1e7).contains(&f64::from(magnitude)) {
        let mut text = alloc::format!("{value}");
        if !text.contains('.') && !text.contains('e') && !text.contains('E') {
            text.push_str(".0");
        }
        text
    } else {
        scientific(&alloc::format!("{value:e}"))
    }
}

/// Format a `double` like `Double.toString`.
#[must_use]
pub fn double_to_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value == f64::INFINITY {
        return "Infinity".into();
    }
    if value == f64::NEG_INFINITY {
        return "-Infinity".into();
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0".into()
        } else {
            "0.0".into()
        };
    }
    // Subnormals use the JDK's Schubfach rendering, whose decimal grid is
    // pinned to the minimum exponent rather than the value's magnitude.
    let bits = value.to_bits();
    let mantissa = bits & 0x000F_FFFF_FFFF_FFFF;
    if bits & 0x7FF0_0000_0000_0000 == 0 && mantissa != 0 {
        let magnitude = subnormal_to_string(mantissa, true);
        return if value.is_sign_negative() {
            alloc::format!("-{magnitude}")
        } else {
            magnitude
        };
    }
    let magnitude = value.abs();
    if (1e-3..1e7).contains(&magnitude) {
        let mut text = alloc::format!("{value}");
        if !text.contains('.') && !text.contains('e') && !text.contains('E') {
            text.push_str(".0");
        }
        text
    } else {
        scientific(&alloc::format!("{value:e}"))
    }
}

/// Render a subnormal magnitude exactly as the JDK's Schubfach algorithm does.
/// For subnormals the binary exponent is pinned at `Q_MIN`, so the decimal grid
/// `10^floor(log10(2^Q_MIN))` is fixed; this yields more significant digits than
/// a magnitude-based shortest form for small subnormals (e.g. `4.9E-324`).
fn subnormal_to_string(mantissa: u64, double: bool) -> String {
    const MASK_63: u64 = (1 << 63) - 1;

    fn multiply_high(a: u64, b: u64) -> u64 {
        ((u128::from(a) * u128::from(b)) >> 64) as u64
    }

    /// The rounded high part of `cp * (g1 2^63 + g0) * 2^-127`.
    fn rop(g1: u64, g0: u64, cp: u64) -> u64 {
        let x1 = multiply_high(g0, cp);
        let y0 = (u128::from(g1) * u128::from(cp)) as u64;
        let y1 = multiply_high(g1, cp);
        let z = (y0 >> 1) + x1;
        let vbp = y1 + (z >> 63);
        vbp | (((z & MASK_63) + MASK_63) >> 63)
    }

    /// Format `f * 10^e` in Java scientific notation (always used for subnormals).
    fn format_subnormal(f: u64, e: i32) -> String {
        let digits = alloc::format!("{f}");
        let exponent = e + digits.len() as i32 - 1;
        let mut fraction = if digits.len() > 1 {
            digits[1..].to_string()
        } else {
            String::from("0")
        };
        while fraction.len() > 1 && fraction.ends_with('0') {
            fraction.pop();
        }
        alloc::format!("{}.{}E{}", &digits[..1], fraction, exponent)
    }

    let (q_min, c_tiny, k, g1, g0, flog2): (i32, u64, i32, u64, u64, i32) = if double {
        (
            -1074,
            3,
            -324,
            0x4F0C_EDC9_5A71_8DD4,
            0x5B01_E8B0_9AA0_D1B5,
            1076,
        )
    } else {
        (
            -149,
            8,
            -45,
            0x59AE_DFC1_0D72_79C5,
            0x7768_A00B_22A0_0001,
            149,
        )
    };
    let (c, dk) = if mantissa < c_tiny {
        (10 * mantissa, -1i32)
    } else {
        (mantissa, 0)
    };
    let out = c & 1;
    let cb = c << 2;
    let cbr = cb + 2;
    let cbl = cb - 2;
    let h = (q_min + flog2 + 2) as u32;
    let vb = rop(g1, g0, cb << h);
    let vbl = rop(g1, g0, cbl << h);
    let vbr = rop(g1, g0, cbr << h);
    let s = vb >> 2;
    if s >= 100 {
        let sp10 = 10 * multiply_high(s, 115_292_150_460_684_698 << 4);
        let tp10 = sp10 + 10;
        let upin = vbl + out <= sp10 << 2;
        let wpin = (tp10 << 2) + out <= vbr;
        if upin != wpin {
            return format_subnormal(if upin { sp10 } else { tp10 }, k);
        }
    }
    let t = s + 1;
    let uin = vbl + out <= s << 2;
    let win = (t << 2) + out <= vbr;
    if uin != win {
        return format_subnormal(if uin { s } else { t }, k + dk);
    }
    let cmp = vb as i64 - ((s + t) << 1) as i64;
    let away = cmp > 0 || (cmp == 0 && (s & 1) != 0);
    format_subnormal(if away { t } else { s }, k + dk)
}

fn scientific(rust_exponent: &str) -> String {
    let Some((mantissa, exponent)) = rust_exponent.split_once('e') else {
        return rust_exponent.into();
    };
    let mut mantissa = mantissa.to_string();
    if !mantissa.contains('.') {
        mantissa.push_str(".0");
    }
    alloc::format!("{mantissa}E{exponent}")
}

/// Parse an `int` exactly like `Integer.parseInt`: optional sign, base 10, overflow rejected.
#[must_use]
pub fn parse_int(text: &str) -> Option<i32> {
    parse_long(text)?.try_into().ok()
}

/// Parse a `long` exactly like `Long.parseLong`.
#[must_use]
pub fn parse_long(text: &str) -> Option<i64> {
    parse_long_radix(text, 10)
}

/// Parse with a radix, like `Integer.parseInt(s, radix)`.
#[must_use]
pub fn parse_int_radix(text: &str, radix: u32) -> Option<i32> {
    parse_long_radix(text, radix)?.try_into().ok()
}

/// Parse with a radix, like `Long.parseLong(s, radix)`.
#[must_use]
pub fn parse_long_radix(text: &str, radix: u32) -> Option<i64> {
    if !(2..=36).contains(&radix) {
        return None;
    }
    let bytes = text.as_bytes();
    let (negative, digits) = match bytes.first()? {
        b'-' => (true, &bytes[1..]),
        b'+' => (false, &bytes[1..]),
        _ => (false, bytes),
    };
    if digits.is_empty() {
        return None;
    }
    let mut value: i64 = 0;
    for &byte in digits {
        let digit = match byte {
            b'0'..=b'9' => u32::from(byte - b'0'),
            b'a'..=b'z' => u32::from(byte - b'a') + 10,
            b'A'..=b'Z' => u32::from(byte - b'A') + 10,
            _ => return None,
        };
        if digit >= radix {
            return None;
        }
        let digit = i64::from(digit);
        value = value.checked_mul(i64::from(radix))?;
        value = if negative {
            value.checked_sub(digit)?
        } else {
            value.checked_add(digit)?
        };
    }
    Some(value)
}

/// Parse an unsigned `u64` with a radix, like `Long.parseUnsignedLong(s, radix)`.
/// A leading `-` is rejected; `+` is allowed; overflow yields `None`.
#[must_use]
pub fn parse_ulong_radix(text: &str, radix: u32) -> Option<u64> {
    if !(2..=36).contains(&radix) {
        return None;
    }
    let bytes = text.as_bytes();
    let digits = match bytes.first()? {
        b'-' => return None,
        b'+' => &bytes[1..],
        _ => bytes,
    };
    if digits.is_empty() {
        return None;
    }
    let mut value: u64 = 0;
    for &byte in digits {
        let digit = match byte {
            b'0'..=b'9' => u32::from(byte - b'0'),
            b'a'..=b'z' => u32::from(byte - b'a') + 10,
            b'A'..=b'Z' => u32::from(byte - b'A') + 10,
            _ => return None,
        };
        if digit >= radix {
            return None;
        }
        value = value.checked_mul(u64::from(radix))?;
        value = value.checked_add(u64::from(digit))?;
    }
    Some(value)
}

/// Parse an unsigned `u32` with a radix, like `Integer.parseUnsignedInt(s, radix)`.
#[must_use]
pub fn parse_uint_radix(text: &str, radix: u32) -> Option<u32> {
    parse_ulong_radix(text, radix)?.try_into().ok()
}

/// Format an `i64` in an arbitrary radix, like `Long.toBinaryString`/`toHexString`/`toString`.
#[must_use]
pub fn long_to_radix_string(mut value: i64, radix: u32) -> String {
    if radix == 10 {
        return value.to_string();
    }
    if value == 0 {
        return "0".into();
    }
    let negative = value < 0;
    let mut digits = [0u8; 65];
    let mut count = 0;
    // Operate on the two's-complement magnitude so `i64::MIN` formats correctly.
    while value != 0 {
        let digit = (value % i64::from(radix)).unsigned_abs() as u32;
        digits[count] = core::char::from_digit(digit, radix).unwrap_or('?') as u8;
        count += 1;
        value /= i64::from(radix);
    }
    if negative {
        digits[count] = b'-';
        count += 1;
    }
    let mut out = String::with_capacity(count);
    for index in (0..count).rev() {
        out.push(digits[index] as char);
    }
    out
}

/// Format an `i32` in an arbitrary radix.
#[must_use]
pub fn int_to_radix_string(value: i32, radix: u32) -> String {
    if radix == 10 {
        return value.to_string();
    }
    if value == 0 {
        return "0".into();
    }
    let negative = value < 0;
    let mut digits = [0u8; 34];
    let mut count = 0;
    let mut value = value;
    while value != 0 {
        let digit = (value % radix as i32).unsigned_abs();
        digits[count] = core::char::from_digit(digit, radix).unwrap_or('?') as u8;
        count += 1;
        value /= radix as i32;
    }
    if negative {
        digits[count] = b'-';
        count += 1;
    }
    let mut out = String::with_capacity(count);
    for index in (0..count).rev() {
        out.push(digits[index] as char);
    }
    out
}

/// Format a `u64` in an arbitrary radix (unsigned, as `Long.toBinaryString` etc. require).
#[must_use]
pub fn ulong_to_radix_string(mut value: u64, radix: u32) -> String {
    if radix == 10 {
        return value.to_string();
    }
    if value == 0 {
        return "0".into();
    }
    let mut digits = [0u8; 65];
    let mut count = 0;
    while value != 0 {
        let digit = (value % u64::from(radix)) as u32;
        digits[count] = core::char::from_digit(digit, radix).unwrap_or('?') as u8;
        count += 1;
        value /= u64::from(radix);
    }
    let mut out = String::with_capacity(count);
    for index in (0..count).rev() {
        out.push(digits[index] as char);
    }
    out
}

/// Strip the optional trailing `f`/`F`/`d`/`D` suffix that `Float.parseFloat`
/// and `Double.parseDouble` accept on a decimal literal.
fn strip_float_suffix(text: &str) -> &str {
    match text.as_bytes().last() {
        Some(b'f' | b'F' | b'd' | b'D') => &text[..text.len() - 1],
        _ => text,
    }
}

/// Whether `text` spells NaN or Infinity in a case the JDK does not accept.
/// The exact `NaN`/`Infinity` spellings are matched by the caller; without this
/// guard Rust's parser would accept case-insensitive nan/inf words.
fn is_nan_inf_word(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let body = lower.strip_prefix(['+', '-']).unwrap_or(&lower);
    matches!(body, "nan" | "inf" | "infinity")
}

/// If `text` is a hex float literal (optional sign, `0x`/`0X`, mantissa, `p`
/// exponent), return the body `parse_hex_float` expects: the sign followed by
/// the mantissa and exponent, with the `0x` prefix removed.
fn hex_float_body(text: &str) -> Option<String> {
    let (sign, rest) = if let Some(rest) = text.strip_prefix('-') {
        ("-", rest)
    } else if let Some(rest) = text.strip_prefix('+') {
        ("+", rest)
    } else {
        ("", text)
    };
    let hex = rest
        .strip_prefix("0x")
        .or_else(|| rest.strip_prefix("0X"))?;
    Some(alloc::format!("{sign}{hex}"))
}

/// Parse a `float` like `Float.parseFloat`, including `NaN`, `Infinity`, and hex literals.
#[must_use]
pub fn parse_float(text: &str) -> Option<f32> {
    let trimmed = text.trim();
    if let Some(hex) = hex_float_body(trimmed) {
        return parse_hex_float(&hex).map(|value| value as f32);
    }
    match trimmed {
        "NaN" | "+NaN" | "-NaN" => return Some(f32::NAN),
        "Infinity" | "+Infinity" => return Some(f32::INFINITY),
        "-Infinity" => return Some(f32::NEG_INFINITY),
        _ => {}
    }
    let body = strip_float_suffix(trimmed);
    if is_nan_inf_word(body) {
        return None;
    }
    body.parse::<f32>().ok()
}

/// Parse a `double` like `Double.parseDouble`.
#[must_use]
pub fn parse_double(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if let Some(hex) = hex_float_body(trimmed) {
        return parse_hex_float(&hex);
    }
    match trimmed {
        "NaN" | "+NaN" | "-NaN" => return Some(f64::NAN),
        "Infinity" | "+Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    let body = strip_float_suffix(trimmed);
    if is_nan_inf_word(body) {
        return None;
    }
    body.parse::<f64>().ok()
}

fn parse_hex_float(text: &str) -> Option<f64> {
    let (mantissa, exponent) = split_hex_exponent(text);
    let (negative, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, mantissa.strip_prefix('+').unwrap_or(mantissa)),
    };
    let (integer, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if integer.is_empty() && fraction.is_empty() {
        return None;
    }
    let mut value = 0.0f64;
    for ch in integer.chars() {
        value = value * 16.0 + f64::from(ch.to_digit(16)? as u8);
    }
    let mut scale = 1.0f64 / 16.0;
    for ch in fraction.chars() {
        value += f64::from(ch.to_digit(16)? as u8) * scale;
        scale /= 16.0;
    }
    if let Some(exponent) = exponent {
        // A trailing float/double suffix (f/F/d/D) is allowed after the binary
        // exponent, e.g. "0x.003p12f"; strip it before parsing the exponent.
        let exponent = exponent
            .strip_suffix(['f', 'F', 'd', 'D'])
            .unwrap_or(exponent);
        let exponent: i64 = exponent.parse().ok()?;
        // A zero significand stays zero for any exponent (0 * 2^huge would
        // otherwise become NaN once pow2 saturates to infinity).  The exponent
        // can exceed i32 (the JDK accepts up to long range); pow2 saturates well
        // before then, so clamping preserves the over/underflow behaviour.
        if value != 0.0 {
            value *= pow2(exponent.clamp(-2000, 2000) as i32);
        }
    }
    Some(if negative { -value } else { value })
}

fn split_hex_exponent(text: &str) -> (&str, Option<&str>) {
    if let Some(index) = text.rfind(['p', 'P']) {
        (&text[..index], Some(&text[index + 1..]))
    } else {
        (text, None)
    }
}

fn pow2(exponent: i32) -> f64 {
    // Construct 2^exponent directly from the IEEE-754 bit layout instead of
    // looping (a literal exponent can be as large as 2^31, which would spin for
    // billions of iterations).  Saturate to infinity / zero outside the
    // representable range so the caller's multiply over/underflows correctly.
    if exponent >= 1024 {
        return f64::INFINITY;
    }
    if exponent <= -1075 {
        return 0.0;
    }
    if exponent >= -1022 {
        f64::from_bits(((exponent + 1023) as u64) << 52)
    } else {
        f64::from_bits(1u64 << (exponent + 1074))
    }
}
