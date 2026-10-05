//! Reference vectors taken from cray-sim's `fp_test.cpp` (see `tests/fp/xmp_ref.vec`).

use std::collections::HashMap;
use std::path::PathBuf;

use cray1_fp::{fmul, fmul_model, read_vector_file, MulKind, MulModel, Op, Profile};

fn reference_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fp/xmp_ref.vec")
}

#[test]
fn xmp_profile_reproduces_every_reference_vector() {
    let vectors =
        read_vector_file(reference_file()).expect("tests/fp/xmp_ref.vec must be readable");
    let mut counts: HashMap<Op, usize> = HashMap::new();
    for v in &vectors {
        let got = v.op.eval(v.a, v.b, Profile::Xmp);
        assert_eq!(got.value, v.result, "{v}: got {:016X}", got.value);
        assert_eq!(got, v.expected(), "{v}: flag mismatch");
        *counts.entry(v.op).or_default() += 1;
    }
    // 24 per table, plus 4 + 1 sums, 1 product and 1 reciprocal from main().
    assert_eq!(counts[&Op::Add], 29);
    assert_eq!(counts[&Op::Mul], 25);
    assert_eq!(counts[&Op::Recip], 25);
    assert_eq!(vectors.len(), 79);
}

#[test]
fn cray1_profile_returns_the_same_results() {
    for v in read_vector_file(reference_file()).unwrap() {
        assert_eq!(
            v.op.eval(v.a, v.b, Profile::Cray1),
            v.op.eval(v.a, v.b, Profile::Xmp),
            "{v}"
        );
    }
}

#[test]
fn product_vectors_do_not_separate_the_two_pyramid_widths() {
    // Both the manual's pyramid (cut after 2^-56) and cray-sim's (cut after 2^-57) reproduce
    // every product vector, so the vectors cannot decide between them.
    for v in read_vector_file(reference_file()).unwrap() {
        if v.op == Op::Mul {
            for model in [MulModel::XMP_MANUAL, MulModel::CRAY_SIM] {
                assert_eq!(
                    fmul_model(v.a, v.b, MulKind::Full, &model).value,
                    v.result,
                    "{v} {model:?}"
                );
            }
        }
    }
}

#[test]
fn excluded_product_case_is_the_rounded_product() {
    // main() of fp_test.cpp expects ...B48F from operator*, which cray-sim does not deliver.
    let (a, b) = (0x4005_CAE2_0FC3_F04D, 0x3FFE_CB56_F313_8000);
    assert_eq!(
        fmul(a, b, MulKind::Full, Profile::Xmp).value,
        0x4003_A126_2B15_B48E
    );
    assert_eq!(
        fmul_model(a, b, MulKind::Full, &MulModel::CRAY_SIM).value,
        0x4003_A126_2B15_B48E
    );
    assert_eq!(
        fmul(a, b, MulKind::Rounded, Profile::Xmp).value,
        0x4003_A126_2B15_B48F
    );
}

#[test]
fn one_sum_vector_rules_out_other_alignment_rules() {
    // Table entry 13: -(.FFFFFFFFFFFF x 2^-2) + (.800000000001 x 2^0) = .800000000004 x 2^-1.
    let (a, b, expected) = (
        0xBFFE_FFFF_FFFF_FFFFu64,
        0x4000_8000_0000_0001u64,
        0x3FFF_8000_0000_0004u64,
    );
    assert_eq!(cray1_fp::fadd(a, b).value, expected);
    let big = 0x8000_0000_0001i128;
    let small = 0xFFFF_FFFF_FFFFi128;
    // Shift left until bit `top` is set.
    let normalise = |mut c: i128, top: u32| {
        while c >> top == 0 {
            c <<= 1;
        }
        c
    };
    // Magnitude truncation of the shifted operand (what the unit does).
    assert_eq!(normalise(big - (small >> 2), 47), 0x8000_0000_0004);
    // Truncating the two's complement of the shifted operand instead (floor of -small/4).
    assert_eq!(normalise(big + ((-small) >> 2), 47), 0x8000_0000_0002);
    // Exact alignment with two guard bits, truncated after normalising.
    assert_eq!(normalise((big << 2) - small, 49) >> 2, 0x8000_0000_0002);
}
