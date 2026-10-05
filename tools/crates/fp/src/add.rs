//! Floating-point add unit (instructions 062, 063 and 170 to 173).
//!
//! Sources: CRAY-1 HRM 2240004 rev C pages 3-22 and 3-23 (range error, 49-bit register,
//! "bits shifted out of the register are lost; no round-up takes place") and HR-0097B
//! pages 4-24 and 4-28 (carry-out handling, underflow to an all-zero word, range error on an
//! out-of-range result).

use crate::{pack, unpack, FpResult, EXP_MIN, EXP_OVERFLOW, SIGN_BIT};

/// Floating sum `a + b` as the add unit forms it.
///
/// 1. The operand with the smaller exponent is shifted right by the exponent difference.
///    The shifted-out bits are dropped from the *magnitude* (no guard bits, no rounding).
/// 2. Magnitudes are added or subtracted in a 49-bit register. The result takes the sign of
///    the larger magnitude after alignment.
/// 3. A carry into the 49th bit shifts the sum right one place (dropping one more bit) and
///    adds one to the exponent. Otherwise the sum is shifted left until bit 47 is set and the
///    exponent is reduced by the shift count.
/// 4. A zero coefficient or a result exponent below `020000` gives an all-zero word with no
///    error. An operand exponent of `060000` or above, or a result exponent that reaches
///    `060000`, sets `range_error` and forces the result exponent to `060000`; the computed
///    coefficient and sign are kept.
///
/// See the crate documentation for which of these points are confirmed by reference
/// vectors and which are taken from the manuals only.
pub fn fadd(a: u64, b: u64) -> FpResult {
    let x = unpack(a);
    let y = unpack(b);
    // Trial subtraction of the exponents selects the operand to shift down.
    let (big, small) = if x.exp >= y.exp { (x, y) } else { (y, x) };
    let diff = u32::from(big.exp - small.exp);
    let aligned = if diff >= 48 { 0 } else { small.coef >> diff };

    let (negative, mut mag) = if big.sign == small.sign {
        (big.sign, big.coef + aligned)
    } else if big.coef >= aligned {
        (big.sign, big.coef - aligned)
    } else {
        (small.sign, aligned - big.coef)
    };

    let operand_overflow = big.exp >= EXP_OVERFLOW;
    if mag == 0 {
        // No functional unit generates a negative zero. With an out-of-range operand the
        // exponent is still forced to 060000 (manual wording; not confirmed by a vector).
        return if operand_overflow {
            FpResult {
                value: pack(false, EXP_OVERFLOW, 0),
                range_error: true,
            }
        } else {
            FpResult {
                value: 0,
                range_error: false,
            }
        };
    }

    let mut exp = i32::from(big.exp);
    if mag >> 48 != 0 {
        mag >>= 1;
        exp += 1;
    } else {
        let shift = mag.leading_zeros() - 16;
        mag <<= shift;
        exp -= shift as i32;
    }

    if operand_overflow || exp >= i32::from(EXP_OVERFLOW) {
        return FpResult {
            value: pack(negative, EXP_OVERFLOW, mag),
            range_error: true,
        };
    }
    if exp < i32::from(EXP_MIN) {
        return FpResult {
            value: 0,
            range_error: false,
        };
    }
    FpResult {
        value: pack(negative, exp as u16, mag),
        range_error: false,
    }
}

/// Floating difference `a - b`: the add unit with the sign of `b` inverted.
pub fn fsub(a: u64, b: u64) -> FpResult {
    fadd(a, b ^ SIGN_BIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magnitude_truncation_vector() {
        // cray-sim fp_test.cpp table entry 13: only magnitude truncation gives ...04.
        let r = fadd(0xBFFE_FFFF_FFFF_FFFF, 0x4000_8000_0000_0001);
        assert_eq!(r.value, 0x3FFF_8000_0000_0004);
        assert!(!r.range_error);
    }

    #[test]
    fn carry_drops_low_bit() {
        let r = fadd(0x4001_FFFF_FFFF_FFFF, 0x4001_8000_0000_0000);
        assert_eq!(r.value, 0x4002_BFFF_FFFF_FFFF);
    }

    #[test]
    fn subtract_is_add_of_negated() {
        // 1.5 - 1.0 = 0.5
        assert_eq!(
            fsub(0x4001_C000_0000_0000, 0x4001_8000_0000_0000).value,
            0x4000_8000_0000_0000
        );
        assert_eq!(fsub(0x4001_8000_0000_0000, 0x4001_8000_0000_0000).value, 0);
    }
}
