//! One test per special case that the manuals list. Page references: "HRM" is the CRAY-1
//! Hardware Reference Manual 2240004 rev C, "XMP" is HR-0097B.

use cray1_fp::{
    fadd, fmul, frecip, fsub, pack, recip_seed, recip_table_word, unpack, FpResult, MulKind,
    Profile, EXP_BIAS, SIGN_BIT,
};

const HALF: u64 = 0x8000_0000_0000;
const ONES: u64 = 0xFFFF_FFFF_FFFF;
const KINDS: [MulKind; 4] = [
    MulKind::Full,
    MulKind::HalfRounded,
    MulKind::Rounded,
    MulKind::TwoMinus,
];
const PROFILES: [Profile; 2] = [Profile::Xmp, Profile::Cray1];

fn ok(value: u64) -> FpResult {
    FpResult {
        value,
        range_error: false,
    }
}

fn err(value: u64) -> FpResult {
    FpResult {
        value,
        range_error: true,
    }
}

fn w(exp: u16, coef: u64) -> u64 {
    pack(false, exp, coef)
}

// ---------------------------------------------------------------- add unit

#[test]
fn add_range_error_example_from_the_manual() {
    // HRM page 3-22: 60000.4 + 57777.4 = 60000.6 with a range error.
    assert_eq!(
        fadd(w(0o60000, HALF), w(0o57777, HALF)),
        err(w(0o60000, 0xC000_0000_0000))
    );
}

#[test]
fn add_normalises_an_integer_with_exponent_40060() {
    // HRM page 3-21: insert exponent 040060 into a 48-bit integer, then add to zero.
    assert_eq!(fadd(0, w(0o40060, 1)), ok(w(0o40001, HALF)));
    assert_eq!(fadd(0, w(0o40060, 42)), ok(w(0o40006, 42 << 42)));
    assert_eq!(fadd(w(0o40060, ONES), 0), ok(w(0o40060, ONES)));
    assert_eq!(
        cray1_fp::to_f64(fadd(0, pack(true, 0o40060, 1_000_000)).value),
        -1.0e6
    );
}

#[test]
fn add_result_is_normalised_even_if_operands_are_not() {
    // HRM pages 3-17 and 4-39.
    let r = fadd(w(0o40010, 0x0000_0000_0300), w(0o40010, 0x0000_0000_0100));
    assert_eq!(r, ok(w(0o40010 - 37, HALF)));
    assert_eq!(
        fsub(w(0o40010, 0x0000_0000_0300), w(0o40010, 0x0000_0000_0100)),
        ok(w(0o40010 - 38, HALF))
    );
}

#[test]
fn add_zero_operands() {
    let x = pack(true, 0o40003, 0xF000_0000_0000);
    assert_eq!(fadd(0, 0), ok(0));
    assert_eq!(fadd(x, 0), ok(x));
    assert_eq!(fadd(0, x), ok(x));
    // 063 with a zero first operand negates (HRM page 4-39).
    assert_eq!(fsub(0, x), ok(x ^ SIGN_BIT));
    // (Sk) = 2^63 when k = 0: the sign-only word behaves as zero.
    assert_eq!(fadd(x, SIGN_BIT), ok(x));
    assert_eq!(fsub(x, SIGN_BIT), ok(x));
}

#[test]
fn add_never_generates_negative_zero() {
    // HRM page 3-20.
    let x = pack(true, 0o40003, 0xF000_0000_0000);
    assert_eq!(fadd(x, x ^ SIGN_BIT), ok(0));
    assert_eq!(fsub(x, x), ok(0));
    assert_eq!(fadd(SIGN_BIT, SIGN_BIT), ok(0));
    assert_eq!(fadd(SIGN_BIT, 0), ok(0));
    assert_eq!(fadd(pack(true, 0o40000, 0), pack(true, 0o30000, 0)), ok(0));
}

#[test]
fn add_shift_of_48_or_more_loses_the_smaller_operand() {
    let big = w(0o40100, 0xC000_0000_0001);
    for d in [48u16, 49, 63, 64, 1000] {
        assert_eq!(fadd(big, w(0o40100 - d, ONES)), ok(big), "difference {d}");
        assert_eq!(fsub(big, w(0o40100 - d, ONES)), ok(big), "difference {d}");
    }
    // At 47 exactly one bit of the smaller operand survives.
    assert_eq!(fadd(big, w(0o40100 - 47, ONES)), ok(big + 1));
    assert_eq!(fsub(big, w(0o40100 - 47, ONES)), ok(big - 1));
}

#[test]
fn add_carry_shifts_right_and_drops_a_bit() {
    // XMP page 4-28: "the low-order bit is discarded and an appropriate exponent adjustment
    // is made".
    assert_eq!(
        fadd(w(0o40000, ONES), w(0o40000, ONES)),
        ok(w(0o40001, ONES))
    );
    assert_eq!(fadd(w(0o40000, ONES), w(0o40000, 1)), ok(w(0o40001, HALF)));
    assert_eq!(fadd(w(0o40000, ONES), w(0o40000, 2)), ok(w(0o40001, HALF)));
}

#[test]
fn add_underflow_gives_zero_without_an_error() {
    // XMP page 4-24 note: a generated exponent below 020000 returns all zero bits, no fault.
    assert_eq!(fadd(w(0o17777, HALF), 0), ok(0));
    assert_eq!(fadd(w(0o20000, HALF), 0), ok(w(0o20000, HALF)));
    // Cancellation that takes the exponent below 020000.
    assert_eq!(fsub(w(0o20000, 0xC000_0000_0000), w(0o20000, HALF)), ok(0));
    assert_eq!(
        fsub(w(0o20001, 0xC000_0000_0000), w(0o20001, HALF)),
        ok(w(0o20000, HALF))
    );
    assert_eq!(
        fsub(w(0o20057, HALF | 1), w(0o20057, HALF)),
        ok(w(0o20000, HALF))
    );
    assert_eq!(fsub(w(0o20056, HALF | 1), w(0o20056, HALF)), ok(0));
    // Very small exponents must not wrap around into the overflow range.
    assert_eq!(fsub(w(5, HALF | 1), w(5, HALF)), ok(0));
    assert_eq!(fadd(w(0, 1), w(0, 1)), ok(0));
}

#[test]
fn add_accepts_operands_from_the_underflow_range() {
    // Not stated outright in any manual: the unit tests only its result. Two operands at
    // 017777 can carry into the valid range. (cray-sim replaces such operands by zero.)
    assert_eq!(
        fadd(w(0o17777, ONES), w(0o17777, ONES)),
        ok(w(0o20000, ONES))
    );
    assert_eq!(
        fadd(w(0o20005, HALF), w(0o17777, HALF)),
        ok(w(0o20005, HALF | (HALF >> 6)))
    );
}

#[test]
fn add_range_error_follows_the_larger_incoming_exponent() {
    // HRM page 3-22: larger incoming exponent >= 060000 sets the flag and exponent 060000
    // goes to the result with the computed coefficient.
    assert_eq!(fadd(w(0o60000, HALF), 0), err(w(0o60000, HALF)));
    assert_eq!(
        fadd(w(0o77777, ONES), w(0o77777, ONES)),
        err(w(0o60000, ONES))
    );
    assert_eq!(
        fadd(w(0o60000, ONES), w(0o60000, ONES)),
        err(w(0o60000, ONES))
    );
    assert_eq!(
        fadd(pack(true, 0o60001, HALF), w(0o40000, HALF)),
        err(pack(true, 0o60000, HALF))
    );
    // Even when normalisation would bring the true exponent back into range.
    assert_eq!(
        fsub(w(0o60000, HALF | 1), w(0o60000, HALF)),
        err(w(0o60000, HALF))
    );
    // Exactly cancelling out-of-range operands: flag and exponent 060000, zero coefficient.
    // (The manuals do not cover this case; see the crate documentation.)
    assert_eq!(fsub(w(0o60000, HALF), w(0o60000, HALF)), err(w(0o60000, 0)));
    // The flag depends on the exponent only, not on the coefficient that carries it.
    assert_eq!(fadd(w(0o60000, 0), w(0o40000, HALF)), err(w(0o60000, 0)));
}

#[test]
fn a_sum_that_carries_out_of_57777_is_not_an_add_range_error() {
    // The CRAY-1 manual names only the incoming exponent as the add unit's range error
    // (page 3-21, the same in revisions C, E and F). The X-MP also flags a final exponent of
    // 060000 (XMP page 4-24); this library follows the CRAY-1.
    assert_eq!(
        fadd(w(0o57777, HALF), w(0o57777, HALF)),
        ok(w(0o60000, HALF))
    );
    assert_eq!(
        fadd(w(0o57777, HALF), w(0o57776, HALF)),
        ok(w(0o57777, 0xC000_0000_0000))
    );
    assert_eq!(fadd(w(0o57777, ONES), 0), ok(w(0o57777, ONES)));
}

#[test]
fn add_sign_follows_the_larger_magnitude() {
    // 0.5 + (-2.0) = -1.5
    assert_eq!(
        fadd(w(0o40000, HALF), pack(true, 0o40002, HALF)),
        ok(pack(true, 0o40001, 0xC000_0000_0000))
    );
    // An unnormalised operand with the larger exponent can be the smaller magnitude; the
    // sign then comes from the other operand: -(2^-44) + 0.5 = (1 - 2^-43) * 2^-1.
    let r = fadd(pack(true, 0o40004, 1), w(0o40000, HALF));
    assert_eq!(r, ok(w(0o37777, 0xFFFF_FFFF_FFE0)));
}

// ----------------------------------------------------------- multiply unit

#[test]
fn multiply_simple_values() {
    let one = w(0o40001, HALF);
    let half = w(0o40000, HALF);
    for p in PROFILES {
        assert_eq!(fmul(half, half, MulKind::Full, p), ok(w(0o37777, HALF)));
        assert_eq!(
            fmul(one, pack(true, 0o40005, 0xA000_0000_0000), MulKind::Full, p),
            ok(pack(true, 0o40005, 0xA000_0000_0000))
        );
        // 1.5 * 1.5 = 2.25
        assert_eq!(
            fmul(
                w(0o40001, 0xC000_0000_0000),
                w(0o40001, 0xC000_0000_0000),
                MulKind::Full,
                p
            ),
            ok(w(0o40002, 0x9000_0000_0000))
        );
    }
}

#[test]
fn integer_multiply_when_both_exponents_are_zero() {
    for p in PROFILES {
        // XMP page 4-26, figure 4-8: 4 and 6 in bits 47..24 give 30 (octal) in the low bits.
        assert_eq!(fmul(4 << 24, 6 << 24, MulKind::Full, p), ok(0o30));
        // The result is the upper 48 bits of the product, never normalised.
        assert_eq!(fmul(HALF, HALF, MulKind::Full, p), ok(0x4000_0000_0000));
        assert_eq!(fmul(1 << 40, 1 << 40, MulKind::Full, p), ok(1 << 32));
        // Signs multiply as usual (sign and magnitude result).
        assert_eq!(
            fmul(SIGN_BIT | (4 << 24), 6 << 24, MulKind::Full, p),
            ok(SIGN_BIT | 0o30)
        );
        assert_eq!(
            fmul(SIGN_BIT | (4 << 24), SIGN_BIT | (6 << 24), MulKind::Full, p),
            ok(0o30)
        );
        assert_eq!(fmul(0, 0, MulKind::Full, p), ok(0));
    }
}

#[test]
fn integer_multiply_can_be_one_too_large() {
    // XMP page 4-27: with non-zero low bits the truncation compensation constant can make
    // the product one too large. 6 * 0x54 * 2^40 / 2^48 = 1.97, returned as 2. (This case is
    // also in cray-sim's fp_test.cpp.)
    assert_eq!(
        fmul(6, 0x5400_0000_0000, MulKind::Full, Profile::Xmp),
        ok(2)
    );
    assert_eq!(
        fmul(6, 0x5000_0000_0000, MulKind::Full, Profile::Xmp),
        ok(1)
    );
}

#[test]
fn negative_zero_can_leave_the_multiply_unit() {
    // XMP page 4-23: a negative zero is generated only when one goes into the multiply unit.
    assert_eq!(fmul(SIGN_BIT, 0, MulKind::Full, Profile::Xmp), ok(SIGN_BIT));
    assert_eq!(fmul(SIGN_BIT, SIGN_BIT, MulKind::Full, Profile::Xmp), ok(0));
}

#[test]
fn exactly_one_zero_exponent_is_an_underflow() {
    // HRM page 3-22 and XMP figure 4-7 zone 2: result +0, no error, even when the other
    // operand is out of range (XMP page 4-26 note).
    let zero_exp = [0u64, SIGN_BIT, ONES, SIGN_BIT | 5];
    let others = [
        w(1, ONES),
        w(0o17777, HALF),
        w(0o40001, HALF),
        pack(true, 0o57777, ONES),
        w(0o60000, ONES),
        w(0o77777, ONES),
    ];
    for p in PROFILES {
        for kind in KINDS {
            for z in zero_exp {
                for x in others {
                    assert_eq!(fmul(z, x, kind, p), ok(0), "{z:016X} * {x:016X} {kind:?}");
                    assert_eq!(fmul(x, z, kind, p), ok(0), "{x:016X} * {z:016X} {kind:?}");
                }
            }
        }
    }
}

#[test]
fn multiply_overflow_zones() {
    for p in PROFILES {
        // Either operand exponent >= 060000 (zone 7).
        assert_eq!(
            fmul(w(0o60000, HALF), w(0o40001, HALF), MulKind::Full, p),
            err(w(0o60000, HALF))
        );
        assert_eq!(
            fmul(
                w(0o40001, ONES),
                pack(true, 0o77777, ONES),
                MulKind::Full,
                p
            ),
            err(pack(true, 0o60000, 0xFFFF_FFFF_FFFD))
        );
        assert_eq!(
            fmul(w(0o60000, HALF), w(1, HALF), MulKind::Full, p),
            err(w(0o60000, HALF))
        );
        // Exponent sum >= 060000 with both operands in range (zone 7).
        assert_eq!(
            fmul(w(0o50001, ONES), w(0o50000, ONES), MulKind::Full, p),
            err(w(0o60000, 0xFFFF_FFFF_FFFD))
        );
        // Zone 6: the sum is exactly 060000 and the product needs the normalising shift. The
        // true exponent would be 057777, but the test is made before the shift.
        assert_eq!(
            fmul(w(0o50000, HALF), w(0o50000, HALF), MulKind::Full, p),
            err(w(0o60000, HALF))
        );
        // One below the boundary is fine.
        assert_eq!(
            fmul(w(0o50000, ONES), w(0o47777, ONES), MulKind::Full, p),
            ok(w(0o57777, 0xFFFF_FFFF_FFFD))
        );
        assert_eq!(
            fmul(w(0o50000, HALF), w(0o47777, HALF), MulKind::Full, p),
            ok(w(0o57776, HALF))
        );
    }
}

#[test]
fn multiply_underflow_zones() {
    for p in PROFILES {
        // Zone 2: exponent sum below 020000 gives +0 and no error.
        assert_eq!(
            fmul(
                w(0o30000, ONES),
                pack(true, 0o27777, ONES),
                MulKind::Full,
                p
            ),
            ok(0)
        );
        assert_eq!(fmul(w(1, ONES), w(1, ONES), MulKind::Full, p), ok(0));
        // Zone 3: sum exactly 020000. Without a shift the result is in range; with a shift
        // the exponent 017777 comes out and is not zeroed.
        assert_eq!(
            fmul(w(0o30000, ONES), w(0o30000, ONES), MulKind::Full, p),
            ok(w(0o20000, 0xFFFF_FFFF_FFFD))
        );
        assert_eq!(
            fmul(w(0o30000, HALF), w(0o30000, HALF), MulKind::Full, p),
            ok(w(0o17777, HALF))
        );
        // Zone 4: an operand from the underflow range is accepted when the result is in range.
        assert_eq!(
            fmul(w(0o10000, HALF), w(0o57777, HALF), MulKind::Full, p),
            ok(w(0o27776, HALF))
        );
        assert_eq!(
            fmul(w(0o17777, ONES), w(0o40002, HALF), MulKind::Full, p),
            ok(w(0o20000, ONES))
        );
        // The same operand one exponent lower lands on the zone 3 boundary with a shift.
        assert_eq!(
            fmul(w(0o17777, ONES), w(0o40001, HALF), MulKind::Full, p),
            ok(w(0o17777, ONES))
        );
    }
}

#[test]
fn multiply_does_not_normalise_unnormalised_operands() {
    // HRM page 4-40: "The result is not guaranteed to be normalized if the operands are
    // unnormalized." Only the single shift is available.
    let r = fmul(
        w(0o40010, 0x0000_0001_0000),
        w(0o40001, HALF),
        MulKind::Full,
        Profile::Xmp,
    );
    assert_eq!(r, ok(w(0o40010, 0x0000_0001_0000)));
    let z = fmul(w(0o40010, 0), w(0o40001, HALF), MulKind::Full, Profile::Xmp);
    assert_eq!(z, ok(w(0o40010, 0)));
}

#[test]
fn half_precision_keeps_29_bits_and_rounds() {
    // XMP pages 4-30 and 4-35: round bits at 2^-31 and 2^-32, 29 most significant bits of the
    // normalised result kept, low 19 bits zero.
    let third = w(0o37777, 0xAAAA_AAAA_AAAA);
    let three = w(0o40002, 0xC000_0000_0000);
    let r = fmul(third, three, MulKind::HalfRounded, Profile::Xmp);
    // The exact product coefficient is just below one half; the round bits carry it up to
    // exactly one half, so the result is 1.0 with no normalising shift.
    assert_eq!(r, ok(w(0o40001, HALF)));
    assert_eq!(
        fmul(third, three, MulKind::Full, Profile::Xmp),
        ok(w(0o40000, 0xFFFF_FFFF_FFFF))
    );
    for (a, b) in [
        (w(0o40001, ONES), w(0o40001, 0xB504_F333_F9DE)),
        (w(0o40003, 0xDEAD_BEEF_1234), w(0o37770, 0x9E37_79B9_7F4A)),
    ] {
        let h = fmul(a, b, MulKind::HalfRounded, Profile::Xmp);
        assert_eq!(unpack(h.value).coef & 0x7_FFFF, 0);
        let full = fmul(a, b, MulKind::Full, Profile::Xmp);
        // Within one unit of the 29th bit of the full product.
        let diff = unpack(h.value).coef.abs_diff(unpack(full.value).coef);
        assert!(unpack(h.value).exp == unpack(full.value).exp && diff < 1 << 19);
    }
}

#[test]
fn half_precision_round_carry_out_of_the_top_wraps_unverified() {
    // Not covered by any source: two coefficients within 2^-33 of one make the rounded
    // pyramid sum reach 1.0, which does not fit. This model lets the carry fall off.
    let a = w(0o40001, 0xFFFF_FFFF_8000);
    let r = fmul(a, a, MulKind::HalfRounded, Profile::Xmp);
    assert_eq!(r, ok(w(0o40001, 0)));
}

#[test]
fn rounded_multiply_adds_three_eighths_of_the_last_bit() {
    // XMP page 4-30: round bits at 2^-50 and 2^-51, i.e. 3/8 of 2^-48. All logical products
    // of the operands below are left of the pyramid cut, so the pyramid sum is exact and the
    // only other addend is the compensation constant (9/256 of 2^-48).
    //
    // Product at least one half (no shift, last bit 2^-48): .11 x (.11 + n * 2^-48)
    // = .1001 + 0.75 n * 2^-48. The fraction must reach 1 - 3/8 - 9/256 = 0.59 to round up:
    // "25 percent low".
    let y = w(0o40001, 0xC000_0000_0000);
    for (n, full, rounded) in [(1u64, 0u64, 1u64), (2, 1, 1), (3, 2, 2), (4, 3, 3)] {
        let x = w(0o40001, 0xC000_0000_0000 | n);
        assert_eq!(
            fmul(x, y, MulKind::Full, Profile::Xmp),
            ok(w(0o40002, 0x9000_0000_0000 + full)),
            "n = {n}"
        );
        assert_eq!(
            fmul(x, y, MulKind::Rounded, Profile::Xmp),
            ok(w(0o40002, 0x9000_0000_0000 + rounded)),
            "n = {n}"
        );
    }
    // Product below one half (shifted, last bit 2^-49): .101 x (.1 + n * 2^-48)
    // = .0101 + 1.25 n * 2^-49. The same round bits are now 3/4 of the last bit, so a
    // fraction of 0.18 is enough: "50 percent high".
    let y = w(0o40001, 0xA000_0000_0000);
    for (n, full, rounded) in [(1u64, 1u64, 2u64), (2, 2, 3), (3, 3, 4), (4, 5, 5)] {
        let x = w(0o40001, HALF | n);
        assert_eq!(
            fmul(x, y, MulKind::Full, Profile::Xmp),
            ok(w(0o40001, 0xA000_0000_0000 + full)),
            "n = {n}"
        );
        assert_eq!(
            fmul(x, y, MulKind::Rounded, Profile::Xmp),
            ok(w(0o40001, 0xA000_0000_0000 + rounded)),
            "n = {n}"
        );
    }
}

#[test]
fn reciprocal_iteration_of_an_exact_reciprocal_is_wrong() {
    // XMP page 4-34 caution: the iteration must be used once per reciprocal approximation;
    // on an exact reciprocal it gives an incorrect result. The unit forms 2^E - product, so
    // "2 - 1.0 * 1.0" comes out as 3.0.
    let one = w(0o40001, HALF);
    let r = fmul(one, one, MulKind::TwoMinus, Profile::Xmp);
    assert_eq!(cray1_fp::to_f64(r.value), 3.0);
    // With the approximation from the reciprocal unit it works.
    let approx = frecip(one, Profile::Xmp).value;
    let c = fmul(approx, one, MulKind::TwoMinus, Profile::Xmp).value;
    assert!((cray1_fp::to_f64(c) - 1.0).abs() < 1.0e-9);
}

#[test]
fn xmp_multiply_is_commutative_on_awkward_operands() {
    // XMP page 4-30: "The multiplication is commutative".
    let words = [
        w(0o40001, ONES),
        w(0o37000, 0xB504_F333_F9DE),
        pack(true, 0o41234, 0x8000_0000_0001),
        w(0o40000, 0x0123_4567_89AB),
        0x1234_5678_9ABC_DEF0,
    ];
    for kind in KINDS {
        for a in words {
            for b in words {
                assert_eq!(
                    fmul(a, b, kind, Profile::Xmp),
                    fmul(b, a, kind, Profile::Xmp)
                );
            }
        }
    }
}

// --------------------------------------------------------- reciprocal unit

#[test]
fn reciprocal_exponent_and_sign() {
    for p in PROFILES {
        assert_eq!(
            frecip(w(0o40001, HALF), p),
            ok(w(0o40000, 0xFFFF_FFFF_8000))
        );
        assert_eq!(
            frecip(pack(true, 0o40001, HALF), p),
            ok(pack(true, 0o40000, 0xFFFF_FFFF_8000))
        );
        // Result exponent is 0100001 - e over the whole valid range.
        assert_eq!(unpack(frecip(w(0o20002, HALF), p).value).exp, 0o57777);
        assert_eq!(unpack(frecip(w(0o57777, HALF), p).value).exp, 0o20002);
        assert_eq!(
            unpack(frecip(w(EXP_BIAS + 10, ONES), p).value).exp,
            EXP_BIAS - 9
        );
    }
}

#[test]
fn reciprocal_range_errors() {
    // HRM page 3-22: exponent <= 020001 or >= 060000 is a range error; exponent 060000 goes
    // to the result with the computed coefficient. Bit 47 of that coefficient is cleared
    // (cray-sim, confirmed for the zero operand by its test data).
    for p in PROFILES {
        for e in [
            0u16, 1, 0o17777, 0o20000, 0o20001, 0o60000, 0o60001, 0o77777,
        ] {
            for sign in [false, true] {
                let r = frecip(pack(sign, e, HALF), p);
                assert_eq!(
                    r,
                    err(pack(sign, 0o60000, 0x7FFF_FFFF_8000)),
                    "exponent {e:o}"
                );
            }
        }
        for e in [0o20002u16, 0o20003, 0o40000, 0o57776, 0o57777] {
            assert!(!frecip(w(e, HALF), p).range_error, "exponent {e:o}");
        }
        // (Sj) = 0 produces a range error; the result is meaningless (HRM page 4-42).
        assert_eq!(frecip(0, p), err(0x6000_7FFC_02FF_0000));
        assert_eq!(frecip(SIGN_BIT, p), err(0xE000_7FFC_02FF_0000));
    }
}

#[test]
fn reciprocal_does_not_test_bit_47() {
    // HRM page 4-42: "the unit assumes that bit 2^47 of (Sj) = 1; no test is made of this
    // bit". The result is normalised-looking but meaningless: the first Newton step uses the
    // bit as given and the second assumes it set.
    let unnorm = w(0o40001, 0x4000_0000_0000);
    let r = frecip(unnorm, Profile::Xmp);
    assert!(!r.range_error);
    assert!(cray1_fp::is_normalized(r.value));
    assert_ne!(
        r.value,
        frecip(w(0o40001, 0xC000_0000_0000), Profile::Xmp).value
    );
    assert!((cray1_fp::to_f64(r.value) * 0.5 - 1.0).abs() > 0.1);
}

#[test]
fn reciprocal_table_formula_reproduces_the_diagnostic_table() {
    // round(2^15 / (128.5 + i)), packed as (2*A0) << 18 | (A0^2) << 2, was compared entry by
    // entry with the 128-entry table in cray-sim's cray_float.cpp: all equal. The table is
    // not copied here; this is its FNV-1a hash (8 little-endian bytes per entry) taken from
    // that file.
    let mut hash = 0xCBF2_9CE4_8422_2325u64;
    for i in 0..128 {
        let word = u64::from(recip_table_word(i));
        for k in 0..8 {
            hash ^= (word >> (8 * k)) & 0xFF;
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }
    assert_eq!(hash, 0xF310_0741_74A6_E685);
    // The seeds decrease from 255 to 128 and never repeat more than twice.
    assert!((0..127).all(|i| recip_seed(i) >= recip_seed(i + 1)));
    assert!((0..126).all(|i| recip_seed(i) > recip_seed(i + 2)));
}
