//! The vector instructions 140 to 175, the manual's worked examples, and a
//! result register that is also an operand.

use super::*;
use cray_xmp_fp::{fadd, fmul, frecip, from_f64, fsub, MulKind};

/// Set elements 0.. of Vi.
fn fill(m: &mut Machine, i: usize, values: &[u64]) {
    for (e, v) in values.iter().enumerate() {
        m.set_v(i, e, Some(*v));
    }
}

/// Elements 0..n of Vi.
fn elements(m: &Machine, i: usize, n: usize) -> Vec<Option<u64>> {
    (0..n).map(|e| m.v(i, e)).collect()
}

fn some(values: &[u64]) -> Vec<Option<u64>> {
    values.iter().map(|v| Some(*v)).collect()
}

/// A manual figure such as "0 60000 0000 0000 0000 0005": one bit, then 21
/// octal digits.
fn fig(text: &str) -> u64 {
    let digits: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    assert_eq!(digits.len(), 22, "{}", text);
    let top = u64::from_str_radix(&digits[..1], 8).unwrap();
    assert!(top < 2);
    top << 63 | u64::from_str_radix(&digits[1..], 8).unwrap()
}

#[test]
fn vector_logical_140_to_145() {
    // Pages 4-49 and 4-50: the four-bit examples, (Sj) or (Vj element) =
    // 1100 and (Vk element) = 1010; (VL) operations starting at element 0.
    let mut m = monitor_cal("V3 S1&V2; V4 V1&V2; V5 S1!V2; V6 V1!V2; V7 S1\\V2; V0 V1\\V2");
    m.set_vl(Some(2));
    m.set_s(1, Some(0b1100));
    fill(&mut m, 1, &[0b1100, 0b0011, 0b1111]);
    fill(&mut m, 2, &[0b1010, 0b0101, 0b1111]);
    steps(&mut m, 6);
    assert_eq!(elements(&m, 3, 3), [Some(0b1000), Some(0b0100), None]);
    assert_eq!(elements(&m, 4, 3), [Some(0b1000), Some(0b0001), None]);
    assert_eq!(elements(&m, 5, 3), [Some(0b1110), Some(0b1101), None]);
    assert_eq!(elements(&m, 6, 3), [Some(0b1110), Some(0b0111), None]);
    assert_eq!(elements(&m, 7, 3), [Some(0b0110), Some(0b1001), None]);
    assert_eq!(elements(&m, 0, 3), [Some(0b0110), Some(0b0110), None]);

    // Page 4-52: (Sj) = 0 if j = 0: 140i0k clears Vi, 142i0k transmits Vk.
    let mut m = monitor(&[0o140302, 0o142402, 0o144502]);
    m.set_vl(Some(2));
    m.set_s(0, Some(u64::MAX));
    fill(&mut m, 2, &[0b1010, 0b0101]);
    steps(&mut m, 3);
    assert_eq!(elements(&m, 3, 2), some(&[0, 0]));
    assert_eq!(elements(&m, 4, 2), some(&[0b1010, 0b0101]));
    assert_eq!(elements(&m, 5, 2), some(&[0b1010, 0b0101]));
}

#[test]
fn vector_merge_examples_of_page_4_51() {
    // Example 1: (VL) = 4, (VM) = 0 60000 0000 0000 0000 0000, (S2) = -1,
    // V6 = 1, 2, 3, 4.  After 146726, V7 = 1, -1, -1, 4 and "the remaining
    // elements of V7 are unaltered".
    let mut m = monitor(&[0o146726]);
    m.set_vl(Some(4));
    m.set_vm(Some(fig("0 60000 0000 0000 0000 0000")));
    m.set_s(2, Some(u64::MAX));
    fill(&mut m, 6, &[1, 2, 3, 4, 5]);
    fill(&mut m, 7, &[70, 71, 72, 73, 74]);
    steps(&mut m, 1);
    assert_eq!(
        elements(&m, 7, 6),
        [
            Some(1),
            Some(u64::MAX),
            Some(u64::MAX),
            Some(4),
            Some(74),
            None
        ]
    );

    // Example 2: the same VL and VM, V2 = 1, 2, 3, 4 and V3 = -1, -2, -3,
    // -4.  After 147123, V1 = -1, 2, 3, -4.
    let neg = |n: u64| n.wrapping_neg();
    let mut m = monitor(&[0o147123]);
    m.set_vl(Some(4));
    m.set_vm(Some(fig("0 60000 0000 0000 0000 0000")));
    fill(&mut m, 2, &[1, 2, 3, 4, 5]);
    fill(&mut m, 3, &[neg(1), neg(2), neg(3), neg(4), neg(5)]);
    fill(&mut m, 1, &[10, 11, 12, 13, 14]);
    steps(&mut m, 1);
    assert_eq!(elements(&m, 1, 5), some(&[neg(1), 2, 3, neg(4), 14]));
}

#[test]
fn vector_merge_with_zero() {
    // Page 4-52: (Sj) = 0 if j = 0: 146i0k is the merge of (Vk) and 0.
    // Page 4-50: bit 0 of the mask is element 0, bit 63 element 63.
    let mut m = monitor(&[0o146102]);
    m.set_vl(Some(0)); // 64 elements
    m.set_vm(Some(1 << 63 | 1));
    for e in 0..64 {
        m.set_v(2, e, Some(100 + e as u64));
    }
    steps(&mut m, 1);
    assert_eq!(m.v(1, 0), Some(0));
    assert_eq!(m.v(1, 1), Some(101));
    assert_eq!(m.v(1, 62), Some(162));
    assert_eq!(m.v(1, 63), Some(0));
}

#[test]
fn vector_single_shifts() {
    // Page 4-53: end off, zero fill; "elements of Vi are cleared if the
    // shift count exceeds 63"; (Ak) = 1 if k = 0.
    let x = [0x8000_0000_0000_0001u64, 0x00ff_0000_0000_ff00];
    let run = |parcel: u16, count: u32| {
        let mut m = monitor(&[parcel]);
        m.set_vl(Some(2));
        m.set_a(3, Some(count));
        m.set_a(0, Some(40));
        fill(&mut m, 2, &x);
        steps(&mut m, 1);
        elements(&m, 1, 3)
    };
    assert_eq!(
        run(0o150123, 4),
        [Some(0x10), Some(0x0ff0_0000_000f_f000), None]
    );
    assert_eq!(
        run(0o151123, 4),
        [
            Some(0x0800_0000_0000_0000),
            Some(0x000f_f000_0000_0ff0),
            None
        ]
    );
    assert_eq!(
        run(0o150120, 0),
        [Some(2), Some(0x01fe_0000_0001_fe00), None]
    );
    assert_eq!(
        run(0o151120, 0),
        [
            Some(0x4000_0000_0000_0000),
            Some(0x007f_8000_0000_7f80),
            None
        ]
    );
    assert_eq!(run(0o150123, 63), [Some(1 << 63), Some(0), None]);
    assert_eq!(run(0o150123, 64), [Some(0), Some(0), None]);
    assert_eq!(run(0o151123, 64), [Some(0), Some(0), None]);
    assert_eq!(run(0o151123, 0x80_0000), [Some(0), Some(0), None]);
    assert_eq!(
        run(0o150123, 0),
        some(&x).into_iter().chain([None]).collect::<Vec<_>>()
    );
}

#[test]
fn vector_double_shift_left_example_of_page_4_55() {
    // (VL) = 4, (A1) = 3, V4 as below; after 152541 the first four elements
    // of V5 are ...0073, ...0054, ...0067, ...0070.
    let mut m = monitor(&[0o152541]);
    m.set_vl(Some(4));
    m.set_a(1, Some(3));
    fill(
        &mut m,
        4,
        &[
            fig("0 00000 0000 0000 0000 0007"),
            fig("0 60000 0000 0000 0000 0005"),
            fig("1 00000 0000 0000 0000 0006"),
            fig("1 60000 0000 0000 0000 0007"),
            u64::MAX, // beyond VL: the last element is joined with zeros
        ],
    );
    steps(&mut m, 1);
    assert_eq!(
        elements(&m, 5, 5),
        [Some(0o73), Some(0o54), Some(0o67), Some(0o70), None]
    );
}

#[test]
fn vector_double_shift_right_example_of_page_4_57() {
    // (VL) = 4, (A6) = 3, V2 as below; after 153026 V0 holds ...0001,
    // 1 66000..., 1 50000..., 1 56000...; "the remaining elements of V0 are
    // unaltered".
    let mut m = monitor(&[0o153026]);
    m.set_vl(Some(4));
    m.set_a(6, Some(3));
    fill(
        &mut m,
        2,
        &[
            fig("0 00000 0000 0000 0000 0017"),
            fig("0 60000 0000 0000 0000 0006"),
            fig("1 00000 0000 0000 0000 0006"),
            fig("1 60000 0000 0000 0000 0007"),
        ],
    );
    fill(&mut m, 0, &[9, 9, 9, 9, 9]);
    steps(&mut m, 1);
    assert_eq!(
        elements(&m, 0, 5),
        some(&[
            fig("0 00000 0000 0000 0000 0001"),
            fig("1 66000 0000 0000 0000 0000"),
            fig("1 50000 0000 0000 0000 0000"),
            fig("1 56000 0000 0000 0000 0000"),
            9
        ])
    );
}

#[test]
fn vector_double_shift_counts() {
    // Pages 4-54 to 4-57.  152: element e is joined with element e + 1, the
    // last element (as determined by VL) with zeros.  153: element 0 is
    // joined with zeros on the left, element e with element e - 1.  "If
    // (Ak) > 128, the result is all zeros.  If (Ak) > 64, the result
    // register contains (Ak) - 64 zeros."  (Ak) = 1 if k = 0 (page 4-58).
    let x = [
        0x1111_2222_3333_4444u64,
        0x5555_6666_7777_8888,
        0x9999_aaaa_bbbb_cccc,
    ];
    let run = |parcel: u16, vl: u8, count: u32| {
        let mut m = monitor(&[parcel]);
        m.set_vl(Some(vl));
        m.set_a(3, Some(count));
        m.set_a(0, Some(40));
        fill(&mut m, 2, &x);
        steps(&mut m, 1);
        elements(&m, 1, 3)
    };
    // (VL) = 1: "element 0 would have been joined with 64 bits of zero"
    assert_eq!(run(0o152123, 1, 8), [Some(x[0] << 8), None, None]);
    assert_eq!(run(0o153123, 1, 8), [Some(x[0] >> 8), None, None]);
    // (VL) = 2: "element 1 would have been joined with 64 bits of zero"
    assert_eq!(
        run(0o152123, 2, 8),
        [Some(x[0] << 8 | x[1] >> 56), Some(x[1] << 8), None]
    );
    assert_eq!(
        run(0o153123, 2, 8),
        [Some(x[0] >> 8), Some(x[0] << 56 | x[1] >> 8), None]
    );
    // a count of 64 moves whole elements
    assert_eq!(run(0o152123, 3, 64), some(&[x[1], x[2], 0]));
    assert_eq!(run(0o153123, 3, 64), some(&[0, x[0], x[1]]));
    // (Ak) - 64 zeros
    assert_eq!(run(0o152123, 3, 72), some(&[x[1] << 8, x[2] << 8, 0]));
    assert_eq!(run(0o153123, 3, 72), some(&[0, x[0] >> 8, x[1] >> 8]));
    assert_eq!(run(0o152123, 3, 128), some(&[0, 0, 0]));
    assert_eq!(run(0o153123, 3, 128), some(&[0, 0, 0]));
    assert_eq!(run(0o152123, 3, 0), some(&x));
    assert_eq!(run(0o153123, 3, 0), some(&x));
    assert_eq!(
        run(0o152120, 2, 0),
        [Some(x[0] << 1 | x[1] >> 63), Some(x[1] << 1), None]
    );
    assert_eq!(
        run(0o153120, 2, 0),
        [Some(x[0] >> 1), Some(x[0] << 63 | x[1] >> 1), None]
    );
}

#[test]
fn vector_integer_add_154_to_157() {
    // Pages 4-59 and 4-60: 154 (Sj) + (Vk), 155 (Vj) + (Vk), 156 (Sj) -
    // (Vk), 157 (Vj) - (Vk); no overflow is detected.  Special cases: for
    // 154 with j = 0 (Vi element) = (Vk element), for 156 with j = 0
    // (Vi element) = -(Vk element).
    let mut m = monitor_cal("V3 S1+V2; V4 V1+V2; V5 S1-V2; V6 V1-V2");
    m.set_vl(Some(2));
    m.set_s(1, Some(100));
    fill(&mut m, 1, &[10, u64::MAX]);
    fill(&mut m, 2, &[3, 5]);
    steps(&mut m, 4);
    assert_eq!(elements(&m, 3, 3), [Some(103), Some(105), None]);
    assert_eq!(elements(&m, 4, 3), [Some(13), Some(4), None]);
    assert_eq!(elements(&m, 5, 3), [Some(97), Some(95), None]);
    assert_eq!(elements(&m, 6, 3), [Some(7), Some(u64::MAX - 5), None]);

    let mut m = monitor(&[0o154302, 0o156402]);
    m.set_vl(Some(2));
    m.set_s(0, Some(1000));
    fill(&mut m, 2, &[3, 5]);
    steps(&mut m, 2);
    assert_eq!(elements(&m, 3, 2), some(&[3, 5]));
    assert_eq!(
        elements(&m, 4, 2),
        some(&[3u64.wrapping_neg(), 5u64.wrapping_neg()])
    );
}

fn f(v: f64) -> u64 {
    from_f64(v).unwrap()
}

#[test]
fn vector_floating_point_160_to_174() {
    // Pages 4-61 to 4-67: each element is what the floating point unit
    // delivers for the pair of operands; (Sj) = 0 if j = 0.
    let a = [f(6.0), f(-2.5), f(1.0e10)];
    let b = [f(1.5), f(8.0), f(-3.0)];
    let s = f(3.0);
    let check = |parcel: u16, scalar: bool, op: &dyn Fn(u64, u64) -> u64| {
        let mut m = monitor(&[parcel]);
        m.set_vl(Some(3));
        m.set_s(1, Some(s));
        m.set_s(0, Some(f(77.0)));
        fill(&mut m, 1, &a);
        fill(&mut m, 2, &b);
        steps(&mut m, 1);
        for e in 0..3 {
            let x = if scalar { s } else { a[e] };
            assert_eq!(m.v(3, e), Some(op(x, b[e])), "{:06o} element {}", parcel, e);
        }
        assert_eq!(m.v(3, 3), None);
    };
    check(0o160312, true, &|x, y| fmul(x, y, MulKind::Full).value);
    check(0o161312, false, &|x, y| fmul(x, y, MulKind::Full).value);
    check(0o162312, true, &|x, y| {
        fmul(x, y, MulKind::HalfRounded).value
    });
    check(0o163312, false, &|x, y| {
        fmul(x, y, MulKind::HalfRounded).value
    });
    check(0o164312, true, &|x, y| fmul(x, y, MulKind::Rounded).value);
    check(0o165312, false, &|x, y| fmul(x, y, MulKind::Rounded).value);
    check(0o166312, true, &|x, y| fmul(x, y, MulKind::TwoMinus).value);
    check(0o167312, false, &|x, y| fmul(x, y, MulKind::TwoMinus).value);
    check(0o170312, true, &|x, y| fadd(x, y).value);
    check(0o171312, false, &|x, y| fadd(x, y).value);
    check(0o172312, true, &|x, y| fsub(x, y).value);
    check(0o173312, false, &|x, y| fsub(x, y).value);
    // exact cases, to pin the operand order of the differences
    let mut m = monitor_cal("V3 S1-FV2; V4 V1-FV2; V5 +FV2; V6 -FV2; V7 /HV2");
    m.set_vl(Some(1));
    m.set_s(1, Some(s));
    m.set_s(0, Some(f(77.0)));
    fill(&mut m, 1, &a);
    fill(&mut m, 2, &b);
    steps(&mut m, 5);
    assert_eq!(m.v(3, 0), Some(f(1.5)));
    assert_eq!(m.v(4, 0), Some(f(4.5)));
    assert_eq!(m.v(5, 0), Some(f(1.5)));
    assert_eq!(m.v(6, 0), Some(f(-1.5)));
    assert_eq!(m.v(7, 0), Some(frecip(b[0]).value));
}

#[test]
fn vector_mask_test_175() {
    // Page 4-68: k = 0 zero, 1 nonzero, 2 positive (zero counts as
    // positive), 3 negative; bit 0 of VM is element 0; "VM bits
    // corresponding to untested elements of Vj are zeroed".  Page 4-69: the
    // upper bit of k is not interpreted (k = 4 to 7 as 0 to 3).
    let x = [0u64, 5, 1 << 63, u64::MAX, 0, 7];
    let mask = |bits: &[usize]| bits.iter().fold(0u64, |m, b| m | 1 << (63 - b));
    let run = |parcel: u16| {
        let mut m = monitor(&[parcel]);
        m.set_vl(Some(5));
        m.set_vm(Some(u64::MAX));
        fill(&mut m, 2, &x);
        steps(&mut m, 1);
        m.vm().unwrap()
    };
    assert_eq!(run(0o175020), mask(&[0, 4]));
    assert_eq!(run(0o175021), mask(&[1, 2, 3]));
    assert_eq!(run(0o175022), mask(&[0, 1, 4]));
    assert_eq!(run(0o175023), mask(&[2, 3]));
    for k in 0..4 {
        assert_eq!(run(0o175024 + k), run(0o175020 + k));
        assert_eq!(
            run(0o175720 + k),
            run(0o175020 + k),
            "the i field is ignored"
        );
    }
    // all 64 elements
    let mut m = monitor(&[0o175021]);
    m.set_vl(Some(0o100));
    for e in 0..64 {
        m.set_v(2, e, Some((e % 3 == 0) as u64));
    }
    steps(&mut m, 1);
    let expect = (0..64)
        .filter(|e| e % 3 == 0)
        .fold(0u64, |m, e| m | 1 << (63 - e));
    assert_eq!(m.vm(), Some(expect));
}

#[test]
fn vector_population_count_and_parity() {
    // 174ij1: the count goes to the low 7 bits of each
    // element of Vi, "the remaining higher order bits ... are zeroed".
    // 174ij2: the low bit of the count goes to the low bit of the element.
    let mut m = monitor_cal("V1 PV2; V3 QV2");
    m.set_vl(Some(4));
    fill(&mut m, 1, &[9, 9, 9, 9, 9]);
    fill(&mut m, 2, &[0, u64::MAX, 0x0000_00f0_0000_0001, 1 << 63]);
    steps(&mut m, 2);
    assert_eq!(elements(&m, 1, 5), some(&[0, 64, 5, 1, 9]));
    assert_eq!(elements(&m, 3, 4), some(&[0, 0, 1, 1]));
    // 174 with k = 0 and k = 3 to 7 stays the reciprocal
    for k in [0u16, 3, 7] {
        let mut m = monitor(&[0o174120 | k]);
        m.set_vl(Some(1));
        m.set_v(2, 0, Some(f(2.0)));
        steps(&mut m, 1);
        assert_eq!(m.v(1, 0), Some(frecip(f(2.0)).value), "k = {}", k);
    }
    // an undefined element gives an undefined count, the others are done
    let mut m = monitor_cal("V1 PV2");
    m.set_vl(Some(3));
    m.set_v(2, 0, Some(3));
    m.set_v(2, 1, None);
    m.set_v(2, 2, Some(1));
    steps(&mut m, 1);
    assert_eq!(elements(&m, 1, 3), vec![Some(2), None, Some(1)]);
}

#[test]
fn elements_beyond_the_vector_length_are_unaltered() {
    // Page 4-51: "The remaining elements of V7 are unaltered."
    let mut m = monitor_cal("V1 V2+V3");
    m.set_vl(Some(0o101)); // one operation (page 4-10)
    fill(&mut m, 1, &[7, 7, 7]);
    fill(&mut m, 2, &[1, 1, 1]);
    fill(&mut m, 3, &[2, 2, 2]);
    steps(&mut m, 1);
    assert_eq!(elements(&m, 1, 3), some(&[3, 7, 7]));
}

#[test]
fn a_vector_register_as_operand_and_result_is_not_recursive() {
    // X 3-33: "A V register can be used, however, as both an operand and
    // result in the same vector operation."  Nothing is said of the
    // CRAY-1's groups (HRM 3-14): each operation takes the element as it was
    // before the instruction.
    let mut m = monitor_cal("V1 V1+V2; V3 V3,V3>A1");
    m.set_vl(Some(20));
    m.set_a(1, Some(4));
    for e in 0..20 {
        m.set_v(1, e, Some(100 + e as u64));
        m.set_v(2, e, Some(1000));
        m.set_v(3, e, Some(e as u64 + 1));
    }
    steps(&mut m, 1);
    for e in 0..20 {
        assert_eq!(m.v(1, e), Some(1100 + e as u64), "element {}", e);
    }
    // the double shift takes its neighbour as it was before, too
    steps(&mut m, 1);
    assert_eq!(m.v(3, 0), Some(0));
    for e in 1..20u64 {
        assert_eq!(
            m.v(3, e as usize),
            Some(e << 60 | (e + 1) >> 4),
            "element {}",
            e
        );
    }
}
