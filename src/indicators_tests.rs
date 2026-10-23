use super::*;

fn tf_of(c: &[f64]) -> Tf {
    let n = c.len();
    let entry: Vec<usize> = (1..=n).collect();
    Tf {
        h: c.iter().map(|x| x + 1.0).collect(),
        l: c.iter().map(|x| x - 1.0).collect(),
        c: c.to_vec(),
        done: crate::zones::done_map(&entry, n),
        entry,
    }
}

#[test]
fn rolling_std_matches_naive() {
    let c: Vec<f64> = (0..60)
        .map(|i| 100.0 + (f64::from(i) * 0.7).sin() * 5.0)
        .collect();
    let (m, sd) = rolling_mean_std(&c, 20);
    let w = &c[40..60];
    let mean = w.iter().sum::<f64>() / 20.0;
    let var = w.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / 20.0;
    assert!((m[59] - mean).abs() < 1e-9);
    assert!((sd[59] - var.sqrt()).abs() < 1e-9);
    assert!(m[18].is_nan() && !m[19].is_nan());
}

#[test]
fn supertrend_flips_after_a_break_of_the_lower_band() {
    let mut c: Vec<f64> = (0..60).map(|i| 100.0 + f64::from(i)).collect();
    c.extend((0..30).map(|i| 160.0 - 4.0 * f64::from(i)));
    let h: Vec<f64> = c.iter().map(|x| x + 1.0).collect();
    let l: Vec<f64> = c.iter().map(|x| x - 1.0).collect();
    let (dir, line) = supertrend_line(&h, &l, &c, 10, 3.0);
    assert_eq!(dir[40], 1);
    assert!(line[40] < c[40]);
    assert_eq!(dir[89], -1);
    assert!(line[89] > c[89]);
    let flip = (61..90)
        .find(|i| dir[*i] == -1 && dir[*i - 1] == 1)
        .expect("flip");
    assert!(flip < 70, "flip at {flip}");
}

#[test]
fn super_smoother_is_seeded_steps_and_keeps_a_constant() {
    let x: Vec<f64> = (0..50).map(|i| 100.0 + f64::from(i % 7)).collect();
    let y = supersmoother(&x, 200.0);
    assert_eq!((y[0], y[1]), (x[0], x[0]));
    for i in 2..x.len() {
        assert_eq!(
            y[i].to_bits(),
            ss_step(y[i - 1], y[i - 2], x[i], 200.0).to_bits()
        );
    }
    // Unit DC gain: a constant input stays constant.
    let flat = supersmoother(&[42.0; 300], 200.0);
    assert!(flat.iter().all(|v| (v - 42.0).abs() < 1e-9));
}

#[test]
fn ema_wilder_and_true_range_basics() {
    let e = ema(&[1.0, 2.0, 3.0], 3);
    assert_eq!(e, vec![1.0, 1.5, 2.25]);
    let w = wilder(&[9.0, 1.0, 2.0, 3.0, 6.0], 3, 1);
    assert!(w[0].is_nan() && w[1].is_nan() && w[2].is_nan());
    assert_eq!(w[3], 2.0);
    assert_eq!(w[4], (2.0 * 2.0 + 6.0) / 3.0);
    assert!(wilder(&[1.0, 2.0], 3, 0).iter().all(|v| v.is_nan()));
    let tf = tf_of(&[10.0, 14.0, 9.0]);
    // First bar: high - low; then the gap to the previous close counts.
    assert_eq!(true_range(&tf), vec![2.0, 5.0, 6.0]);
    let a = atr(&tf, 2);
    assert!(a[1].is_nan());
    assert_eq!(a[2], 5.5);
}

#[test]
fn pivots_need_a_unique_extreme_with_five_bars_on_each_side() {
    let mut x = vec![1.0; 21];
    x[10] = 5.0;
    x[3] = -2.0;
    let hi = pivots(&x, true);
    assert_eq!(hi.iter().filter(|v| **v).count(), 1);
    assert!(hi[10]);
    // Too close to the start for a low pivot.
    assert!(pivots(&x, false).iter().all(|v| !v));
    x[12] = 5.0;
    assert!(!pivots(&x, true)[10], "a tie is not a pivot");
}
