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

/// Parse a `float` like `Float.parseFloat`, including `NaN`, `Infinity`, and hex literals.
#[must_use]
pub fn parse_float(text: &str) -> Option<f32> {
    let trimmed = text.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return parse_hex_float(hex).map(|value| value as f32);
    }
    match trimmed {
        "NaN" => Some(f32::NAN),
        "Infinity" | "+Infinity" => Some(f32::INFINITY),
        "-Infinity" => Some(f32::NEG_INFINITY),
        _ => trimmed.to_ascii_lowercase().parse::<f32>().ok(),
    }
}

/// Parse a `double` like `Double.parseDouble`.
#[must_use]
pub fn parse_double(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return parse_hex_float(hex);
    }
    match trimmed {
        "NaN" => Some(f64::NAN),
        "Infinity" | "+Infinity" => Some(f64::INFINITY),
        "-Infinity" => Some(f64::NEG_INFINITY),
        _ => trimmed.to_ascii_lowercase().parse::<f64>().ok(),
    }
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
        let exponent: i32 = exponent.parse().ok()?;
        value *= pow2(exponent);
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
    let mut value = 1.0f64;
    if exponent >= 0 {
        for _ in 0..exponent {
            value *= 2.0;
        }
    } else {
        for _ in 0..-exponent {
            value *= 0.5;
        }
    }
    value
}
