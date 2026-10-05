//! The X-MP manual's own numbers decide where the pyramid is cut.
//!
//! HR-0097B page 4-30: "The average value of this truncation is 9.25 x 2^-56 ... Nine carries
//! are injected at the 2^-56 position ... With compensation, the results range from one too
//! large to one too small in the 2^-48 bit position with approximately 99 percent of the
//! values having zero deviation from what would have been generated had a full 96-bit pyramid
//! been present."
//!
//! A pyramid cut after column 2^-56 (`MulModel::XMP_MANUAL`, as figure 4-10 draws it) matches
//! all three statements. The pyramid in cray-sim's `Mult` (`MulModel::CRAY_SIM`, one more
//! column) matches none, although both reproduce every product vector in `xmp_ref.vec`.

use std::collections::BTreeMap;

use cray1_fp::vectors::SplitMix64;
use cray1_fp::{fmul_model, pack, unpack, MulKind, MulModel, EXP_BIAS};

struct Stats {
    mean_truncated_carries: f64,
    deviation_share: BTreeMap<i64, f64>,
}

fn measure(model: &MulModel, n: u32) -> Stats {
    let mut rng = SplitMix64::new(0x0097_0430);
    let mut carries = 0u64;
    let mut deviations: BTreeMap<i64, u32> = BTreeMap::new();
    for _ in 0..n {
        let a = rng.norm_coef();
        let b = rng.norm_coef();
        let exact = u128::from(a) * u128::from(b); // units of 2^-96
        let formed = u128::from(model.partial_product_sum(a, b)) << (96 - model.last_column);
        carries += ((exact - formed) >> 40) as u64; // whole units of 2^-56 that were dropped

        let r = fmul_model(
            pack(false, EXP_BIAS, a),
            pack(false, EXP_BIAS, b),
            MulKind::Full,
            model,
        );
        let got = unpack(r.value);
        let got_value = u128::from(got.coef) << (if got.exp == EXP_BIAS { 48 } else { 47 });
        let (ulp, truncated) = if exact >> 95 != 0 {
            (1u128 << 48, exact >> 48 << 48)
        } else {
            (1u128 << 47, exact >> 47 << 47)
        };
        let dev = (got_value as i128 - truncated as i128) / ulp as i128;
        *deviations.entry(dev as i64).or_default() += 1;
    }
    Stats {
        mean_truncated_carries: carries as f64 / f64::from(n),
        deviation_share: deviations
            .into_iter()
            .map(|(k, v)| (k, f64::from(v) / f64::from(n)))
            .collect(),
    }
}

#[test]
fn manual_pyramid_matches_the_manuals_statistics() {
    let s = measure(&MulModel::XMP_MANUAL, 400_000);
    println!(
        "cut after 2^-56: mean truncated carries {:.3}, deviations {:?}",
        s.mean_truncated_carries, s.deviation_share
    );
    assert!((s.mean_truncated_carries - 9.25).abs() < 0.05);
    assert_eq!(
        s.deviation_share.keys().copied().collect::<Vec<_>>(),
        [-1, 0, 1]
    );
    assert!((s.deviation_share[&0] - 0.99).abs() < 0.005);
}

#[test]
fn cray_sim_pyramid_does_not() {
    let s = measure(&MulModel::CRAY_SIM, 400_000);
    println!(
        "cut after 2^-57: mean truncated carries {:.3}, deviations {:?}",
        s.mean_truncated_carries, s.deviation_share
    );
    assert!((s.mean_truncated_carries - 4.25).abs() < 0.05);
    // Never one too small, and one too large more than twice as often as the manual allows.
    assert_eq!(
        s.deviation_share.keys().copied().collect::<Vec<_>>(),
        [0, 1]
    );
    assert!((s.deviation_share[&0] - 0.975).abs() < 0.005);
}

#[test]
fn the_two_pyramids_disagree_on_a_few_percent_of_products() {
    let mut rng = SplitMix64::new(0x0097_0431);
    let n = 400_000u32;
    let mut differ = [0u32; 4];
    let kinds = [
        MulKind::Full,
        MulKind::Rounded,
        MulKind::TwoMinus,
        MulKind::HalfRounded,
    ];
    for _ in 0..n {
        let a = pack(false, EXP_BIAS, rng.norm_coef());
        let b = pack(false, EXP_BIAS + 1, rng.norm_coef());
        for (i, kind) in kinds.into_iter().enumerate() {
            let manual = fmul_model(a, b, kind, &MulModel::XMP_MANUAL);
            let sim = fmul_model(a, b, kind, &MulModel::CRAY_SIM);
            differ[i] += u32::from(manual != sim);
        }
    }
    let share: Vec<f64> = differ
        .iter()
        .map(|&d| f64::from(d) / f64::from(n))
        .collect();
    println!(
        "XMP_MANUAL vs CRAY_SIM differ: full {:.4}, rounded {:.4}, 2-minus {:.4}, half {:.4}",
        share[0], share[1], share[2], share[3]
    );
    for s in &share[..2] {
        assert!((0.015..0.045).contains(s), "{s}");
    }
    // cray-sim also guesses the complement step of 067: its result is lower by one or two
    // units most of the time (see `TwoMinus`).
    assert!((0.85..0.95).contains(&share[2]), "{}", share[2]);
    // Half precision also differs in where the round bits sit.
    assert!((0.2..0.35).contains(&share[3]), "{}", share[3]);
}
