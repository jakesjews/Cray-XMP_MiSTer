//! Conversion between Cray words and `f64`, for tests and diagnostics only. Nothing in the
//! arithmetic modules calls these.

use crate::{pack, unpack, EXP_BIAS, NORM_BIT};

fn scale(mut m: f64, mut k: i32) -> f64 {
    // Multiply by 2^k in steps that are exact powers of two, so no step overflows early.
    while k > 1000 {
        m *= f64::powi(2.0, 1000);
        k -= 1000;
    }
    while k < -1000 {
        m *= f64::powi(2.0, -1000);
        k += 1000;
    }
    m * f64::powi(2.0, k)
}

/// Value of a Cray floating-point word as an `f64`.
///
/// The word is read literally: sign, biased exponent and 48-bit coefficient, normalised or
/// not, whatever the exponent range. Values beyond the `f64` range become infinities or
/// zero, and the 48-bit coefficient always fits the 53-bit `f64` significand.
pub fn to_f64(x: u64) -> f64 {
    let u = unpack(x);
    let mag = scale(u.coef as f64, i32::from(u.exp) - i32::from(EXP_BIAS) - 48);
    if u.sign {
        -mag
    } else {
        mag
    }
}

/// Nearest normalised Cray word to `v` (round to nearest, ties to even on the 48-bit
/// coefficient). Returns `None` for NaN and infinities. Both zeros map to the all-zero word.
pub fn from_f64(v: f64) -> Option<u64> {
    if !v.is_finite() {
        return None;
    }
    if v == 0.0 {
        return Some(0);
    }
    let bits = v.to_bits();
    let negative = bits >> 63 != 0;
    let field = ((bits >> 52) & 0x7FF) as i32;
    let fraction = bits & ((1u64 << 52) - 1);
    // v = m * 2^e2 with m a 53-bit integer whose top bit is set.
    let (mut m, mut e2) = if field == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1 << 52), field - 1075)
    };
    let shift = m.leading_zeros() as i32 - 11;
    m <<= shift;
    e2 -= shift;
    // Round the 53-bit significand to 48 bits.
    let mut coef = m >> 5;
    let rest = m & 0x1F;
    if rest > 0x10 || (rest == 0x10 && coef & 1 != 0) {
        coef += 1;
    }
    let mut exp = e2 + 53 + i32::from(EXP_BIAS);
    if coef >> 48 != 0 {
        coef >>= 1;
        exp += 1;
    }
    debug_assert!(coef & NORM_BIT != 0);
    Some(pack(negative, exp as u16, coef))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        assert_eq!(from_f64(1.0), Some(0x4001_8000_0000_0000));
        assert_eq!(from_f64(-0.5), Some(0xC000_8000_0000_0000));
        assert_eq!(from_f64(1024.0), Some(0x400B_8000_0000_0000));
        assert_eq!(from_f64(0.0), Some(0));
        assert_eq!(from_f64(f64::NAN), None);
        assert_eq!(to_f64(0x4001_8000_0000_0000), 1.0);
        assert_eq!(to_f64(0xC003_F000_0000_0000), -7.5);
        assert_eq!(to_f64(0), 0.0);
    }

    #[test]
    fn round_trip_is_close() {
        for &v in &[
            3.25,
            -1.0e-30,
            6.02214076e23,
            1.0 / 3.0,
            f64::MIN_POSITIVE,
            5e-324,
            1.0e300,
        ] {
            let back = to_f64(from_f64(v).unwrap());
            assert!(((back - v) / v).abs() < 1.0e-14, "{v} -> {back}");
        }
    }

    #[test]
    fn unnormalised_and_out_of_range_words() {
        // 040060 with an integer coefficient is the integer itself.
        assert_eq!(to_f64(0x4030_0000_0000_002A), 42.0);
        assert_eq!(to_f64(0x5FFF_8000_0000_0000), f64::INFINITY);
        assert_eq!(to_f64(0xA000_8000_0000_0000), -0.0);
    }
}
