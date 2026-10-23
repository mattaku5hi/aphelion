use super::*;

/// Close path: 600 flat bars with a small wobble, a 20% rally in 40 bars, a fall back in 30.
fn closes(mirror: bool) -> Vec<f64> {
    let mut c = Vec::new();
    for i in 0..600 {
        c.push(100.0 * (1.0 + if i % 2 == 0 { 0.001 } else { -0.001 }));
    }
    for i in 1..=40 {
        c.push(100.0 * 1.2f64.powf(i as f64 / 40.0));
    }
    for i in 1..=30 {
        c.push(120.0 * (100.0f64 / 120.0).powf(i as f64 / 30.0));
    }
    c.extend(std::iter::repeat_n(100.0, 20));
    if mirror {
        c.iter().map(|x| 10_000.0 / x).collect()
    } else {
        c
    }
}

fn ohlc(c: &[f64]) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let o: Vec<f64> = (0..c.len())
        .map(|i| if i == 0 { c[0] } else { c[i - 1] })
        .collect();
    let h = (0..c.len()).map(|i| o[i].max(c[i]) * 1.001).collect();
    let l = (0..c.len()).map(|i| o[i].min(c[i]) * 0.999).collect();
    (o, h, l)
}

fn x_of(c: &[f64], short: bool) -> Vec<usize> {
    let (o, h, l) = ohlc(c);
    let (mean, r) = zones(&h, &l, c);
    let b = Bars {
        o: &o,
        h: &h,
        l: &l,
        c,
    };
    x_events(&b, &mean, &r, short, 400)
}

#[test]
fn stoch_is_high_at_new_high_and_low_at_new_low() {
    let c: Vec<f64> = (0..200).map(|i| 100.0 + i as f64).collect();
    let h: Vec<f64> = c.iter().map(|x| x + 0.5).collect();
    let l: Vec<f64> = c.iter().map(|x| x - 0.5).collect();
    let s = stoch_mix(&h, &l, &c);
    assert!(
        s[19].is_nan() && s[20].is_finite(),
        "needs the longest window"
    );
    assert!(s[199] > 95.0, "rising close sits near the top: {}", s[199]);
    let d: Vec<f64> = c.iter().rev().copied().collect();
    let hd: Vec<f64> = d.iter().map(|x| x + 0.5).collect();
    let ld: Vec<f64> = d.iter().map(|x| x - 0.5).collect();
    assert!(stoch_mix(&hd, &ld, &d)[199] < 5.0);
}

#[test]
fn gradation_converges_unclipped_and_mirrors() {
    let s = vec![100.0; 60];
    let sell = gradation(&s, true);
    let fixed = GRAD_K * (100.0 - GRAD_S0) / (1.0 - GRAD_A);
    assert!((sell[59] - fixed).abs() < 0.5, "{} vs {fixed}", sell[59]);
    // Buy side uses 100 - S: the history keeps it far below zero (not clipped).
    let buy = gradation(&s, false);
    let low = GRAD_K * (0.0 - GRAD_S0) / (1.0 - GRAD_A);
    assert!((buy[59] - low).abs() < 1.0 && buy[59] < 0.0, "{}", buy[59]);
    assert_eq!(gradation(&[f64::NAN; 5], true), vec![0.0; 5]);
}

#[test]
fn band_start_needs_open_beyond_inner_band_then_holds() {
    let g = [30.0, 30.0, 30.0, 10.0, 30.0];
    let mean = [100.0; 5];
    let r = [1.0; 5];
    // Open 2R away (inside the start gate): never starts.
    assert_eq!(band(&g, &[102.0; 5], &mean, &r, true), vec![false; 5]);
    // Starts at bar 1 (open 3.2R), stays on although the open is back inside, off at g <= 15
    // and does not restart on an open inside the gate.
    let o = [100.0, 103.2, 101.0, 101.0, 101.0];
    assert_eq!(
        band(&g, &o, &mean, &r, true),
        vec![false, true, true, false, false]
    );
    // Wrong side of the Mean for a LONG.
    assert_eq!(band(&g, &o, &mean, &r, false), vec![false; 5]);
}

#[test]
fn repeat_filter_blocks_near_in_time_and_price_only() {
    let c = [100.0, 100.2, 100.4, 102.0, 100.0, 100.0, 100.1, 100.2];
    let f = [true, true, true, true, false, false, true, true];
    // Bars 1 and 2 are blocked (<5 bars, <1%); bar 3 passes on a 2% move; bar 6 is 3 bars
    // after bar 3 but 2% away from it, so it passes; bar 7 is blocked again.
    let out = repeat_filter(&f, &c, 5, 0.01);
    assert_eq!(
        out,
        vec![true, false, false, true, false, false, true, false]
    );
    let late = repeat_filter(
        &[true, false, false, false, false, true],
        &[1.0; 6],
        5,
        0.01,
    );
    assert_eq!(
        late,
        vec![true, false, false, false, false, true],
        "5 bars is enough"
    );
}

#[test]
fn short_x_fires_once_after_the_rally_turns() {
    let c = closes(false);
    let x = x_of(&c, true);
    assert_eq!(x.len(), 1, "{x:?}");
    // The peak is bar 639; the band ends once the fall started, never during the rally.
    assert!(x[0] > 639 && x[0] < 670, "{}", x[0]);
    assert!(x_of(&c, false).is_empty(), "a rally gives no LONG X");
}

#[test]
fn long_x_mirrors_the_short_one() {
    let long = x_of(&closes(true), false);
    assert_eq!(long.len(), 1, "{long:?}");
    let short = x_of(&closes(false), true);
    assert!(long[0].abs_diff(short[0]) <= 3, "{long:?} {short:?}");
    assert!(x_of(&closes(true), true).is_empty());
}

#[test]
fn no_x_when_the_move_stays_inside_the_small_zone() {
    // Same shape scaled to a 0.3% excursion: the close never reaches 5.587R beyond the Mean.
    let c: Vec<f64> = closes(false)
        .iter()
        .map(|v| 100.0 + (v - 100.0) * 0.015)
        .collect();
    let (o, h, l) = ohlc(&c);
    let (mean, r) = zones(&h, &l, &c);
    let x = x_of(&c, true);
    for i in &x {
        assert!((c[*i] - mean[*i]) / r[*i] >= ZONE_R, "bar {i}");
    }
    assert!(o.len() == c.len());
}

#[test]
fn x_uses_no_future_bars() {
    let c = closes(false);
    let full = x_of(&c, true);
    for cut in [650usize, 660, 680] {
        let expect: Vec<usize> = full.iter().copied().filter(|i| *i < cut).collect();
        assert_eq!(x_of(&c[..cut], true), expect, "cut {cut}");
    }
}

#[test]
fn combo_needs_two_confirmations_inside_the_lookback_without_looking_ahead() {
    let none = vec![false; 120];
    let (mut rsi, mut macd, mut div) = (none.clone(), none.clone(), none.clone());
    rsi[95] = true;
    div[91] = true; // exactly 10 bars back from 100 (91..=100)
    macd[101] = true; // after the X: not counted
    assert_eq!(combo_events(&[100], [&rsi, &macd, &div], 10), vec![100]);
    // Divergence one bar too old.
    div[91] = false;
    div[90] = true;
    assert!(combo_events(&[100], [&rsi, &macd, &div], 10).is_empty());
    // A single confirmation is not enough.
    assert!(combo_events(&[100], [&rsi, &none, &none], 10).is_empty());
}
