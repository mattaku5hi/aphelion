use super::*;

#[test]
fn ladder_rungs_and_done_map() {
    use std::f64::consts::PI;
    assert_eq!(LADDER[0], PI);
    assert_eq!(LADDER[2], 2.415 * PI);
    assert_eq!(LADDER[1], LADDER[2] - 2.0);
    assert_eq!(LADDER[3], LADDER[2] + 2.0);
    assert!((LADDER[1] - 5.587).abs() < 1e-3 && (LADDER[2] - 7.587).abs() < 1e-3);
    assert_eq!(done_map(&[4, 8], 9), vec![-1, -1, -1, -1, 0, 0, 0, 0, 1, 1]);
}

#[test]
fn super_smoother_zones_follow_hlc3_and_the_true_range() {
    let n = 600;
    let c: Vec<f64> = (0..n).map(|i| 100.0 + (i as f64 * 0.05).sin()).collect();
    let entry: Vec<usize> = (1..=n).collect();
    let tf = Tf {
        h: c.iter().map(|x| x + 0.5).collect(),
        l: c.iter().map(|x| x - 0.5).collect(),
        c: c.clone(),
        done: done_map(&entry, n),
        entry,
    };
    let ind = make_ind(&tf, BasisKind::Ss, 200, DevKind::Range);
    // hlc3 equals the close here; the Mean stays near 100, R near the 1.0 bar range.
    assert!(ind.basis.iter().all(|b| (b - 100.0).abs() < 1.5));
    assert!((ind.dev[n - 1] - 1.0).abs() < 0.2, "{}", ind.dev[n - 1]);
    let sma = make_ind(&tf, BasisKind::Sma, 50, DevKind::Std);
    assert!(sma.basis[48].is_nan() && !sma.basis[49].is_nan());
}
