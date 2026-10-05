//! Mathematical sanity checks that depend on neither reference: results are compared with
//! exact integer or rational arithmetic.

use cray1_fp::vectors::{corner_operands, SplitMix64};
use cray1_fp::{
    fadd, fdiv, fmul, frecip, fsub, pack, unpack, MulKind, Op, Profile, EXP_BIAS, NORM_BIT,
    SIGN_BIT,
};

const MILLION: usize = 1_000_000;

fn signed(word: u64) -> i128 {
    let u = unpack(word);
    if u.sign {
        -i128::from(u.coef)
    } else {
        i128::from(u.coef)
    }
}

/// Add and subtract against exact arithmetic.
///
/// Documented bound (HRM page 3-23: "bits shifted out of the register are lost; no round-up
/// takes place"), in units of the last coefficient bit of the operand with the larger
/// exponent, before normalisation:
///
/// * like signs: the result is low by less than 1 unit, or by less than 2 units when the sum
///   carried (one more bit is dropped);
/// * unlike signs: the result is high in magnitude by less than 1 unit.
///
/// After a normalising left shift of k places the same error is up to 2^k units of the
/// result, which is why the bound is stated before normalisation.
#[test]
fn add_matches_exact_arithmetic_within_the_truncation_bound() {
    let mut rng = SplitMix64::new(0xADD0_0001);
    let mut inexact = 0u32;
    for i in 0..MILLION {
        let d = rng.below(51) as u16;
        let e_small = 0o20100 + rng.below(0o37500) as u16;
        let e_big = e_small + d;
        let big = pack(rng.next_u64() & 1 != 0, e_big, rng.norm_coef());
        let small = pack(rng.next_u64() & 1 != 0, e_small, rng.norm_coef());
        let (a, b) = if i & 1 == 0 {
            (big, small)
        } else {
            (small, big)
        };
        let subtract = i & 2 != 0;
        let r = if subtract { fsub(a, b) } else { fadd(a, b) };
        assert!(!r.range_error);

        // Exact value in units of 2^-d of the larger operand's last bit.
        let b_eff = if subtract { b ^ SIGN_BIT } else { b };
        let (x, y) = if i & 1 == 0 { (a, b_eff) } else { (b_eff, a) }; // x has the larger exponent
        let exact = (signed(x) << d) + signed(y);

        let u = unpack(r.value);
        if exact == 0 {
            assert_eq!(r.value, 0);
            continue;
        }
        assert!(
            u.coef & NORM_BIT != 0,
            "result not normalised: {a:016X} {b:016X} -> {:016X}",
            r.value
        );
        // Result as a whole number of units of the larger operand.
        let shift = i32::from(u.exp) - i32::from(e_big);
        let carried = shift == 1;
        let units = if shift >= 0 {
            i128::from(u.coef) << shift
        } else {
            assert_eq!(
                u.coef & ((1u64 << -shift) - 1),
                0,
                "bits appeared below the register"
            );
            i128::from(u.coef >> -shift)
        };
        let got = (if u.sign { -units } else { units }) << d;
        assert_eq!(got.signum(), exact.signum(), "{a:016X} {b:016X}");
        let like_signs = (signed(x) < 0) == (signed(y) < 0);
        let unit = 1i128 << d;
        let diff = got.abs() - exact.abs();
        if like_signs {
            let bound = if carried { 2 * unit } else { unit };
            assert!(
                diff <= 0 && -diff < bound,
                "{a:016X} {b:016X} -> {:016X} ({diff})",
                r.value
            );
        } else {
            assert!(
                diff >= 0 && diff < unit,
                "{a:016X} {b:016X} -> {:016X} ({diff})",
                r.value
            );
        }
        if diff != 0 {
            inexact += 1;
        }
    }
    assert!(inexact > 100_000, "the test should exercise inexact sums");
}

/// Multiply against the exact 96-bit product.
///
/// With u = 2^-56 and `ulp` the weight of the last result bit (2^-48, or 2^-49 before the
/// shift when the product is below one half), HR-0097B pages 4-29 and 4-30 give:
///
/// * the pyramid drops every logical product right of 2^-56: at most 39u + 2^-96;
/// * nine carries (9u) are always added; 066 adds 2^-50 + 2^-51 = 96u; 065 adds
///   2^-31 + 2^-32 and keeps 29 bits;
/// * the low bits are then discarded.
///
/// So `constants - 39u - 2^-96 - ulp < result - exact <= constants`.
#[test]
fn multiply_matches_the_exact_product_within_the_truncation_bound() {
    let u = 1i128 << 40; // 2^-56 in units of 2^-96
    let mut rng = SplitMix64::new(0x3A11_0002);
    let mut seen_low = false;
    let mut seen_high = false;
    for _ in 0..MILLION {
        let ca = rng.norm_coef();
        let cb = rng.norm_coef();
        let ea = EXP_BIAS - 1000 + rng.below(2000) as u16;
        let eb = EXP_BIAS - 1000 + rng.below(2000) as u16;
        let a = pack(rng.next_u64() & 1 != 0, ea, ca);
        let b = pack(rng.next_u64() & 1 != 0, eb, cb);
        let exact = (u128::from(ca) * u128::from(cb)) as i128; // units of 2^-96 at exponent E
        let e = i32::from(ea) + i32::from(eb) - i32::from(EXP_BIAS);

        for (kind, constants, kept_bits) in [
            (MulKind::Full, 9 * u, 48u32),
            (MulKind::Rounded, 9 * u + 96 * u, 48),
            (MulKind::HalfRounded, 9 * u + (3i128 << 64), 29),
        ] {
            let r = fmul(a, b, kind, Profile::Xmp);
            assert!(!r.range_error);
            let x = unpack(r.value);
            assert_eq!(x.sign, (a ^ b) >> 63 != 0);
            assert!(x.coef & NORM_BIT != 0);
            assert_eq!(x.coef & ((1u64 << (48 - kept_bits)) - 1), 0);
            let shift = e - i32::from(x.exp);
            assert!(shift == 0 || shift == 1, "exponent {e:o} -> {:o}", x.exp);
            // Result in units of 2^-96 at exponent E.
            let got = i128::from(x.coef) << (48 - shift);
            let ulp = 1i128 << (96 - kept_bits as i32 - shift);
            let diff = got - exact;
            assert!(diff <= constants, "{kind:?} {a:016X} {b:016X}: {diff}");
            assert!(
                diff > constants - 39 * u - 1 - ulp,
                "{kind:?} {a:016X} {b:016X}: {diff}"
            );
            if kind == MulKind::Full {
                seen_low |= diff < -(1i128 << (48 - shift));
                seen_high |= diff > 0;
            }
            // The X-MP pyramid is symmetric: the product is commutative.
            assert_eq!(fmul(b, a, kind, Profile::Xmp), r);
        }
    }
    // Unrounded results fall on both sides of the exact product, as the manual says.
    assert!(seen_low && seen_high);
}

fn recip_error(x: u64) -> u128 {
    // x * frecip(x) = c * cr / 2^95 exactly; return |c*cr - 2^95|.
    let c = u128::from(unpack(x).coef);
    let r = frecip(x, Profile::Xmp);
    assert!(!r.range_error);
    let out = unpack(r.value);
    assert_eq!(out.exp, 0o100001 - unpack(x).exp);
    assert!(out.coef & NORM_BIT != 0);
    assert_eq!(
        out.coef & 0x7FFF,
        0,
        "low 15 coefficient bits are always zero"
    );
    (c * u128::from(out.coef)).abs_diff(1u128 << 95)
}

/// The manual says "correct to 30 bits" (HRM page 3-28, HR-0097B page 4-33) in one place and
/// "accurate to 27 bits" (HRM page 4-42) in another. Measured here: |x * frecip(x) - 1| stays
/// below 2^-30 for a million random normalised operands (largest seen about 2^-30.4) and for
/// every table index with all-zero and all-one low bits.
#[test]
fn reciprocal_is_accurate_to_30_bits() {
    let limit = 1u128 << (95 - 30);
    let mut worst = 0u128;
    let mut rng = SplitMix64::new(0x1234_5678);
    for _ in 0..MILLION {
        let x = pack(
            rng.next_u64() & 1 != 0,
            0o20002 + rng.below(0o37776) as u16,
            rng.norm_coef(),
        );
        let err = recip_error(x);
        assert!(err < limit, "{x:016X}: error {err}");
        worst = worst.max(err);
    }
    for (x, _) in corner_operands(Op::Recip, Profile::Xmp) {
        let u = unpack(x);
        if u.coef & NORM_BIT != 0 && (0o20002..=0o57777).contains(&u.exp) {
            let err = recip_error(x);
            assert!(err < limit, "{x:016X}: error {err}");
            worst = worst.max(err);
        }
    }
    // Not much better than 30 bits either: the worst case is above 2^-31.
    println!(
        "reciprocal: worst |x*r - 1| = 2^{:.2}",
        (worst as f64).log2() - 95.0
    );
    assert!(worst > 1u128 << (95 - 31));
}

/// Signed error of a quotient in units of its last coefficient bit, as (numerator, cb):
/// error = numerator / cb.
fn quotient_error(a: u64, b: u64, q: u64) -> (i128, i128) {
    let (ua, ub, uq) = (unpack(a), unpack(b), unpack(q));
    assert!(
        uq.coef & NORM_BIT != 0,
        "{a:016X} / {b:016X} = {q:016X} is not normalised"
    );
    assert_eq!(uq.sign, ua.sign != ub.sign);
    // exact / ulp(q) = ca * 2^n / cb
    let n = 48 + i32::from(ua.exp) - i32::from(ub.exp) - i32::from(uq.exp) + i32::from(EXP_BIAS);
    assert!((40..=56).contains(&n));
    let exact = i128::from(ua.coef) << n;
    let got = i128::from(uq.coef) * i128::from(ub.coef);
    (got - exact, i128::from(ub.coef))
}

/// The four-instruction divide sequence of HRM page 3-28 against the exact quotient.
///
/// It is **not** always within one unit of the last place. Over these million random pairs
/// the quotient is between 2.28 units too small and 1.56 units too large, 0.45 units too
/// small on average, and within one unit four times out of five. The manual's own figure is
/// 47 bits for the correction factor `2 - r*b`, whose coefficient sits just above one half,
/// so one unit of its last bit is already up to two units of the quotient; the two
/// truncating multiplies add about half a unit each. (With cray-sim's rule for 067 the
/// quotient would be 3.7 units low to 0.1 high and within one unit a fifth of the time.)
#[test]
fn divide_sequence_accuracy() {
    let mut rng = SplitMix64::new(0x00AB_CDEF);
    let (mut lowest, mut highest) = (0f64, 0f64);
    let mut within_one = 0usize;
    let mut sum = 0f64;
    for _ in 0..MILLION {
        let a = pack(
            rng.next_u64() & 1 != 0,
            EXP_BIAS - 500 + rng.below(1000) as u16,
            rng.norm_coef(),
        );
        let b = pack(
            rng.next_u64() & 1 != 0,
            EXP_BIAS - 500 + rng.below(1000) as u16,
            rng.norm_coef(),
        );
        let q = fdiv(a, b, Profile::Xmp);
        assert!(!q.range_error);
        let (num, den) = quotient_error(a, b, q.value);
        // Hard bounds: more than 3 units low or 2 units high never happens.
        assert!(
            num > -3 * den && num < 2 * den,
            "{a:016X} / {b:016X} = {:016X}",
            q.value
        );
        let err = num as f64 / den as f64;
        lowest = lowest.min(err);
        highest = highest.max(err);
        sum += err;
        if num.abs() <= den {
            within_one += 1;
        }
    }
    let mean = sum / MILLION as f64;
    assert!((-2.5..-2.0).contains(&lowest), "lowest {lowest}");
    assert!((1.3..1.8).contains(&highest), "highest {highest}");
    assert!((-0.6..-0.3).contains(&mean), "mean {mean}");
    let share = within_one as f64 / MILLION as f64;
    println!("divide (HRM 3-28 order): error {lowest:.3} to {highest:+.3} units, mean {mean:.3}, within one unit {share:.3}");
    assert!((0.6..0.9).contains(&share), "within one unit: {share}");
}

/// The X-MP manual's sequence (HR-0097B page 4-34): the iteration is applied to the
/// reciprocal first and the last multiply is rounded. Measured: 2.32 units low to 2.35 high.
#[test]
fn xmp_divide_sequence_accuracy() {
    let mut rng = SplitMix64::new(0x00AB_CDEF);
    let (mut lowest, mut highest) = (0f64, 0f64);
    for _ in 0..MILLION {
        let a = pack(
            rng.next_u64() & 1 != 0,
            EXP_BIAS - 500 + rng.below(1000) as u16,
            rng.norm_coef(),
        );
        let b = pack(
            rng.next_u64() & 1 != 0,
            EXP_BIAS - 500 + rng.below(1000) as u16,
            rng.norm_coef(),
        );
        let r = frecip(b, Profile::Xmp).value;
        let c = fmul(r, b, MulKind::TwoMinus, Profile::Xmp).value;
        let full = fmul(c, r, MulKind::Full, Profile::Xmp).value;
        let q = fmul(full, a, MulKind::Rounded, Profile::Xmp).value;
        let (num, den) = quotient_error(a, b, q);
        assert!(
            num > -3 * den && num < 3 * den,
            "{a:016X} / {b:016X} = {q:016X}"
        );
        let err = num as f64 / den as f64;
        lowest = lowest.min(err);
        highest = highest.max(err);
    }
    println!("divide (HR-0097B 4-34 order): error {lowest:.3} to {highest:+.3} units");
    assert!(lowest < -2.0 && highest > 2.0, "{lowest} {highest}");
}

/// The reciprocal iteration: with r = frecip(b), `2 - r*b` is 1 + (1 - r*b) to within two
/// units of its own last bit, and never more than a quarter of a unit low. The unit forms
/// 198 - P in units of 2^-56 (see `TwoMinus::Cray`), which is 1.55 units of that bit high.
#[test]
fn reciprocal_iteration_is_two_minus_the_product() {
    let mut rng = SplitMix64::new(0x0067_0067);
    let (mut lowest, mut highest) = (f64::MAX, f64::MIN);
    for _ in 0..200_000 {
        let b = pack(
            false,
            EXP_BIAS - 100 + rng.below(200) as u16,
            rng.norm_coef(),
        );
        let r = frecip(b, Profile::Xmp).value;
        let c = fmul(r, b, MulKind::TwoMinus, Profile::Xmp);
        assert!(!c.range_error);
        let uc = unpack(c.value);
        assert!(uc.coef & NORM_BIT != 0);
        // r*b = cr*cb / 2^95 (see recip_error); 2 - r*b in units of 2^-95 is 2^96 - cr*cb.
        let exact =
            (1i128 << 96) - (u128::from(unpack(r).coef) * u128::from(unpack(b).coef)) as i128;
        // c = coef * 2^(exp - bias - 48); in units of 2^-95: coef << (exp - bias + 47).
        let shift = i32::from(uc.exp) - i32::from(EXP_BIAS) + 47;
        assert!(shift == 47 || shift == 48, "{:o}", uc.exp);
        let got = i128::from(uc.coef) << shift;
        let ulp = 1i128 << shift;
        let diff = got - exact;
        lowest = lowest.min(diff as f64 / ulp as f64);
        highest = highest.max(diff as f64 / ulp as f64);
        assert!(4 * diff > -ulp && diff < 2 * ulp, "{b:016X}: {diff}");
    }
    println!("2 - r*b: error {lowest:.3} to {highest:+.3} units of its last bit");
    assert!(lowest < 0.0 && highest > 1.5, "{lowest} {highest}");
}
