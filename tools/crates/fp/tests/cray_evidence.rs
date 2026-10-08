//! Checks against what Cray itself and real machines say the units do, beyond the
//! reference vectors of `xmp_ref.rs`.
//!
//! * The fixed answers of Cray's multiply diagnostic SFM ("STRONG/WEAK/2MINUS TEST") and
//!   results of its floating point test JFPT's simulation of 067, subroutine SMLT (CRAY J90
//!   offline diagnostic listings, January 1997; both programs go back to 1980).
//! * W. Kahan, "How Cray's arithmetic hurts scientific computation", Cray User Group, June
//!   1990: two operand triples and statistics of X/X from the CRAYs of that time.
//!
//! How they were found and compared is in `docs/spec/fp-multiply.md` of the core's
//! repository.

use cray_xmp_fp::vectors::SplitMix64;
use cray_xmp_fp::{fmul, frecip, fsub, pack, to_f64, MulKind};

fn full(a: u64, b: u64) -> u64 {
    fmul(a, b, MulKind::Full).value
}

fn rounded(a: u64, b: u64) -> u64 {
    fmul(a, b, MulKind::Rounded).value
}

/// Y / X as CAL programs form it: the reciprocal of X (070), its correction factor (067),
/// their product, then the product with Y.
fn divide(y: u64, x: u64, first: fn(u64, u64) -> u64, second: fn(u64, u64) -> u64) -> u64 {
    let r = frecip(x).value;
    let c = fmul(r, x, MulKind::TwoMinus).value;
    second(y, first(r, c))
}

#[test]
fn fixed_answers_of_the_multiply_diagnostic() {
    // The two half-precision answers together admit only round bits at 2^-31 and 2^-32
    // and a 29-bit result; the rounded answer fails with round bits one place lower.
    let half = |a, b| fmul(a, b, MulKind::HalfRounded).value;
    assert_eq!(
        half(0x4001_0000_0001_FFFF, 0x4001_8000_0000_0000),
        0x4001_0000_0000_0000
    );
    assert_eq!(
        half(0x4001_0000_0003_FFFF, 0x4001_8000_0000_0000),
        0x4001_0000_0008_0000
    );
    assert_eq!(
        rounded(0x4001_0000_0000_0001, 0x4001_A000_0000_0000),
        0x4001_0000_0000_0002
    );
}

#[test]
fn reciprocal_iteration_as_the_diagnostic_simulates_it() {
    // (reciprocal approximation r of x, x, 2 - r*x)
    let cases = [
        (
            0x3FFC_9039_5FED_8000u64,
            0x4005_E333_B266_F103u64,
            0x4001_8000_0000_67D9u64,
        ),
        (
            0x3FFF_B81C_F90C_0000,
            0x4002_B1FA_3C80_DB06,
            0x4001_8000_0000_07E8,
        ),
        (
            0x3FFC_B41C_73E8_0000,
            0x4005_B5EE_9E9E_2FA4,
            0x4001_8000_0000_5BB3,
        ),
        (
            0x3FFF_90FE_571B_0000,
            0x4002_E1FF_0E4A_E394,
            0x4001_8000_0000_2DED,
        ),
    ];
    for (r, x, want) in cases {
        assert_eq!(frecip(x).value, r, "{x:016X}");
        let got = fmul(r, x, MulKind::TwoMinus);
        assert_eq!((got.value, got.range_error), (want, false), "{x:016X}");
        // the operands either way round
        assert_eq!(fmul(x, r, MulKind::TwoMinus).value, want);
    }
}

#[test]
fn kahan_operand_triples() {
    // "[B*B] - [A*C] = -2^48" although B*B exceeds A*C: unrounded, then rounded multiply.
    let minus_2_48 = -(2f64.powi(48));
    let (a, b, c) = (
        0x4032_8000_0000_0000u64,
        0x4030_FFFF_FFFF_FFFFu64,
        0x402F_FFFF_FFFF_FFFEu64,
    );
    assert_eq!(to_f64(fsub(full(b, b), full(a, c)).value), minus_2_48);
    let (a, b, c) = (
        0x4030_8000_0958_92E6u64,
        0x4030_B504_FFFF_FFFFu64,
        0x4031_8000_08C0_6C75u64,
    );
    assert_eq!(to_f64(fsub(rounded(b, b), rounded(a, c)).value), minus_2_48);
    // with the unrounded multiply the second triple shows nothing
    assert_eq!(fsub(full(b, b), full(a, c)).value, 0);
}

#[test]
fn kahan_statistics_of_x_over_x() {
    // "[X/X] < 1 at half of randomly chosen values X" with unrounded multiplies; with the
    // last multiply rounded, below 1 "at about one X in 6" and above 1 "at one in 33".
    // (cray-sim's rule for 067 would give 99 percent, and 83 and 0 percent.)
    let mut rng = SplitMix64::new(0xCA4A);
    let n = 200_000u32;
    let (mut ff_low, mut fr_low, mut fr_high) = (0u32, 0u32, 0u32);
    for _ in 0..n {
        let x = pack(false, 0o40000 + rng.below(16) as u16, rng.norm_coef());
        ff_low += u32::from(to_f64(divide(x, x, full, full)) < 1.0);
        let q = to_f64(divide(x, x, full, rounded));
        fr_low += u32::from(q < 1.0);
        fr_high += u32::from(q > 1.0);
    }
    let share = |k: u32| f64::from(k) / f64::from(n);
    println!(
        "X/X: below 1 {:.3} unrounded; {:.3} below and {:.3} above with the last multiply rounded",
        share(ff_low),
        share(fr_low),
        share(fr_high)
    );
    assert!((0.47..0.53).contains(&share(ff_low)), "{}", share(ff_low));
    assert!((0.145..0.19).contains(&share(fr_low)), "{}", share(fr_low));
    assert!(
        (0.025..0.04).contains(&share(fr_high)),
        "{}",
        share(fr_high)
    );
}
