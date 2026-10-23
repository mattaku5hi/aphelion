use super::*;
use crate::zones::done_map;

fn tf_from(closes: &[f64]) -> Tf {
    let n = closes.len();
    let (mut h, mut l) = (vec![], vec![]);
    for (i, c) in closes.iter().enumerate() {
        let prev = if i == 0 { *c } else { closes[i - 1] };
        h.push(c.max(prev) * 1.001);
        l.push(c.min(prev) * 0.999);
    }
    let entry: Vec<usize> = (1..=n).collect();
    Tf {
        h,
        l,
        c: closes.to_vec(),
        done: done_map(&entry, n),
        entry,
    }
}

#[test]
fn bearish_rsi_divergence_is_confirmed_five_bars_after_the_pivot() {
    // Two rallies: the second makes a higher high on a much weaker push.
    let mut c: Vec<f64> = (0..30)
        .map(|i| 100.0 + if i % 2 == 0 { 0.3 } else { -0.3 })
        .collect();
    let mut p = 100.0;
    for s in [3.0, 4.0, 5.0, 4.0, 3.0, 2.0, -3.0, -4.0, -3.0, -2.0, -1.0] {
        p += s;
        c.push(p);
    }
    for s in [-1.0, 0.5, -0.5, 0.5, -0.5, 0.5, -0.5, 0.5, -0.5, 0.5] {
        p += s;
        c.push(p);
    }
    for _ in 0..18 {
        p += 1.5;
        c.push(p);
        p -= 0.5;
        c.push(p);
    }
    for _ in 0..8 {
        p -= 1.0;
        c.push(p);
    }
    let aux = Aux::new(&tf_from(&c));
    let hits: Vec<(usize, usize)> = aux
        .piv_s
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.map(|t| (i, t)))
        .collect();
    assert!(!hits.is_empty(), "no divergence found");
    for (i, t) in &hits {
        assert_eq!(i - t, 5);
        assert!(aux.rsi[*t] < 100.0);
    }
    assert!(aux.piv_l.iter().all(Option::is_none));
}

#[test]
fn rsi_extremes_and_macd_turns() {
    // 40 bars of an accelerating rally, then 40 bars down.
    let mut c: Vec<f64> = (0..40).map(|i| 100.0 + 0.05 * f64::from(i * i)).collect();
    c.extend((1..=40).map(|i| 178.0 - 2.0 * f64::from(i)));
    let aux = Aux::new(&tf_from(&c));
    assert!(aux.rsi[13].is_nan() && !aux.rsi[14].is_nan());
    assert_eq!(aux.rsi[39], 100.0);
    assert!(aux.zone_flag(39, true) && aux.rsi_flag(39, true));
    assert!(aux.zone_flag(79, false));
    // The histogram turns down right after the top: a short MACD signal near bar 40.
    let turn = (35..50).find(|i| aux.macd_flag(*i, true)).expect("turn");
    assert!((40..=42).contains(&turn), "turn at {turn}");
    assert!(!(0..80).any(|i| aux.macd_flag(i, false) && i < 40));
}
