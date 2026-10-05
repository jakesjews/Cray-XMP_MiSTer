//! A and S register instructions, 020 to 077 except the block transfers.

use super::*;
use cray1_fp::{fadd, fmul, frecip, from_f64, fsub, MulKind, Profile};

#[test]
fn special_register_values() {
    // Page 4-5: in the j and k fields A0 reads as 0 and 1, S0 as 0 and
    // 2**63; in the i field the registers are read.
    let mut m = monitor(&[
        0o030100, // A1 = (A0 as j) + (A0 as k) = 0 + 1
        0o031200, // A2 = 0 - 1
        0o032310, // A3 = A1 * (A0 as k) = A1 * 1
        0o060100, // S1 = (S0 as j) + (S0 as k) = 2**63
        0o051200, // S2 = 0 | 2**63
        0o054003, // S0 = (S0 as i) << 3: the real S0
        0o030001, // A0 = (A0 as j) + A1 = A1
    ]);
    m.set_a(0, Some(0o1234));
    m.set_s(0, Some(0x55));
    steps(&mut m, 7);
    assert_eq!(m.a(1), Some(1));
    assert_eq!(m.a(2), Some(0xff_ffff));
    assert_eq!(m.a(3), Some(1));
    assert_eq!(m.s(1), Some(1 << 63));
    assert_eq!(m.s(2), Some(1 << 63));
    assert_eq!(m.s(0), Some(0x55 << 3));
    assert_eq!(m.a(0), Some(1));
}

#[test]
fn a_immediates() {
    // Page 4-19: 020 enters jkm with two upper bits of zero; 021 its
    // complement, so the two upper bits are one.  Page 4-20: 022 enters jk,
    // no sign extension.
    let mut m = monitor(&[
        0o020177, 0o177777, 0o021200, 0o000000, 0o021377, 0o177777, 0o022477,
    ]);
    steps(&mut m, 4);
    assert_eq!(m.a(1), Some(0x3f_ffff));
    assert_eq!(m.a(2), Some(0xff_ffff));
    assert_eq!(m.a(3), Some(0xc0_0000));
    assert_eq!(m.a(4), Some(0o77));
}

#[test]
fn a_from_s_and_b() {
    // Page 4-21: 023 enters the low 24 bits of (Sj); (Sj) = 0 if j = 0.
    // Page 4-22: 024 and 025 move between Ai and Bjk.
    let mut m = monitor_cal("A1 S2; A3 S0; B17 A1; A4 B17");
    m.set_s(2, Some(0xdead_beef_1234_5678));
    m.set_s(0, Some(u64::MAX));
    steps(&mut m, 4);
    assert_eq!(m.a(1), Some(0x34_5678));
    assert_eq!(m.a(3), Some(0));
    assert_eq!(m.b(0o17), Some(0x34_5678));
    assert_eq!(m.a(4), Some(0x34_5678));
}

#[test]
fn population_and_leading_zero_counts() {
    // Page 4-23: 026 counts the one bits; (Ai) = 0 if j = 0.
    // Page 4-24: 027 counts the leading zeros; (Ai) = 64 if j = 0 and 0 if
    // (Sj) is negative.
    let mut m = monitor_cal("A1 PS1; A2 PS0; A3 ZS1; A4 ZS0; A5 ZS2; A6 PS2; A7 ZS3");
    m.set_s(0, Some(u64::MAX));
    m.set_s(1, Some(0x0000_00f0_0000_0001));
    m.set_s(2, Some(u64::MAX));
    m.set_s(3, Some(1));
    steps(&mut m, 7);
    assert_eq!(m.a(1), Some(5));
    assert_eq!(m.a(2), Some(0));
    assert_eq!(m.a(3), Some(24));
    assert_eq!(m.a(4), Some(64));
    assert_eq!(m.a(5), Some(0));
    assert_eq!(m.a(6), Some(64));
    assert_eq!(m.a(7), Some(63));
}

#[test]
fn a_arithmetic() {
    // Page 4-25, special cases of 030 and 031; page 4-26, of 032.  No
    // overflow is detected: the arithmetic is 24-bit two's complement
    // (page 3-10).
    let mut m = monitor(&[
        0o030312, // A3 = A1 + A2
        0o031412, // A4 = A1 - A2
        0o031521, // A5 = A2 - A1
        0o032612, // A6 = A1 * A2
        0o030702, // A7 = A2          (j = 0, k != 0)
    ]);
    m.set_a(1, Some(0xff_fff0));
    m.set_a(2, Some(0x20));
    steps(&mut m, 5);
    assert_eq!(m.a(3), Some(0x10)); // wraps
    assert_eq!(m.a(4), Some(0xff_ffd0));
    assert_eq!(m.a(5), Some(0x30));
    assert_eq!(m.a(6), Some(0xff_fe00)); // -16 * 32
    assert_eq!(m.a(7), Some(0x20));

    let mut m = monitor(&[
        0o030300, // 1                (j = 0 and k = 0)
        0o030410, // (A1) + 1         (j != 0 and k = 0)
        0o031502, // -(A2)            (j = 0 and k != 0)
        0o031600, // -1               (j = 0 and k = 0)
        0o031710, // (A1) - 1         (j != 0 and k = 0)
    ]);
    m.set_a(1, Some(100));
    m.set_a(2, Some(7));
    steps(&mut m, 5);
    assert_eq!(m.a(3), Some(1));
    assert_eq!(m.a(4), Some(101));
    assert_eq!(m.a(5), Some(0xff_fff9));
    assert_eq!(m.a(6), Some(0xff_ffff));
    assert_eq!(m.a(7), Some(99));

    // 032: (Ai) = 0 if j = 0; (Ai) = (Aj) if k = 0 and j != 0.
    let mut m = monitor(&[0o032302, 0o032410, 0o032512]);
    m.set_a(1, Some(0x12_3456));
    m.set_a(2, Some(0x1000));
    steps(&mut m, 3);
    assert_eq!(m.a(3), Some(0));
    assert_eq!(m.a(4), Some(0x12_3456));
    assert_eq!(m.a(5), Some(0x45_6000)); // low 24 bits of the product
}

#[test]
fn channel_status_reads_zero() {
    // Project definition: no channels are attached, 033 delivers 0
    // (whatever (Aj) is, even undefined).
    let mut m = monitor_cal("A1 CI; A2 CA,A3; A4 CE,A3");
    m.set_a(1, Some(5));
    m.set_a(2, Some(5));
    m.set_a(3, None);
    m.set_a(4, Some(5));
    steps(&mut m, 3);
    assert_eq!((m.a(1), m.a(2), m.a(4)), (Some(0), Some(0), Some(0)));
}

#[test]
fn s_immediates() {
    // Page 4-31: 040 enters jkm with 42 upper bits of zero, 041 its
    // complement with 42 upper bits of one.
    let mut m = monitor(&[0o040177, 0o177777, 0o041200, 0o000005]);
    steps(&mut m, 2);
    assert_eq!(m.s(1), Some(0x3f_ffff));
    assert_eq!(m.s(2), Some(!5u64));
}

#[test]
fn masks_042_043() {
    // Page 4-32.  042: 64 - jk ones from the right: "if jk = 0, Si contains
    // all one bits and if jk = 77, Si contains zeros in all but the lowest
    // order bit".  043: jk ones from the left: "if jk = 0, Si contains all
    // zeroed bits and if jk = 77, Si contains ones in all but the lowest
    // order bit".
    let mut m = monitor(&[0o042100, 0o042277, 0o043300, 0o043477, 0o042560, 0o043614]);
    steps(&mut m, 6);
    assert_eq!(m.s(1), Some(u64::MAX));
    assert_eq!(m.s(2), Some(1));
    assert_eq!(m.s(3), Some(0));
    assert_eq!(m.s(4), Some(u64::MAX - 1));
    assert_eq!(m.s(5), Some(0xffff)); // 64 - 48 = 16 ones from the right
    assert_eq!(m.s(6), Some(0xfff0_0000_0000_0000)); // 12 ones from the left
}

#[test]
fn logical_products_sums_differences() {
    // Pages 4-33 to 4-35: the four-bit examples of 044, 045, 046, 047 and
    // 051 with (Sj) = 1100 and (Sk) = 1010.
    let mut m = monitor_cal("S3 S1&S2; S4 #S2&S1; S5 S1\\S2; S6 #S1\\S2; S7 S1!S2");
    m.set_s(1, Some(0b1100));
    m.set_s(2, Some(0b1010));
    steps(&mut m, 5);
    assert_eq!(m.s(3), Some(0b1000));
    assert_eq!(m.s(4), Some(0b0100));
    assert_eq!(m.s(5), Some(0b0110));
    assert_eq!(m.s(6), Some(!0b0110u64)); // 1001 in the four bits shown
    assert_eq!(m.s(7), Some(0b1110));
}

#[test]
fn logical_special_cases() {
    const SIGN: u64 = 1 << 63;
    let x = 0x8123_4567_89ab_cdefu64;
    let y = 0x0fed_cba9_8765_4321u64;
    let run = |parcel: u16| {
        let mut m = monitor(&[parcel]);
        m.set_s(0, Some(0x7777));
        m.set_s(1, Some(x));
        m.set_s(2, Some(y));
        m.set_s(3, Some(0x00ff_00ff_00ff_00ff));
        steps(&mut m, 1);
        m.s(3).unwrap()
    };
    // Page 4-33, 044: (Sj) if j = k != 0; cleared if j = 0; the sign bit of
    // (Sj) if j != 0 and k = 0.
    assert_eq!(run(0o044311), x);
    assert_eq!(run(0o044302), 0);
    assert_eq!(run(0o044310), SIGN);
    assert_eq!(run(0o044320), 0);
    // 045: cleared if j = k or j = 0; (Sj) with the sign bit cleared if
    // j != 0 and k = 0.
    assert_eq!(run(0o045311), 0);
    assert_eq!(run(0o045302), 0);
    assert_eq!(run(0o045310), x & !SIGN);
    // Page 4-34, 046: cleared if j = k != 0; (Sk) if j = 0, k != 0; the
    // sign bit of (Sj) complemented if j != 0, k = 0.
    assert_eq!(run(0o046322), 0);
    assert_eq!(run(0o046302), y);
    assert_eq!(run(0o046310), x ^ SIGN);
    // 047: all ones if j = k != 0; the complement of (Sk) if j = 0, k != 0;
    // all bits but the sign bit of (Sj) complemented if j != 0, k = 0.
    assert_eq!(run(0o047322), u64::MAX);
    assert_eq!(run(0o047302), !y);
    assert_eq!(run(0o047310), !x ^ SIGN);
    // Page 4-35, 051: (Sj) if j = k != 0; (Sk) if j = 0, k != 0; (Sj) with
    // the sign bit set if j != 0, k = 0; only the sign bit if j = k = 0.
    assert_eq!(run(0o051322), y);
    assert_eq!(run(0o051302), y);
    assert_eq!(run(0o051320), y | SIGN);
    assert_eq!(run(0o051300), SIGN);
}

#[test]
fn scalar_merge_050() {
    // Page 4-34: (Si) = (Sj)(Sk) + (Si)(not Sk), with the example
    // (Sk) = 11110000, (Si) = 11001100, (Sj) = 10101010 -> 10101100.
    let mut m = monitor(&[0o050312]);
    m.set_s(2, Some(0b1111_0000));
    m.set_s(3, Some(0b1100_1100));
    m.set_s(1, Some(0b1010_1010));
    steps(&mut m, 1);
    assert_eq!(m.s(3), Some(0b1010_1100));

    // Page 4-35: bits of Si are cleared where (Sk) is one if j = 0, k != 0;
    // the sign bit of (Sj) replaces that of Si if j != 0, k = 0; the sign
    // bit of Si is cleared if j = k = 0.
    let run = |parcel: u16, si: u64| {
        let mut m = monitor(&[parcel]);
        m.set_s(0, Some(0x7777));
        m.set_s(1, Some(0x8000_0000_0000_00ff));
        m.set_s(2, Some(0xffff_0000_0000_0000));
        m.set_s(3, Some(si));
        steps(&mut m, 1);
        m.s(3).unwrap()
    };
    assert_eq!(run(0o050302, u64::MAX), 0x0000_ffff_ffff_ffff);
    assert_eq!(run(0o050310, 0x1234), 0x8000_0000_0000_1234);
    assert_eq!(run(0o050300, u64::MAX), u64::MAX >> 1);
}

#[test]
fn single_shifts_052_to_055() {
    // Page 4-36: 052 shifts (Si) left jk places into S0, 053 right 64 - jk
    // places into S0, 054 and 055 the same into Si; end off, zero fill.
    let x = 0x8000_0000_0000_00f1u64;
    let mut m = monitor(&[
        0o052304, // S0 = S3 << 4
        0o075010, // T10 = S0 (keep it)
        0o053374, // S0 = S3 >> (64 - 60) = S3 >> 4
        0o054304, // S3 = S3 << 4
        0o055470, // S4 = S4 >> 8
        0o055500, // S5 = S5 >> 64
        0o054600, // S6 = S6 << 0
        0o053100, // S0 = S1 >> 64
    ]);
    for i in 1..7 {
        m.set_s(i, Some(x));
    }
    steps(&mut m, 2);
    assert_eq!(m.t(0o10), Some(0xf10));
    steps(&mut m, 1);
    assert_eq!(m.s(0), Some(0x0800_0000_0000_000f));
    steps(&mut m, 4);
    assert_eq!(m.s(3), Some(0xf10));
    assert_eq!(m.s(4), Some(0x0080_0000_0000_0000));
    assert_eq!(m.s(5), Some(0));
    assert_eq!(m.s(6), Some(x));
    steps(&mut m, 1);
    assert_eq!(m.s(0), Some(0));
}

#[test]
fn double_shifts_056_057() {
    let hi = 0x0123_4567_89ab_cdefu64;
    let lo = 0xfedc_ba98_7654_3210u64;
    let run = |parcel: u16, count: u32| {
        let mut m = monitor(&[parcel]);
        m.set_s(1, Some(hi));
        m.set_s(2, Some(lo));
        m.set_a(3, Some(count));
        m.set_a(0, Some(40)); // never used as the count: k = 0 means 1
        steps(&mut m, 1);
        (m.s(1).unwrap(), m.s(2).unwrap())
    };
    // Page 4-37, 056: (Si) is the most significant half; the high 64 bits
    // of the result go to Si; (Sj) is unchanged.
    assert_eq!(run(0o056123, 8), (0x2345_6789_abcd_effe, lo));
    assert_eq!(run(0o056123, 64), (lo, lo));
    assert_eq!(run(0o056123, 68), (lo << 4, lo));
    // "Si is cleared if the shift count exceeds 127."
    assert_eq!(run(0o056123, 127).0, lo << 63);
    assert_eq!(run(0o056123, 128).0, 0);
    assert_eq!(run(0o056123, 0x80_0000).0, 0);
    // "A shift of one place occurs if the k designator is zero."
    assert_eq!(run(0o056120, 0).0, hi << 1 | lo >> 63);
    // "The 056 instruction produces the same result as the 054 instruction
    // if the shift count does not exceed 63 and the j designator is zero."
    assert_eq!(run(0o056103, 12).0, hi << 12);
    // "effectively a circular shift if the shift count does not exceed 64
    // and the i and j designators are equal and nonzero"
    assert_eq!(run(0o056113, 12).0, hi.rotate_left(12));
    assert_eq!(run(0o056113, 64).0, hi);

    // 057: (Sj) is the most significant half; the low 64 bits go to Si.
    assert_eq!(run(0o057123, 8).0, 0x1001_2345_6789_abcd);
    assert_eq!(run(0o057123, 64).0, lo);
    assert_eq!(run(0o057123, 68).0, lo >> 4);
    assert_eq!(run(0o057123, 128).0, 0);
    assert_eq!(run(0o057120, 0).0, hi >> 1 | lo << 63);
    assert_eq!(run(0o057103, 12).0, hi >> 12);
    assert_eq!(run(0o057113, 12).0, hi.rotate_right(12));
}

#[test]
fn s_integer_arithmetic() {
    // Page 4-38, with its special cases; no overflow is detected.
    const SIGN: u64 = 1 << 63;
    let run = |parcel: u16| {
        let mut m = monitor(&[parcel]);
        m.set_s(0, Some(0x7777));
        m.set_s(1, Some(u64::MAX - 1));
        m.set_s(2, Some(5));
        steps(&mut m, 1);
        m.s(3).unwrap()
    };
    assert_eq!(run(0o060312), 3); // wraps
    assert_eq!(run(0o061321), 7); // 5 - (-2)
    assert_eq!(run(0o061312), u64::MAX - 6);
    assert_eq!(run(0o060302), 5); // (Si) = (Sk) if j = 0 and k != 0
    assert_eq!(run(0o060300), SIGN); // (Si) = 2**63 if j = 0 and k = 0
    assert_eq!(run(0o060320), 5 ^ SIGN); // (Sj) with 2**63 complemented
    assert_eq!(run(0o061302), 5u64.wrapping_neg()); // (Si) = -(Sk)
    assert_eq!(run(0o061320), 5 ^ SIGN); // (Sj) with 2**63 complemented
}

fn f(v: f64) -> u64 {
    from_f64(v).unwrap()
}

#[test]
fn floating_point_instructions_use_the_fp_units() {
    // Pages 4-39 to 4-42: 062 to 070 deliver what the floating point units
    // deliver for (Sj) and (Sk), with (Sj) = 0 if j = 0 and (Sk) = 2**63 if
    // k = 0.
    let (a, b) = (f(6.0), f(1.5));
    let run = |parcel: u16| {
        let mut m = monitor(&[parcel]);
        m.set_s(0, Some(f(100.0)));
        m.set_s(1, Some(a));
        m.set_s(2, Some(b));
        steps(&mut m, 1);
        m.s(3).unwrap()
    };
    let p = Profile::Cray1;
    assert_eq!(run(0o062312), f(7.5));
    assert_eq!(run(0o062312), fadd(a, b).value);
    assert_eq!(run(0o063312), f(4.5));
    assert_eq!(run(0o063312), fsub(a, b).value);
    assert_eq!(run(0o064312), fmul(a, b, MulKind::Full, p).value);
    assert_eq!(run(0o065312), fmul(a, b, MulKind::HalfRounded, p).value);
    assert_eq!(run(0o066312), fmul(a, b, MulKind::Rounded, p).value);
    assert_eq!(run(0o067312), fmul(a, b, MulKind::TwoMinus, p).value);
    assert_eq!(run(0o070310), frecip(a, p).value);
    // Page 4-39: (Si) = (Sk) normalized if j = 0; -(Sk) normalized for 063.
    assert_eq!(run(0o062302), b);
    assert_eq!(run(0o063302), fsub(0, b).value);
    assert_eq!(run(0o063302), f(-1.5));
    assert_eq!(run(0o062310), fadd(a, 1 << 63).value);
    assert_eq!(run(0o064310), fmul(a, 1 << 63, MulKind::Full, p).value);
    assert_eq!(run(0o070300), frecip(0, p).value);
}

#[test]
fn integer_to_floating_point_by_071_and_062() {
    // Page 4-43: 071i2k gives exponent 40060 and the 24-bit magnitude as
    // coefficient; page 3-21: adding it to zero (062i0k) normalizes.
    let mut m = monitor_cal("S1 +FA2; S1 +FS1; S3 +FA4; S3 +FS3");
    m.set_a(2, Some(5));
    m.set_a(4, Some(0xff_fffb)); // -5
    steps(&mut m, 1);
    assert_eq!(m.s(1), Some(0o40060 << 48 | 5));
    steps(&mut m, 1);
    assert_eq!(m.s(1), Some(0x4003_a000_0000_0000)); // 0.101 (binary) * 2**3
    assert_eq!(m.s(1), Some(f(5.0)));
    steps(&mut m, 1);
    assert_eq!(m.s(3), Some(1 << 63 | 0o40060 << 48 | 5));
    steps(&mut m, 1);
    assert_eq!(m.s(3), Some(f(-5.0)));
}

#[test]
fn transmit_071_variants() {
    // Pages 4-43 and 4-44.
    let mut m = monitor(&[
        0o071102, // S1 = (A2), no sign extension
        0o071212, // S2 = (A2), sign extended
        0o071322, // S3 = (A2) as unnormalized floating point
        0o071430, // 0.75 * 2**48
        0o071540, // 0.5
        0o071650, // 1.0
        0o071760, // 2.0
        0o071070, // 4.0
    ]);
    m.set_a(2, Some(0x80_0001));
    steps(&mut m, 8);
    assert_eq!(m.s(1), Some(0x80_0001));
    assert_eq!(m.s(2), Some(0xffff_ffff_ff80_0001));
    // the two's complement of (Ak) is the magnitude, bit 0 the sign
    assert_eq!(m.s(3), Some(1 << 63 | 0o40060 << 48 | 0x7f_ffff));
    // "(Si) = 0.6 x 2**60 (octal)": exponent 40060, coefficient .6 octal
    assert_eq!(m.s(4), Some(0x4030_c000_0000_0000));
    assert_eq!(m.s(4), Some(f(0.75 * 281474976710656.0)));
    // "0.4 x 2**0 (octal)" to "0.4 x 2**3 (octal)"
    assert_eq!(m.s(5), Some(0x4000_8000_0000_0000));
    assert_eq!(m.s(5), Some(f(0.5)));
    assert_eq!(m.s(6), Some(f(1.0)));
    assert_eq!(m.s(7), Some(f(2.0)));
    assert_eq!(m.s(0), Some(f(4.0)));

    // "(Ak) = 1 if k = 0"; a positive (Ak) gives a positive number.
    let mut m = monitor(&[0o071100, 0o071210, 0o071320, 0o071423]);
    m.set_a(0, Some(77));
    m.set_a(3, Some(0x12_3456));
    steps(&mut m, 4);
    assert_eq!(m.s(1), Some(1));
    assert_eq!(m.s(2), Some(1));
    assert_eq!(m.s(3), Some(0o40060 << 48 | 1));
    assert_eq!(m.s(4), Some(0o40060 << 48 | 0x12_3456));
}

#[test]
fn transmits_072_to_077() {
    // Page 4-45: 073 enters (VM), 074 (Tjk); 075 enters (Si) into Tjk.
    // Page 4-46: 076 and 077 use the low six bits of (Ak) as the element
    // number; (Ak) = 1 if k = 0; (Sj) = 0 if j = 0.
    let mut m = monitor_cal("S1 VM; T77 S2; S3 T77; V4,A5 S2; S6 V4,A5; V4,A0 S0; S7 V4,A0");
    m.set_vm(Some(0xf0f0));
    m.set_s(2, Some(0x1234));
    m.set_a(5, Some(0o1077)); // element 77 octal
    m.set_s(0, Some(99));
    steps(&mut m, 5);
    assert_eq!(m.s(1), Some(0xf0f0));
    assert_eq!(m.t(0o77), Some(0x1234));
    assert_eq!(m.s(3), Some(0x1234));
    assert_eq!(m.v(4, 63), Some(0x1234));
    assert_eq!(m.s(6), Some(0x1234));
    steps(&mut m, 2);
    assert_eq!(m.v(4, 1), Some(0)); // element 1, value (S0 as j) = 0
    assert_eq!(m.s(7), Some(0));
    assert_eq!(m.v(4, 0), None); // untouched
}
