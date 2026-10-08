//! The vector generator and the text format.

use std::collections::HashSet;

use cray_xmp_fp::vectors::{
    corner_operands, parse_line, read_vectors, write_vectors, FLAG_RANGE_ERROR,
};
use cray_xmp_fp::{read_vector_file, unpack, vectors, write_vector_file, Op, Vector, NORM_BIT};

fn exp(word: u64) -> u16 {
    unpack(word).exp
}

fn coef(word: u64) -> u64 {
    unpack(word).coef
}

#[test]
fn generator_is_deterministic_and_seeded() {
    for op in Op::ALL {
        let a: Vec<Vector> = vectors(op, 3000, 42).collect();
        let b: Vec<Vector> = vectors(op, 3000, 42).collect();
        let c: Vec<Vector> = vectors(op, 3000, 43).collect();
        assert_eq!(a.len(), 3000);
        assert_eq!(a, b, "{op:?}");
        let corners = corner_operands(op).len();
        assert!(corners < 3000, "{op:?} has {corners} corner cases");
        // Corner cases come first and do not depend on the seed; the rest does.
        assert_eq!(a[..corners], c[..corners]);
        assert_ne!(a[corners..], c[corners..]);
        // A shorter run is a prefix of a longer one.
        let short: Vec<Vector> = vectors(op, 100, 42).collect();
        assert_eq!(short[..], a[..100]);
    }
}

#[test]
fn every_vector_is_what_the_crate_computes() {
    for op in Op::ALL {
        for v in vectors(op, 2500, 7) {
            assert_eq!(v.op, op);
            assert_eq!(v.expected(), op.eval(v.a, v.b), "{v}");
            assert_eq!(v.flags & !FLAG_RANGE_ERROR, 0);
            if op == Op::Recip {
                assert_eq!(v.b, 0);
            }
        }
    }
}

#[test]
fn text_format_round_trips() {
    let v = Vector {
        op: Op::Mul2M,
        a: 0x4001_8000_0000_0000,
        b: 0xC000_FFFF_FFFF_FFFF,
        result: 0x0123_4567_89AB_CDEF,
        flags: 1,
    };
    let line = v.to_string();
    assert_eq!(
        line,
        "mul2m 4001800000000000 C000FFFFFFFFFFFF 0123456789ABCDEF 1"
    );
    assert_eq!(parse_line(&line), Ok(Some(v)));
    assert_eq!(parse_line("  # just a comment"), Ok(None));
    assert_eq!(parse_line(""), Ok(None));
    assert_eq!(
        parse_line("recip 1 0 2 0   # trailing comment"),
        Ok(Some(Vector {
            op: Op::Recip,
            a: 1,
            b: 0,
            result: 2,
            flags: 0
        }))
    );
    assert!(parse_line("div 1 2 3 0").is_err());
    assert!(parse_line("add 1 2 3").is_err());
    assert!(parse_line("add 1 2 xyz 0").is_err());
    for op in Op::ALL {
        assert_eq!(Op::from_name(op.name()), Some(op));
        let mut text = Vec::new();
        write_vectors(&mut text, op, 500, 3).unwrap();
        let parsed = read_vectors(&text[..]).unwrap();
        assert_eq!(parsed, vectors(op, 500, 3).collect::<Vec<_>>());
        // Every line is exactly five fields of fixed width.
        for line in String::from_utf8(text).unwrap().lines() {
            let fields: Vec<&str> = line.split(' ').collect();
            assert_eq!(fields.len(), 5);
            assert_eq!(
                [
                    fields[1].len(),
                    fields[2].len(),
                    fields[3].len(),
                    fields[4].len()
                ],
                [16, 16, 16, 1]
            );
        }
    }
}

#[test]
fn vector_file_round_trips() {
    let dir = std::env::temp_dir().join(format!("cray-xmp-fp-vectors-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("all.vec");
    let written = write_vector_file(&path, &Op::ALL, 300, 99).unwrap();
    assert_eq!(written, 7 * 300);
    let back = read_vector_file(&path).unwrap();
    let expect: Vec<Vector> = Op::ALL
        .into_iter()
        .flat_map(|op| vectors(op, 300, 99))
        .collect();
    assert_eq!(back, expect);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn add_corner_set_covers_the_listed_cases() {
    let corners = corner_operands(Op::Add);
    assert_eq!(corners, corner_operands(Op::Sub));
    let normal = |w: u64| coef(w) & NORM_BIT != 0;

    // Exponent differences between normalised operands.
    let diffs: HashSet<u16> = corners
        .iter()
        .filter(|&&(a, b)| normal(a) && normal(b))
        .map(|&(a, b)| exp(a).abs_diff(exp(b)))
        .collect();
    for d in [0u16, 1, 47, 48, 49, 63, 64, 65, 100] {
        assert!(diffs.contains(&d), "exponent difference {d}");
    }

    // Cancellation leaving 1..=48 leading zeros in the 48-bit difference (48 = zero).
    let mut zeros = HashSet::new();
    for &(a, b) in &corners {
        let (ua, ub) = (unpack(a), unpack(b));
        if ua.sign != ub.sign && normal(a) && normal(b) && ua.exp.abs_diff(ub.exp) <= 1 {
            let (big, small) = if ua.exp >= ub.exp { (ua, ub) } else { (ub, ua) };
            let diff = big.coef.abs_diff(small.coef >> (big.exp - small.exp));
            zeros.insert(if diff == 0 {
                48
            } else {
                diff.leading_zeros() - 16
            });
        }
    }
    for k in 1..=48 {
        assert!(zeros.contains(&k), "cancellation leaving {k} leading zeros");
    }

    // Carry out, zeros of both signs, unnormalised operands, edge exponents.
    assert!(corners
        .iter()
        .any(|&(a, b)| unpack(a).sign == unpack(b).sign
            && exp(a) == exp(b)
            && coef(a) + coef(b) >= 1 << 48));
    for z in [0u64, 1 << 63] {
        assert!(corners.iter().any(|&(a, b)| a == z && normal(b)));
        assert!(corners.iter().any(|&(a, b)| normal(a) && b == z));
    }
    assert!(corners
        .iter()
        .any(|&(a, b)| !normal(a) && coef(a) != 0 && b == 0));
    assert!(corners
        .iter()
        .any(|&(a, b)| !normal(a) && coef(a) != 0 && normal(b) && exp(a) > exp(b)));
    let exps: HashSet<u16> = corners
        .iter()
        .flat_map(|&(a, b)| [exp(a), exp(b)])
        .collect();
    for e in [0o17777u16, 0o20000, 0o57777, 0o60000] {
        assert!(exps.contains(&e), "exponent {e:o}");
    }

    // Outcomes: silent underflow from non-zero operands, range errors, in-range results.
    let results: Vec<Vector> = corners
        .iter()
        .map(|&(a, b)| Vector::compute(Op::Add, a, b))
        .collect();
    assert!(results.iter().any(|v| v.result == 0
        && v.flags == 0
        && coef(v.a) != 0
        && coef(v.b) != 0
        && exp(v.a) >= 0o20000
        && v.a != v.b ^ (1 << 63)));
    // a carry out of in-range operands reaches 060000 without the error (CRAY-1 rule)
    assert!(results.iter().any(|v| v.flags == 0
        && exp(v.result) == 0o60000
        && exp(v.a) < 0o60000
        && exp(v.b) < 0o60000));
    assert!(results
        .iter()
        .any(|v| v.flags == FLAG_RANGE_ERROR && exp(v.a) >= 0o60000));
    assert!(results
        .iter()
        .any(|v| exp(v.result) == 0o20000 && v.flags == 0));
}

#[test]
fn multiply_corner_set_covers_the_listed_cases() {
    for op in [Op::Mul, Op::MulH, Op::MulR, Op::Mul2M] {
        let corners = corner_operands(op);
        let set: HashSet<(u64, u64)> = corners.iter().copied().collect();

        // Coefficients near all ones and every single-bit coefficient.
        assert!(corners.iter().any(|&(a, b)| coef(a) == 0xFFFF_FFFF_FFFF
            && coef(b) == 0xFFFF_FFFF_FFFF
            && exp(a) != 0));
        assert!(corners.iter().any(|&(a, _)| coef(a) == 0xFFFF_FFFF_FFFE));
        for bit in 0..48 {
            assert!(
                corners
                    .iter()
                    .any(|&(a, b)| coef(a) == 1 << bit && coef(b) == 1 << bit),
                "single bit {bit}"
            );
        }
        // Both exponents zero, and exactly one.
        assert!(corners
            .iter()
            .any(|&(a, b)| exp(a) == 0 && exp(b) == 0 && coef(a) != 0 && coef(b) != 0));
        assert!(corners
            .iter()
            .any(|&(a, b)| exp(a) == 0 && exp(b) >= 0o60000));
        assert!(corners
            .iter()
            .any(|&(a, b)| exp(a) != 0 && exp(b) == 0 && coef(b) != 0));
        // Exponent sums on both sides of each range edge.
        let sums: HashSet<i32> = corners
            .iter()
            .filter(|&&(a, b)| exp(a) != 0 && exp(b) != 0)
            .map(|&(a, b)| i32::from(exp(a)) + i32::from(exp(b)) - 0o40000)
            .collect();
        for e in [0o17777, 0o20000, 0o20001, 0o57777, 0o60000, 0o60001] {
            assert!(sums.contains(&e), "exponent sum {e:o}");
        }
        // A x B and B x A are both present for asymmetric pairs.
        let swapped = corners
            .iter()
            .filter(|&&(a, b)| a != b && set.contains(&(b, a)))
            .count();
        assert!(swapped > 500, "{op:?}: {swapped}");

        // Outcomes: range error, silent underflow, and a result exponent of 017777.
        let results: Vec<Vector> = corners
            .iter()
            .map(|&(a, b)| Vector::compute(op, a, b))
            .collect();
        assert!(results.iter().any(|v| v.flags == FLAG_RANGE_ERROR));
        assert!(results
            .iter()
            .any(|v| v.result == 0 && v.flags == 0 && exp(v.a) != 0 && exp(v.b) != 0));
        assert!(results
            .iter()
            .any(|v| exp(v.result) == 0o17777 && v.flags == 0));
    }
}

#[test]
fn reciprocal_corner_set_covers_the_listed_cases() {
    let corners = corner_operands(Op::Recip);
    assert!(corners.iter().all(|&(_, b)| b == 0));
    let low_mask = (1u64 << 40) - 1;
    for index in 0..128u64 {
        let with_index: Vec<u64> = corners
            .iter()
            .map(|&(a, _)| coef(a))
            .filter(|c| c & NORM_BIT != 0 && (c >> 40) & 0x7F == index)
            .collect();
        assert!(
            with_index.iter().any(|c| c & low_mask == 0),
            "index {index} minimum"
        );
        assert!(
            with_index.iter().any(|c| c & low_mask == low_mask),
            "index {index} maximum"
        );
        assert!(
            with_index
                .iter()
                .any(|c| c & low_mask != 0 && c & low_mask != low_mask),
            "index {index} random"
        );
    }
    assert!(corners
        .iter()
        .any(|&(a, _)| coef(a) == NORM_BIT && exp(a) == 0o40001));
    assert!(corners
        .iter()
        .any(|&(a, _)| coef(a) & NORM_BIT == 0 && coef(a) != 0));
    assert!(corners.iter().any(|&(a, _)| a == 0));
    let exps: HashSet<u16> = corners.iter().map(|&(a, _)| exp(a)).collect();
    for e in [0o20001u16, 0o20002, 0o57777, 0o60000] {
        assert!(exps.contains(&e), "exponent {e:o}");
    }
}

#[test]
fn random_classes_include_uniform_and_aligned_normalised_operands() {
    for op in [Op::Add, Op::Sub] {
        let corners = corner_operands(op).len();
        let random: Vec<Vector> = vectors(op, corners + 40_000, 5).skip(corners).collect();
        // Class 1: normalised, in range, exponent difference 0..=50 with every value seen.
        let mut seen = HashSet::new();
        for v in random.iter().skip(1).step_by(4) {
            assert!(coef(v.a) & NORM_BIT != 0 && coef(v.b) & NORM_BIT != 0);
            let d = exp(v.a).abs_diff(exp(v.b));
            assert!(d <= 50);
            seen.insert(d);
        }
        assert_eq!(seen.len(), 51);
        // Class 0: uniform 64-bit patterns reach every exponent quarter and both signs.
        let quarters: HashSet<u16> = random.iter().step_by(4).map(|v| exp(v.a) >> 13).collect();
        assert_eq!(quarters.len(), 4);
        // The stream produces range errors, zero results and ordinary results.
        assert!(random.iter().any(|v| v.flags == FLAG_RANGE_ERROR));
        assert!(random.iter().any(|v| v.result == 0));
        assert!(
            random
                .iter()
                .filter(|v| v.flags == 0 && v.result != 0)
                .count()
                > 20_000
        );
    }
}
