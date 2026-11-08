/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! JavaScript number utilities for safe integer operations.
//!
//! In JavaScript, `Number.MAX_SAFE_INTEGER` (2^53-1) represents the maximum safe integer value,
//! and `Number.MIN_SAFE_INTEGER` (-2^53+1) represents the minimum safe integer value.

use ryu::Buffer;

/// In JavaScript, `Number.MAX_SAFE_INTEGER` (2^53-1)
/// represents the maximum safe integer value.
pub const MAX_SAFE_INTEGER: f64 = 9007199254740991.0;

/// In JavaScript, `Number.MIN_SAFE_INTEGER` (-2^53+1)
/// represents the minimum safe integer value.
pub const MIN_SAFE_INTEGER: f64 = -9007199254740991.0;

/// Checks if a value is within the JavaScript safe integer range.
///
/// Returns `true` if the value is between `MIN_SAFE_INTEGER` and `MAX_SAFE_INTEGER` (inclusive).
pub fn in_safe_integer_range(value: f64) -> bool {
    value >= MIN_SAFE_INTEGER && value <= MAX_SAFE_INTEGER
}

/// Checks if a float value is a safe integer in JavaScript.
///
/// Returns `true` if the value is an integer and within the safe integer range.
pub fn is_float_safe_integer(value: f64) -> bool {
    value.is_finite() && value.fract() == 0.0 && in_safe_integer_range(value)
}

/// Formats a float according to ECMAScript's decimal Number::toString rules,
/// including Note 2's closest-decimal, ties-to-even recommendation.
/// See https://tc39.es/ecma262/#sec-numeric-types-number-tostring.
pub fn ecma_string_of_float(v: f64) -> String {
    // 1. If NaN, return "NaN"
    if v.is_nan() {
        return "NaN".to_owned();
    }
    // 2. If +0 or -0, return "0"
    if v == 0.0 {
        return "0".to_owned();
    }
    // 3. If negative, return "-" + ToString(-v)
    if v < 0.0 {
        return format!("-{}", ecma_string_of_float(-v));
    }
    // 4. If +Infinity, return "Infinity"
    if v.is_infinite() {
        return "Infinity".to_owned();
    }

    // Ryū supplies the closest shortest decimal, with ties rounded to even.
    // Extract its significant digits and exponent for ECMAScript formatting.
    let (digits, n) = shortest_decimal_digits(v);
    let k = digits.len() as i32;

    // ECMAScript formatting rules (where s are the significant digits,
    // k is the number of significant digits, and n is such that
    // the value = s * 10^(n-k)):
    if k <= n && n <= 21 {
        // Case: integer-like, e.g. 120, 1000
        let mut result = digits;
        for _ in 0..(n - k) {
            result.push('0');
        }
        result
    } else if 0 < n && n <= 21 {
        // Case: decimal with digits on both sides, e.g. 12.34
        let mut result = String::with_capacity(k as usize + 1);
        result.push_str(&digits[..n as usize]);
        result.push('.');
        result.push_str(&digits[n as usize..]);
        result
    } else if -6 < n && n <= 0 {
        // Case: 0.00...0digits, e.g. 0.000001
        let mut result = String::with_capacity(2 + (-n) as usize + k as usize);
        result.push_str("0.");
        for _ in 0..(-n) {
            result.push('0');
        }
        result.push_str(&digits);
        result
    } else if k == 1 {
        // Case: scientific notation with single digit, e.g. 1e+21, 1e-7
        let mut result = digits;
        result.push('e');
        if n - 1 > 0 {
            result.push('+');
        }
        result.push_str(&(n - 1).to_string());
        result
    } else {
        // Case: scientific notation with multiple digits, e.g. 1.23e+25
        let mut result = String::with_capacity(k as usize + 5);
        result.push(digits.as_bytes()[0] as char);
        result.push('.');
        result.push_str(&digits[1..]);
        result.push('e');
        if n - 1 > 0 {
            result.push('+');
        }
        result.push_str(&(n - 1).to_string());
        result
    }
}

/// Produces the shortest decimal string representation of a float value.
///
/// Unlike `ecma_string_of_float` which follows ECMAScript's `Number::toString()`
/// formatting rules, this function actively tries all possible representations
/// (fixed notation, exponential with various coefficient/exponent splits) and
/// returns the shortest one.
///
/// Examples:
/// - `shortest_string_of_float(0.0000001)` -> `"1e-7"` (not `"0.0000001"`)
/// - `shortest_string_of_float(1000.0)` -> `"1e3"` (not `"1000"`)
/// - `shortest_string_of_float(100.0)` -> `"100"` (same length as `"1e2"`, prefers fixed)
/// - `shortest_string_of_float(2592000000.0)` -> `"2592e6"`
pub fn shortest_string_of_float(v: f64) -> String {
    // Handle special values
    if v.is_nan() {
        return "NaN".to_owned();
    }
    if v == 0.0 {
        return "0".to_owned();
    }
    if v < 0.0 {
        return format!("-{}", shortest_string_of_float(-v));
    }
    if v.is_infinite() {
        return "Infinity".to_owned();
    }

    let (digits, n) = shortest_decimal_digits(v);
    let k = digits.len() as i32;

    // Generate fixed notation representation
    let fixed = if k <= n {
        // Integer-like, e.g., value 1000 with digits="1", n=4 -> "1000"
        let mut s = digits.clone();
        for _ in 0..(n - k) {
            s.push('0');
        }
        s
    } else if n > 0 {
        // Decimal with digits on both sides, e.g., 123.456
        let mut s = String::with_capacity(k as usize + 1);
        s.push_str(&digits[..n as usize]);
        s.push('.');
        s.push_str(&digits[n as usize..]);
        s
    } else {
        // 0.000...0digits, e.g., 0.0000001
        let mut s = String::with_capacity(2 + (-n) as usize + k as usize);
        s.push_str("0.");
        for _ in 0..(-n) {
            s.push('0');
        }
        s.push_str(&digits);
        s
    };

    // Try all possible exponential representations.
    // For split point j (1..=k), the representation is:
    //   - If j == k: "<digits>e<n-k>" (integer coefficient)
    //   - If j < k:  "<digits[..j]>.<digits[j..]>e<n-j>" (decimal coefficient)
    // The exponent part is formatted without '+' for positive exponents,
    // matching double-conversion's ToShortest behavior.
    let mut best = fixed;

    for j in 1..=k {
        let exp = n - j;
        if exp == 0 {
            // Would just be the fixed notation with no trailing zeros
            // (the coefficient has a decimal point already accounted for in fixed)
            // This is only the same as fixed when j == n, which we handled above.
            // The exponential form "XYZe0" is never shorter than "XYZ" or "X.YZ".
            continue;
        }

        let exp_str = exp.to_string();
        let candidate = if j == k {
            // Integer coefficient: "<digits>e<exp>"
            // Length: k + 1 (for 'e') + exp_str.len()
            let mut s = String::with_capacity(k as usize + 1 + exp_str.len());
            s.push_str(&digits);
            s.push('e');
            s.push_str(&exp_str);
            s
        } else {
            // Decimal coefficient: "<digits[..j]>.<digits[j..]>e<exp>"
            // Length: j + 1 (for '.') + (k-j) + 1 (for 'e') + exp_str.len()
            let mut s = String::with_capacity(k as usize + 2 + exp_str.len());
            s.push_str(&digits[..j as usize]);
            s.push('.');
            s.push_str(&digits[j as usize..]);
            s.push('e');
            s.push_str(&exp_str);
            s
        };

        if candidate.len() < best.len() {
            best = candidate;
        }
    }

    best
}

/// Returns (significant_digits_string, n) where:
/// - significant_digits_string contains the minimal significant digits with no
///   leading/trailing zeros (except for the digits themselves)
/// - n is the exponent such that the value = s * 10^(n-k), where k is the
///   length of the digit string and s is its integer value
///
/// For example:
/// - 123.456 -> ("123456", 3) because 123456 * 10^(3-6) = 123.456
/// - 0.001   -> ("1", -2) because 1 * 10^(-2-1) = 0.001
/// - 1e20    -> ("1", 21) because 1 * 10^(21-1) = 1e20
fn shortest_decimal_digits(v: f64) -> (String, i32) {
    let mut buffer = Buffer::new();
    let s = buffer.format_finite(v);

    if let Some(e_pos) = s.find('e') {
        // Scientific notation: e.g. "1.23e-15" or "1e20"
        let mantissa = &s[..e_pos];
        let exp: i32 = s[e_pos + 1..].parse().unwrap();

        if let Some(dot_pos) = mantissa.find('.') {
            // "1.23e-15" -> digits = "123", decimal places = 2
            let int_part = &mantissa[..dot_pos];
            let frac_part = &mantissa[dot_pos + 1..];
            let digits = format!("{}{}", int_part, frac_part);
            // n = exp + number of integer digits
            // For "1.23e-15": digits="123", exp=-15, n = -15 + 1 = -14
            // Check: 123 * 10^(-14-3) = 123 * 10^-17 = 1.23e-15 ✓
            let n = exp + (int_part.len() as i32);
            // Strip trailing zeros from digits
            let digits = digits.trim_end_matches('0');
            (digits.to_owned(), n)
        } else {
            // "1e20" -> digits = "1", exp = 20, n = 21
            // Check: 1 * 10^(21-1) = 1e20 ✓
            (mantissa.to_owned(), exp + 1)
        }
    } else if let Some(dot_pos) = s.find('.') {
        // Decimal notation: e.g. "123.456" or "0.001"
        let int_part = &s[..dot_pos];
        let frac_part = &s[dot_pos + 1..];

        if int_part == "0" {
            // "0.001" -> we need to find the first non-zero digit
            let leading_zeros = frac_part.len() - frac_part.trim_start_matches('0').len();
            let significant = frac_part.trim_start_matches('0');
            let significant = significant.trim_end_matches('0');
            // n = -(leading_zeros) since the value is 0.000...0(significant)
            // For "0.001": leading_zeros=2, significant="1", n = -2
            // Check: 1 * 10^(-2-1) = 0.001 ✓
            let n = -(leading_zeros as i32);
            (significant.to_owned(), n)
        } else {
            // "123.456" -> digits = "123456", n = 3 (number of integer digits)
            let digits = format!("{}{}", int_part, frac_part);
            let n = int_part.len() as i32;
            (digits.trim_end_matches('0').to_owned(), n)
        }
    } else {
        // Plain integer: e.g. "100" - but Rust normally doesn't produce this for f64
        // Rust f64 Display always has a decimal point or 'e' for finite non-zero values.
        // But handle it for safety.
        let trimmed = s.trim_end_matches('0');
        let n = s.len() as i32;
        if trimmed.is_empty() {
            ("0".to_owned(), 1)
        } else {
            (trimmed.to_owned(), n)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_integer_constants() {
        assert_eq!(MAX_SAFE_INTEGER, 9007199254740991.0);
        assert_eq!(MIN_SAFE_INTEGER, -9007199254740991.0);
    }

    #[test]
    fn test_in_safe_integer_range() {
        assert!(in_safe_integer_range(0.0));
        assert!(in_safe_integer_range(MAX_SAFE_INTEGER));
        assert!(in_safe_integer_range(MIN_SAFE_INTEGER));
        assert!(!in_safe_integer_range(MAX_SAFE_INTEGER + 1.0));
        assert!(!in_safe_integer_range(MIN_SAFE_INTEGER - 1.0));
    }

    #[test]
    fn test_is_float_safe_integer() {
        assert!(is_float_safe_integer(0.0));
        assert!(is_float_safe_integer(42.0));
        assert!(is_float_safe_integer(-42.0));
        assert!(is_float_safe_integer(MAX_SAFE_INTEGER));
        assert!(is_float_safe_integer(MIN_SAFE_INTEGER));

        // Not integers
        assert!(!is_float_safe_integer(3.14));
        assert!(!is_float_safe_integer(0.5));

        // Out of safe range
        assert!(!is_float_safe_integer(MAX_SAFE_INTEGER + 1.0));
        assert!(!is_float_safe_integer(MIN_SAFE_INTEGER - 1.0));

        // Special values
        assert!(!is_float_safe_integer(f64::NAN));
        assert!(!is_float_safe_integer(f64::INFINITY));
        assert!(!is_float_safe_integer(f64::NEG_INFINITY));
    }

    #[test]
    fn test_ecma_string_of_float_special_values() {
        assert_eq!(ecma_string_of_float(f64::NAN), "NaN");
        assert_eq!(ecma_string_of_float(f64::INFINITY), "Infinity");
        assert_eq!(ecma_string_of_float(f64::NEG_INFINITY), "-Infinity");
        assert_eq!(ecma_string_of_float(0.0), "0");
        assert_eq!(ecma_string_of_float(-0.0), "0");
    }

    #[test]
    fn test_ecma_string_of_float_integers() {
        assert_eq!(ecma_string_of_float(1.0), "1");
        assert_eq!(ecma_string_of_float(-1.0), "-1");
        assert_eq!(ecma_string_of_float(42.0), "42");
        assert_eq!(ecma_string_of_float(100.0), "100");
        assert_eq!(ecma_string_of_float(1000.0), "1000");
        assert_eq!(ecma_string_of_float(1000000.0), "1000000");
    }

    #[test]
    fn test_ecma_string_of_float_decimals() {
        assert_eq!(ecma_string_of_float(0.5), "0.5");
        assert_eq!(ecma_string_of_float(3.14), "3.14");
        assert_eq!(ecma_string_of_float(0.1), "0.1");
        assert_eq!(ecma_string_of_float(123.456), "123.456");
    }

    #[test]
    fn test_ecma_string_of_float_small_numbers() {
        // -6 < n <= 0 case
        assert_eq!(ecma_string_of_float(0.000001), "0.000001");
        // n <= -6 case -> scientific notation
        assert_eq!(ecma_string_of_float(0.0000001), "1e-7");
        assert_eq!(ecma_string_of_float(1e-22), "1e-22");
        assert_eq!(ecma_string_of_float(5e-324), "5e-324");
    }

    #[test]
    fn test_ecma_string_of_float_large_numbers() {
        // k <= n <= 21 case
        assert_eq!(ecma_string_of_float(1e20), "100000000000000000000");
        // n > 21 case -> scientific notation
        assert_eq!(ecma_string_of_float(1e21), "1e+21");
        assert_eq!(ecma_string_of_float(1e22), "1e+22");
    }

    #[test]
    fn test_ecma_string_of_float_negative() {
        assert_eq!(ecma_string_of_float(-42.0), "-42");
        assert_eq!(ecma_string_of_float(-0.5), "-0.5");
        assert_eq!(ecma_string_of_float(-1e-22), "-1e-22");
        assert_eq!(ecma_string_of_float(-1e21), "-1e+21");
    }

    #[test]
    fn test_ecma_string_of_float_scientific_multi_digit() {
        assert_eq!(ecma_string_of_float(1.5e21), "1.5e+21");
        assert_eq!(ecma_string_of_float(1.23e-15), "1.23e-15");
        assert_eq!(
            ecma_string_of_float(1.7976931348623157e308),
            "1.7976931348623157e+308"
        );
    }

    #[test]
    fn test_ecma_string_of_float_safe_integers() {
        // These are commonly used with is_float_safe_integer
        assert_eq!(ecma_string_of_float(0.0), "0");
        assert_eq!(ecma_string_of_float(1.0), "1");
        assert_eq!(ecma_string_of_float(MAX_SAFE_INTEGER), "9007199254740991");
    }

    #[test]
    fn test_shortest_string_of_float_special_values() {
        assert_eq!(shortest_string_of_float(f64::NAN), "NaN");
        assert_eq!(shortest_string_of_float(f64::INFINITY), "Infinity");
        assert_eq!(shortest_string_of_float(f64::NEG_INFINITY), "-Infinity");
        assert_eq!(shortest_string_of_float(0.0), "0");
        assert_eq!(shortest_string_of_float(-0.0), "0");
    }

    #[test]
    fn test_shortest_string_of_float_basic() {
        // Simple values that don't benefit from exponential notation
        assert_eq!(shortest_string_of_float(1.0), "1");
        assert_eq!(shortest_string_of_float(42.0), "42");
        assert_eq!(shortest_string_of_float(100.0), "100");
        assert_eq!(shortest_string_of_float(0.5), "0.5");
        assert_eq!(shortest_string_of_float(1.1), "1.1");
    }

    #[test]
    fn test_shortest_string_of_float_exponential() {
        // Values where exponential notation is shorter
        // These are the key cases from the Flow tests
        assert_eq!(shortest_string_of_float(0.0000001), "1e-7");
        assert_eq!(shortest_string_of_float(1000.0), "1e3");
        assert_eq!(shortest_string_of_float(2592000000.0), "2592e6");
    }

    #[test]
    fn test_shortest_string_of_float_negative() {
        assert_eq!(shortest_string_of_float(-42.0), "-42");
        assert_eq!(shortest_string_of_float(-0.0000001), "-1e-7");
        assert_eq!(shortest_string_of_float(-1000.0), "-1e3");
    }

    #[test]
    fn test_ecma_string_of_float_decimal_ties() {
        // Exact quarter steps at this magnitude lie between two shortest decimals.
        for (bits, expected) in [
            (0x430c_6bf5_2634_0002, "1000000000000000.2"),
            (0x430c_6bf5_2634_0006, "1000000000000000.8"),
            (0xc30c_6bf5_2634_0002, "-1000000000000000.2"),
            (0xc30c_6bf5_2634_0006, "-1000000000000000.8"),
        ] {
            assert_eq!(ecma_string_of_float(f64::from_bits(bits)), expected);
        }
    }

    #[test]
    fn test_ecma_string_of_float_notation_boundaries() {
        // JavaScript switches notation at 1e-6 and 1e21, including their neighbors.
        for (bits, expected) in [
            (0x3eb0_c6f7_a0b5_ed8c, "9.999999999999997e-7"),
            (0x3eb0_c6f7_a0b5_ed8d, "0.000001"),
            (0x3eb0_c6f7_a0b5_ed8e, "0.0000010000000000000002"),
            (0x444b_1ae4_d6e2_ef4f, "999999999999999900000"),
            (0x444b_1ae4_d6e2_ef50, "1e+21"),
            (0x444b_1ae4_d6e2_ef51, "1.0000000000000001e+21"),
        ] {
            let value = f64::from_bits(bits);
            assert_eq!(ecma_string_of_float(value), expected);
            assert_eq!(ecma_string_of_float(-value), format!("-{expected}"));
        }
    }

    #[test]
    fn test_ecma_string_of_float_finite_extremes() {
        // The largest subnormal and smallest normal retain distinct spellings.
        for (bits, expected) in [
            (0x0000_0000_0000_0001, "5e-324"),
            (0x000f_ffff_ffff_ffff, "2.225073858507201e-308"),
            (0x0010_0000_0000_0000, "2.2250738585072014e-308"),
            (0x7fef_ffff_ffff_ffff, "1.7976931348623157e+308"),
        ] {
            let value = f64::from_bits(bits);
            assert_eq!(ecma_string_of_float(value), expected);
            assert_eq!(ecma_string_of_float(-value), format!("-{expected}"));
        }
    }

    #[test]
    fn test_shortest_decimal_digits_remove_trailing_zeros() {
        // Ryū's trailing .0 must not become significant digits in compact output.
        for (value, expected) in [
            (1000.0, ("1", 4)),
            (1_000_000.0, ("1", 7)),
            (2_592_000_000.0, ("2592", 10)),
            (120.3, ("1203", 3)),
            (0.0012, ("12", -2)),
        ] {
            let (digits, n) = shortest_decimal_digits(value);
            assert_eq!((digits.as_str(), n), expected);
        }
    }
}
