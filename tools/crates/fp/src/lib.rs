//! Bit-exact Cray floating-point arithmetic: the add, multiply and reciprocal approximation
//! units of the CRAY-1 and CRAY X-MP, working on 64-bit operand words.
//!
//! This crate is the reference model that hardware floating-point units are checked
//! against. Read the section "What is and is not validated" before relying on any corner.
//!
//! # API
//!
//! | Item | Instruction | Notes |
//! |------|-------------|-------|
//! | [`fadd`]`(a, b)` | 062, 170, 171 | |
//! | [`fsub`]`(a, b)` | 063, 172, 173 | `fadd(a, -b)` |
//! | [`fmul`]`(a, b, kind, profile)` | 064 to 067, 160 to 167 | [`MulKind`], [`Profile`] |
//! | [`frecip`]`(a, profile)` | 070, 174 | |
//! | [`fdiv`]`(a, b, profile)` | none | the manual's four-instruction divide sequence |
//! | [`fmul_model`], [`MulModel`] | | the multiply with an explicit pyramid model |
//! | [`pack`], [`unpack`], [`is_normalized`] | | word format helpers |
//! | [`to_f64`], [`from_f64`] | | test convenience, never used by the arithmetic |
//! | [`vectors()`], [`vectors_for`], [`write_vector_file`], [`read_vector_file`] | | test vectors, see [`mod@vectors`] |
//! | [`cray1_pyramid`] | | experimental reconstruction of the CRAY-1 pyramid, not used by `fmul` |
//!
//! Every operation returns an [`FpResult`]: the result word and the range error flag. That
//! flag is the only one the manuals define. Underflow is silent and gives an all-zero word.
//!
//! The functions take operand *values*. Register-designator special cases (Sj = 0 when
//! j = 0, Sk = 2^63 when k = 0) belong to the CPU model.
//!
//! # Format
//!
//! Bit 63 is the coefficient sign, bits 62..48 the exponent biased by `040000` (octal), bits
//! 47..0 the coefficient magnitude with the binary point to the left of bit 47. A number is
//! normalised when bit 47 is set. Exponents `020000..=057777` are in range; below is
//! underflow, `060000` and above is overflow.
//!
//! # Profiles
//!
//! [`Profile::Xmp`] is the X-MP arithmetic of the Cray manuals (HR-0097B section 4).
//! [`Profile::Cray1`] returns **exactly the same results**, and stands for the CRAY-1 with
//! the symmetric multiply unit:
//!
//! * The CRAY-1 changed its multiply unit. Revisions C (1977) and E (1979) of the hardware
//!   manual describe a non-commutative staircase pyramid with its truncation constant at
//!   `2^-51` and `2^-52`, one round bit at `2^-49` for 066 and a 30-bit half-precision
//!   result. Change packet E-01 of May 1980, printed in revision F (1982), "documents
//!   changes to the multiply functional unit that supports symmetrical multiply": a straight
//!   cut after `2^-56`, nine carries at `2^-56`, round bits at `2^-50` and `2^-51`,
//!   commutative. That text, the CRAY-1 S manual's and the X-MP manuals' are the same.
//! * The original staircase unit is not modelled. [`cray1_pyramid`] has it as far as figure
//!   3-5 of revision C allows; about one unrounded product in five differs from the
//!   symmetric unit in its last bit (`tests/pyramid.rs`). Which serial numbers had which
//!   unit is not known.
//! * E-01 still calls the half-precision result "30-bit"; the CRAY-1 S and X-MP manuals and
//!   Cray's diagnostic say 29 bits, which is what both profiles return.
//! * The reciprocal unit is the same in both: Cray's diagnostic simulation of it names
//!   CRAY-1 modules.
//!
//! [`fadd`] and [`fsub`] take no profile. The one point where the two manuals differ for
//! the add unit is noted below.
//!
//! # What is and is not validated
//!
//! Reference vectors: the sum, product and reciprocal tables of cray-sim's `fp_test.cpp`
//! (72 cases) and seven further cases from its `main()`, stored in
//! `tests/fp/xmp_ref.vec`. All are reproduced. The 72 are the canned operands and answers
//! of Cray's floating point diagnostic JFPT (tables COPA, COPB, ERFA, ERFM and ERRP in the
//! CRAY J90 offline diagnostic listing of January 1997; the code dates from 1980). They cover in-range, mostly normalised
//! operands of 062, 064 and 070 only. Three more cases in `main()` are left out because
//! cray-sim itself does not reproduce them; one of them, a product, is matched by the
//! reconstructed CRAY-1 staircase and by the X-MP rounded multiply but not by the X-MP
//! unrounded multiply (`tests/pyramid.rs`).
//!
//! Confirmed by those vectors:
//!
//! * add: alignment truncates the magnitude with no guard bit (one vector fails both with
//!   two's-complement truncation and with exact alignment); carry and normalise.
//! * multiply 064: a truncated pyramid plus the constant `9 x 2^-56`, and the integer
//!   multiply for zero exponents. The vectors do **not** tell a pyramid cut after `2^-56`
//!   (the manuals) from one cut after `2^-57` (cray-sim): both reproduce all of them.
//!   `Profile::Xmp` uses `2^-56`, because the manual's own numbers single it out: the mean
//!   truncated carry of 9.25, results from one too small to one too large, and 99 percent
//!   exact, all measured in this crate's tests. cray-sim's variant gives 4.25, never too
//!   small, and 97.5 percent.
//! * reciprocal 070: the complete algorithm for normalised operands, and the result for a
//!   zero operand.
//!
//! Confirmed by Cray's own simulation of the units in that diagnostic (subroutines SMLT and
//! SRP, transcribed and compared outside this crate on millions of operands; see
//! `research/notes/fp-multiply.md` in the core's repository):
//!
//! * 064, 065, 066 and 067 bit for bit, value and range flag, including the pyramid cut
//!   after `2^-56`, the constant 9, the round bits, the 29-bit half-precision result and the
//!   complement step of 067 (`mul::TwoMinus::Cray`);
//! * the multiply's range and underflow rules, the integer multiply, and its sign: the
//!   exclusive OR of the operand signs even when the coefficient comes out zero;
//! * the reciprocal, bit for bit.
//!
//! Taken from the manuals without any reference:
//!
//! * the range error and underflow rules of the add unit, and its flag values;
//! * add: operands with an exponent below `020000` take part normally (cray-sim replaces
//!   them by zero first); a result exponent below `020000` gives zero (cray-sim stops
//!   normalising at `020000`); the range error on a carry out of exponent `057777` is in the
//!   X-MP manual only; a zero coefficient with an operand exponent of `060000` or more
//!   returns exponent `060000` and the flag;
//! * multiply: an operand exponent `1..=017777` is accepted if the result is in range
//!   (cray-sim returns zero); a half-precision coefficient that rounds up past all ones
//!   wraps to zero without an exponent adjustment; the half-precision mask also applies to
//!   the integer multiply;
//! * reciprocal: the range-error result (exponent `060000`, bit 47 cleared) for exponents
//!   other than zero.
//!
//! # Measured accuracy
//!
//! From `tests/sanity.rs`, one million random normalised operands each, against exact
//! integer arithmetic:
//!
//! * add and multiply stay inside the truncation bounds that follow from the manuals'
//!   descriptions (stated in the test comments);
//! * `x * frecip(x)` differs from 1 by less than `2^-30` (largest seen `2^-30.43`): the
//!   "30 bits" of HRM page 3-28, not the "27 bits" of page 4-42;
//! * the four-instruction divide sequence ([`fdiv`]) is **not** always within one unit of
//!   the last coefficient bit. The quotient ranges from 2.28 units too small to 1.56 too
//!   large, is 0.45 units too small on average, and is within one unit four times out of
//!   five. This follows from the arithmetic itself: the correction factor `2 - r*b` has a
//!   coefficient just above one half, so its last bit is worth up to two units of the
//!   quotient (the manual's "47 bits"), and two truncating multiplies follow. W. Kahan's
//!   figures from real machines ("How Cray's arithmetic hurts scientific computation",
//!   1990) fit this rule and not cray-sim's, which would be 3.7 units low to 0.1 high.
//!
//! # Differences from cray-sim
//!
//! Checked once, outside this crate's test suite, by running 500,000 generated vectors per
//! operation through a locally compiled copy of cray-sim's `cray_float.cpp`:
//!
//! * `frecip` gave the same bits for every input;
//! * `fadd` and `fsub` gave the same bits whenever both operand exponents were in
//!   `020000..=057777` and the result was non-zero and in range (about 400,000 cases each);
//! * [`fmul_model`] with [`MulModel::CRAY_SIM`] gave the same bits for all four multiply
//!   kinds whenever both operand exponents and the exponent sum were in range (about 297,000
//!   cases each).
//!
//! Everywhere else the two differ on purpose, as listed above: cray-sim implements no range
//! errors, zeroes operands with small exponents, and has the wider pyramid. Between
//! [`MulModel::XMP_MANUAL`] (what `fmul` uses) and [`MulModel::CRAY_SIM`], random normalised
//! products differ in about 2.6 percent of cases for 064 and 066, 26 percent for 065 and 90
//! percent for 067, where cray-sim's complement step is one or two units lower
//! (`tests/xmp_statistics.rs`).

#![forbid(unsafe_code)]

mod add;
mod convert;
pub mod cray1_pyramid;
mod mul;
mod recip;
pub mod vectors;

pub use add::{fadd, fsub};
pub use convert::{from_f64, to_f64};
pub use mul::{fmul, fmul_model, MulModel};
pub use recip::{frecip, recip_seed, recip_table_word};
pub use vectors::{read_vector_file, vectors, vectors_for, write_vector_file, Op, Vector};

/// Sign bit of the coefficient (bit 63).
pub const SIGN_BIT: u64 = 1 << 63;
/// Mask of the 48-bit coefficient.
pub const COEF_MASK: u64 = (1 << 48) - 1;
/// Coefficient bit 47; set in every normalised number.
pub const NORM_BIT: u64 = 1 << 47;
/// Exponent bias, `040000` octal.
pub const EXP_BIAS: u16 = 0o40000;
/// Smallest in-range biased exponent, `020000` octal.
pub const EXP_MIN: u16 = 0o20000;
/// Largest in-range biased exponent, `057777` octal.
pub const EXP_MAX: u16 = 0o57777;
/// Smallest overflow exponent, `060000` octal; also the exponent forced on a range error.
pub const EXP_OVERFLOW: u16 = 0o60000;

/// Result of a floating-point functional unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct FpResult {
    /// The 64-bit word sent to the result register.
    pub value: u64,
    /// The floating-point error (range error) condition. The CPU turns it into an interrupt
    /// when the floating-point mode flag is set.
    pub range_error: bool,
}

/// Which product the multiply unit forms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MulKind {
    /// 064: floating product.
    Full,
    /// 065: half-precision rounded floating product.
    HalfRounded,
    /// 066: rounded floating product.
    Rounded,
    /// 067: reciprocal iteration, `2 - a*b`.
    TwoMinus,
}

/// Which machine's arithmetic to model. See the crate documentation: the two profiles
/// currently give identical results.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Profile {
    /// CRAY X-MP, per HR-0097B.
    Xmp,
    /// CRAY-1. Falls back to the X-MP arithmetic where the CRAY-1 behaviour is not known
    /// at bit level, which today is everywhere.
    Cray1,
}

/// The three fields of a floating-point word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Unpacked {
    /// True for a negative coefficient.
    pub sign: bool,
    /// Biased exponent, 15 bits.
    pub exp: u16,
    /// Coefficient magnitude, 48 bits.
    pub coef: u64,
}

/// Split a word into sign, biased exponent and coefficient.
pub fn unpack(word: u64) -> Unpacked {
    Unpacked {
        sign: word >> 63 != 0,
        exp: ((word >> 48) & 0x7FFF) as u16,
        coef: word & COEF_MASK,
    }
}

/// Assemble a word. `exp` is masked to 15 bits and `coef` to 48 bits.
pub fn pack(sign: bool, exp: u16, coef: u64) -> u64 {
    (u64::from(sign) << 63) | (u64::from(exp & 0x7FFF) << 48) | (coef & COEF_MASK)
}

/// True when the word is normalised: coefficient bit 47 is set, or the word is all zeros.
pub fn is_normalized(word: u64) -> bool {
    word == 0 || word & NORM_BIT != 0
}

/// The divide sequence of CRAY-1 HRM page 3-28, `a / b` in four instructions:
///
/// 1. `r = frecip(b)` (070)
/// 2. `c = 2 - r*b` (067)
/// 3. `q = a*r` (064)
/// 4. `c*q` (064)
///
/// `range_error` is the OR of the four steps. This is a convenience for tests, not an
/// instruction. The quotient is not correctly rounded: see "Measured accuracy" in the crate
/// documentation.
pub fn fdiv(a: u64, b: u64, profile: Profile) -> FpResult {
    let r = frecip(b, profile);
    let c = fmul(r.value, b, MulKind::TwoMinus, profile);
    let q = fmul(a, r.value, MulKind::Full, profile);
    let out = fmul(c.value, q.value, MulKind::Full, profile);
    FpResult {
        value: out.value,
        range_error: r.range_error || c.range_error || q.range_error || out.range_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_round_trip() {
        for word in [
            0u64,
            SIGN_BIT,
            0x4001_8000_0000_0000,
            0xFFFF_FFFF_FFFF_FFFF,
            0x2000_0000_0000_0001,
        ] {
            let u = unpack(word);
            assert_eq!(pack(u.sign, u.exp, u.coef), word);
        }
        assert_eq!(
            unpack(0xC003_F000_0000_0000),
            Unpacked {
                sign: true,
                exp: 0o40003,
                coef: 0xF000_0000_0000
            }
        );
        assert_eq!(pack(false, 0xFFFF, u64::MAX), 0x7FFF_FFFF_FFFF_FFFF);
    }

    #[test]
    fn normalised_test() {
        assert!(is_normalized(0));
        assert!(is_normalized(0x4001_8000_0000_0000));
        assert!(!is_normalized(0x4030_0000_0000_002A));
        assert!(!is_normalized(SIGN_BIT));
    }
}
