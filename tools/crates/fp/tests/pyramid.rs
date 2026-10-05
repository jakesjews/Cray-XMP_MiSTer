//! Evidence for the outcome of the CRAY-1 pyramid reconstruction (see `cray1_pyramid`):
//! reconstructed from figure 3-5, consistent with the table on HRM page 3-24 in columns 55
//! to 59, but not uniquely determined, and therefore not used by `fmul`.

use cray1_fp::cray1_pyramid::{
    max_truncation, toy6, Staircase, MANUAL_MAX_TRUNCATION, ROW_CUT_FIGURE,
};
use cray1_fp::vectors::SplitMix64;
use cray1_fp::{fmul, pack, unpack, MulKind, Profile, EXP_BIAS};

#[test]
fn worked_example_case_1() {
    // HRM page 3-26: 0.5 x 0.6 (octal). Pyramid sum .011110000, round bit, result
    // .111101 x 2^-1: one bit higher than the conventional .111100 x 2^-1.
    assert_eq!(toy6(0b101000, 0b110000), (0b011110000, 0b111101, true));
}

#[test]
fn worked_example_case_2() {
    // HRM page 3-27: 0.74 x 0.53, both orders round correctly to .100111.
    assert_eq!(toy6(0b111100, 0b101010), (0b100111000, 0b100111, false));
    assert_eq!(toy6(0b101010, 0b111100), (0b100111010, 0b100111, false));
}

#[test]
fn worked_example_case_3() {
    // HRM page 3-27: not commutative. One order gives .100001 (one bit low), the other
    // .100010 (the correctly rounded value of .100001100110).
    assert_eq!(toy6(0b101011, 0b110010), (0b100001010, 0b100001, false));
    assert_eq!(toy6(0b110010, 0b101011), (0b100001100, 0b100010, false));
}

#[test]
fn staircase_matches_the_missing_product_counts_of_the_table() {
    // "Max. missing log. prod." on page 3-24: 3, 11, 16, 21, 26 under bits -55 to -59.
    for staircase in [Staircase::Figure, Staircase::Table] {
        for column in 2..=54 {
            assert_eq!(staircase.missing_in_column(column), 0, "column {column}");
        }
        let counted: Vec<u32> = (55..=59).map(|c| staircase.missing_in_column(c)).collect();
        assert_eq!(counted, [3, 11, 16, 21, 26]);
    }
    // The table continues 37, 36, 35, 34 under bits -60 to -63: every product missing.
    let table: Vec<u32> = (60..=63)
        .map(|c| Staircase::Table.missing_in_column(c))
        .collect();
    assert_eq!(table, [37, 36, 35, 34]);
    // The figure as drawn disagrees there: rows 31-36 and 43-48 extend further right.
    let figure: Vec<u32> = (60..=63)
        .map(|c| Staircase::Figure.missing_in_column(c))
        .collect();
    assert_eq!(figure, [25, 30, 29, 34]);
    // Rows 1 to 8 are complete.
    for row in 1..=8u32 {
        assert_eq!(u32::from(ROW_CUT_FIGURE[(row - 1) as usize]), row + 48);
    }
}

#[test]
fn stated_maximum_truncation_is_not_reproduced() {
    // The manual: "approximately 2^-59 x 317 + 2^-96". The missing logical products of the
    // table reading sum to exactly 2^-59 x 304 + 2^-96; the figure reading gives less.
    assert_eq!(max_truncation(Staircase::Table), (304u128 << 37) + 1);
    assert!(max_truncation(Staircase::Figure) < max_truncation(Staircase::Table));
    // The difference is the table's unexplained row "5 3" under bits -58 and -59:
    // 5 x 2^-58 + 3 x 2^-59 = 13 x 2^-59.
    assert_eq!(
        MANUAL_MAX_TRUNCATION - max_truncation(Staircase::Table),
        13u128 << 37
    );
    assert_eq!(13u128 << 37, (5u128 << 38) + (3u128 << 37));
    // Either way the constant at 2^-51 and 2^-52 (3 x 2^-52) exceeds the largest truncation,
    // which is the manual's "product variation" of 1.16e-16 to 6.66e-16.
    let constant = 3u128 << 44;
    assert!(constant > MANUAL_MAX_TRUNCATION);
    let low = (constant - MANUAL_MAX_TRUNCATION) as f64 / 2f64.powi(96);
    let high = constant as f64 / 2f64.powi(96);
    assert!((low - 1.16e-16).abs() < 0.01e-16 && (high - 6.66e-16).abs() < 0.01e-16);
}

#[test]
fn staircase_product_is_exact_or_one_high_and_not_commutative() {
    // Page 3-24: "the possibility of rounding up 2^-48"; page 3-26: "A x B is not
    // necessarily the same as B x A".
    let mut rng = SplitMix64::new(0xC8A1_0001);
    let n = 200_000;
    for staircase in [Staircase::Figure, Staircase::Table] {
        let (mut high, mut asymmetric, mut differs_from_xmp) = (0u32, 0u32, 0u32);
        for _ in 0..n {
            let a = rng.norm_coef();
            let b = rng.norm_coef();
            let exact = u128::from(a) * u128::from(b);
            let truncated = if exact >> 95 != 0 {
                (exact >> 48) as u64
            } else {
                (exact >> 47) as u64
            };
            let (coef, shifted) = staircase.product(a, b);
            let exact_shifted = exact >> 95 == 0;
            // Compare in units of the exact product's last kept bit.
            let dev = if shifted == exact_shifted {
                coef as i64 - truncated as i64
            } else {
                // The constant pushed the sum across one half.
                assert!(exact_shifted && !shifted && coef == 0x8000_0000_0000);
                (1i64 << 48) - truncated as i64
            };
            assert!(dev == 0 || dev == 1, "{a:012X} x {b:012X}: {dev}");
            high += dev as u32;
            asymmetric += u32::from(staircase.product(b, a) != (coef, shifted));
            let xmp = fmul(
                pack(false, EXP_BIAS, a),
                pack(false, EXP_BIAS, b),
                MulKind::Full,
                Profile::Xmp,
            );
            differs_from_xmp += u32::from(unpack(xmp.value).coef != coef);
        }
        println!(
            "{staircase:?}: one high {:.2}%, A x B != B x A {:.2}%, differs from the X-MP product {:.2}%",
            100.0 * f64::from(high) / f64::from(n),
            100.0 * f64::from(asymmetric) / f64::from(n),
            100.0 * f64::from(differs_from_xmp) / f64::from(n)
        );
        assert!(high > 0 && asymmetric > 0 && differs_from_xmp > 0);
        assert!(f64::from(asymmetric) / f64::from(n) < 0.05);
    }
}

#[test]
fn disputed_fp_test_product_matches_the_staircase() {
    // cray-sim's fp_test.cpp main() expects 4005CAE20FC3F04D x 3FFECB56F3138000 to give
    // ...B48F from the unrounded multiply. cray-sim itself and the X-MP pyramid give ...B48E
    // (tests/xmp_ref.rs). Both staircase readings give ...B48F, in either operand order. One
    // case of unknown origin proves nothing: the X-MP rounded product is also ...B48F.
    let (a, b) = (0xCAE2_0FC3_F04Du64, 0xCB56_F313_8000u64);
    for staircase in [Staircase::Figure, Staircase::Table] {
        assert_eq!(staircase.product(a, b), (0xA126_2B15_B48F, false));
        assert_eq!(staircase.product(b, a), (0xA126_2B15_B48F, false));
    }
    let xmp = fmul(
        pack(false, 0o40005, a),
        pack(false, 0o37776, b),
        MulKind::Full,
        Profile::Xmp,
    );
    assert_eq!(xmp.value, 0x4003_A126_2B15_B48E);
}

#[test]
fn reference_products_are_xmp_results_not_staircase_results() {
    // The 24 table products of fp_test.cpp are all reproduced by the X-MP pyramid
    // (tests/xmp_ref.rs). The staircase gives a coefficient one higher on four of them, so
    // those vectors say nothing in favour of this reconstruction and cannot validate a
    // CRAY-1 profile.
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fp/xmp_ref.vec");
    let (mut total, mut same, mut one_higher) = (0, 0, 0);
    for v in cray1_fp::read_vector_file(path).unwrap() {
        let (a, b, r) = (unpack(v.a), unpack(v.b), unpack(v.result));
        if v.op != cray1_fp::Op::Mul || a.exp == 0 || b.exp == 0 {
            continue;
        }
        total += 1;
        let (coef, shifted) = Staircase::Figure.product(a.coef, b.coef);
        assert_eq!((coef, shifted), Staircase::Figure.product(b.coef, a.coef));
        assert_eq!((coef, shifted), Staircase::Table.product(a.coef, b.coef));
        assert_eq!(
            i32::from(r.exp),
            i32::from(a.exp) + i32::from(b.exp) - 0o40000 - i32::from(shifted)
        );
        match coef - r.coef {
            0 => same += 1,
            1 => one_higher += 1,
            other => panic!("{v}: staircase differs by {other}"),
        }
    }
    assert_eq!((total, same, one_higher), (24, 20, 4));
}
