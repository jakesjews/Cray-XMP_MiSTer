//! Floating-point multiply unit (instructions 064 to 067 and 160 to 167).
//!
//! Sources: HR-0097B (CRAY X-MP four-processor mainframe reference manual) pages 4-24 to
//! 4-30 for the exponent matrix (figure 4-7), the truncated pyramid (figure 4-10), the
//! truncation compensation constant and the round bits; CRAY-1 HRM 2240004 rev C pages 3-22
//! and 4-40 for the range rules shared by both machines. The 067 complement step follows
//! cray-sim (`cray_float.cpp`); no manual describes it at bit level.
//!
//! Bit weights: the coefficient is `a = sum a_p 2^-p` for p = 1 (bit 47) to 48 (bit 0). The
//! logical product `a_p b_q` has weight `2^-(p+q)` and lives in pyramid "column" p+q, so
//! columns run from 2 to 96 and the pyramid output from `2^-1` to `2^-96`.

use crate::{pack, unpack, FpResult, MulKind, Profile, COEF_MASK, EXP_BIAS, EXP_MIN, EXP_OVERFLOW};

/// Parameters of a truncated multiply pyramid with a symmetric (commutative) cut.
///
/// All logical products in columns `2..=last_column` are summed exactly; nothing to the
/// right of `last_column` is formed. Constants are then added in the same adder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MulModel {
    /// Lowest-weight column (`2^-last_column`) whose logical products are formed.
    pub last_column: u32,
    /// Truncation compensation constant, in units of `2^-56`, added to every multiply.
    pub compensation: u64,
    /// Bit positions `n` (weight `2^-n`) of the round bits for the rounded multiply (066).
    pub round_bits: [u32; 2],
    /// Bit positions of the round bits for the half-precision multiply (065).
    pub half_round_bits: [u32; 2],
    /// Number of result coefficient bits kept by the half-precision multiply.
    pub half_bits: u32,
}

impl MulModel {
    /// The X-MP multiplier as the Cray manuals describe it (HR-0097B figure 4-10 and page
    /// 4-30): pyramid cut after column `2^-56`, nine carries injected at `2^-56`, round bits
    /// at `2^-50` and `2^-51`, half-precision round bits at `2^-31` and `2^-32`, 29-bit
    /// half-precision result. This is what [`Profile::Xmp`] uses.
    pub const XMP_MANUAL: MulModel = MulModel {
        last_column: 56,
        compensation: 9,
        round_bits: [50, 51],
        half_round_bits: [31, 32],
        half_bits: 29,
    };

    /// The multiplier core as cray-sim's `Mult` function computes it: one extra pyramid
    /// column (`2^-57`) and half-precision round bits one place lower (`2^-32`, `2^-33`).
    /// Provided for comparison with that simulator only. It reproduces the same 24 product
    /// vectors as [`MulModel::XMP_MANUAL`] but disagrees with the manual's own statistics
    /// (see the crate documentation).
    pub const CRAY_SIM: MulModel = MulModel {
        last_column: 57,
        compensation: 9,
        round_bits: [50, 51],
        half_round_bits: [32, 33],
        half_bits: 29,
    };

    /// Sum of the logical products the pyramid forms, in units of `2^-last_column`.
    pub fn partial_product_sum(&self, a_coef: u64, b_coef: u64) -> u64 {
        let guard = self.last_column - 49;
        let wide = (a_coef & COEF_MASK) << guard;
        let mut sum = 0u64;
        for q in 1..=48u32 {
            if (b_coef >> (48 - q)) & 1 != 0 {
                sum += wide >> (q - 1);
            }
        }
        sum
    }

    fn unit(&self, position: u32) -> u64 {
        1u64 << (self.last_column - position)
    }

    /// Pyramid output including the injected constants, in units of `2^-last_column`.
    /// For [`MulKind::TwoMinus`] this is the complemented output.
    pub fn pyramid_output(&self, a_coef: u64, b_coef: u64, kind: MulKind) -> u64 {
        let sum = self.partial_product_sum(a_coef, b_coef);
        let comp = self.compensation << (self.last_column - 56);
        match kind {
            MulKind::Full => sum + comp,
            MulKind::Rounded => {
                sum + comp + self.unit(self.round_bits[0]) + self.unit(self.round_bits[1])
            }
            MulKind::HalfRounded => {
                sum + comp + self.unit(self.half_round_bits[0]) + self.unit(self.half_round_bits[1])
            }
            // cray-sim: add the complemented constant, then complement the sum. Net effect:
            // the two's complement of (sum - constant).
            MulKind::TwoMinus => comp.wrapping_sub(sum),
        }
    }
}

/// Floating product for instructions 064 (`Full`), 065 (`HalfRounded`), 066 (`Rounded`) and
/// 067 (`TwoMinus`, the reciprocal iteration `2 - a*b`).
///
/// [`Profile::Cray1`] currently returns exactly what [`Profile::Xmp`] returns: the CRAY-1
/// pyramid of figure 3-5 could not be reconstructed uniquely (see [`crate::cray1_pyramid`]).
pub fn fmul(a: u64, b: u64, kind: MulKind, profile: Profile) -> FpResult {
    match profile {
        Profile::Xmp | Profile::Cray1 => fmul_model(a, b, kind, &MulModel::XMP_MANUAL),
    }
}

/// [`fmul`] with an explicit pyramid model.
///
/// Exponent handling (HR-0097B figure 4-7, CRAY-1 HRM page 3-22), with `E = ea + eb - 040000`
/// the exponent of an unshifted product:
///
/// * both exponents zero: integer multiply. The result is the upper 48 bits of the pyramid
///   output with no normalisation shift, exponent zero, sign the exclusive OR of the signs.
/// * exactly one exponent zero: underflow, all-zero result, no error, whatever the other
///   exponent is.
/// * either exponent `>= 060000`, or `E >= 060000`: `range_error`, result exponent `060000`
///   with the computed coefficient.
/// * `E < 020000`: underflow, all-zero result, no error.
/// * otherwise the result exponent is `E` when the pyramid output has its `2^-1` bit set and
///   `E - 1` after a one-place left shift when it has not. The tests above are made on `E`
///   before that shift, so a result exponent of `017777` can come out without being zeroed.
///
/// For [`MulKind::TwoMinus`] the pyramid output is complemented before the shift decision,
/// so the unit really forms `2^(E - 040000) - a*b`. That is `2 - a*b` only when `E` is
/// `040001`, as it is for a number and its reciprocal approximation (HR-0097B page 4-34
/// warns that iterating on an exact reciprocal gives a wrong result).
pub fn fmul_model(a: u64, b: u64, kind: MulKind, model: &MulModel) -> FpResult {
    let x = unpack(a);
    let y = unpack(b);
    let negative = x.sign != y.sign;
    let integer = x.exp == 0 && y.exp == 0;
    if !integer && (x.exp == 0 || y.exp == 0) {
        return FpResult {
            value: 0,
            range_error: false,
        };
    }

    let out = model.pyramid_output(x.coef, y.coef, kind);
    let top = model.last_column - 1;
    let no_shift = integer || (out >> top) & 1 != 0;
    let mut coef = if no_shift {
        out >> (top - 47)
    } else {
        out >> (top - 48)
    } & COEF_MASK;
    if kind == MulKind::HalfRounded {
        coef &= !((1u64 << (48 - model.half_bits)) - 1);
    }
    if integer {
        return FpResult {
            value: pack(negative, 0, coef),
            range_error: false,
        };
    }

    let e = i32::from(x.exp) + i32::from(y.exp) - i32::from(EXP_BIAS);
    if x.exp >= EXP_OVERFLOW || y.exp >= EXP_OVERFLOW || e >= i32::from(EXP_OVERFLOW) {
        return FpResult {
            value: pack(negative, EXP_OVERFLOW, coef),
            range_error: true,
        };
    }
    if e < i32::from(EXP_MIN) {
        return FpResult {
            value: 0,
            range_error: false,
        };
    }
    let exp = if no_shift { e } else { e - 1 };
    FpResult {
        value: pack(negative, exp as u16, coef),
        range_error: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_times_one() {
        let one = 0x4001_8000_0000_0000;
        for kind in [MulKind::Full, MulKind::HalfRounded, MulKind::Rounded] {
            assert_eq!(fmul(one, one, kind, Profile::Xmp).value, one, "{kind:?}");
        }
    }

    #[test]
    fn constants_in_register_units() {
        let m = MulModel::XMP_MANUAL;
        assert_eq!(m.pyramid_output(0, 0, MulKind::Full), 9);
        assert_eq!(m.pyramid_output(0, 0, MulKind::Rounded), 9 + 0x60);
        assert_eq!(m.pyramid_output(0, 0, MulKind::HalfRounded), 9 + (3 << 24));
        // cray-sim's register has one more bit on the right: 18, 0xC0, bits 24 and 25.
        let c = MulModel::CRAY_SIM;
        assert_eq!(c.pyramid_output(0, 0, MulKind::Full), 18);
        assert_eq!(c.pyramid_output(0, 0, MulKind::Rounded), 18 + 0xC0);
        assert_eq!(c.pyramid_output(0, 0, MulKind::HalfRounded), 18 + (3 << 24));
    }

    #[test]
    fn pyramid_drops_columns_right_of_the_cut() {
        let m = MulModel::XMP_MANUAL;
        // a_48 * b_8 is in column 56 (kept); a_48 * b_9 is in column 57 (dropped).
        assert_eq!(m.partial_product_sum(1, 1 << 40), 1);
        assert_eq!(m.partial_product_sum(1, 1 << 39), 0);
        assert_eq!(MulModel::CRAY_SIM.partial_product_sum(1, 1 << 39), 1);
    }
}
