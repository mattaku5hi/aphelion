use super::*;
use crate::combo::combo_events;
use crate::zones::Ind;

const H: i64 = 3_600_000;

fn h1_bars(n: usize, last_close: f64) -> Vec<Bar> {
    (0..n)
        .map(|i| {
            let close = if i == n - 1 { last_close } else { 100.0 };
            Bar {
                open_time_ms: i as i64 * H,
                open: 100.0,
                high: 101.0 + (i % 3) as f64 * 0.1,
                low: 99.0,
                close,
            }
        })
        .collect()
}

#[test]
fn zone_above_mean_mirrors_the_long_zone_and_needs_warm_fresh_history() {
    let n = MIN_BARS;
    let at = n as i64 * H;
    let up = zone_above_mean(&h1_bars(n, 110.0), H, at).unwrap();
    assert!(up > 3.0, "{up}");
    let down = zone_above_mean(&h1_bars(n, 90.0), H, at).unwrap();
    assert!(down < 0.0, "{down}");
    let bars = h1_bars(n, 110.0);
    let (mean, r) = Levels::new(&bars, H).unwrap().at(at).unwrap();
    assert!((up - (110.0 - mean) / r).abs() < 1e-12);
    assert!(zone_above_mean(&h1_bars(n - 1, 110.0), H, (n as i64 - 1) * H).is_none());
    assert!(zone_above_mean(&bars, H, at + H).is_none());
    assert!(zone_above_mean::<Bar>(&[], H, at).is_none());
}

#[test]
fn levels_use_only_closed_bars() {
    let bars = h1_bars(MIN_BARS, 110.0);
    let lv = Levels::new(&bars, H).unwrap();
    let Ind { basis, dev } = make_ind(&build_tf(&bars), BasisKind::Ss, 200, DevKind::Range);
    let last = MIN_BARS as i64 * H;
    assert_eq!(lv.at(last), Some((basis[MIN_BARS - 1], dev[MIN_BARS - 1])));
    // One millisecond earlier the last bar is still open.
    assert_eq!(
        lv.at(last - 1),
        Some((basis[MIN_BARS - 2], dev[MIN_BARS - 2]))
    );
    assert_eq!(lv.at(H - 1), None);
    assert!(Levels::new(&bars[1..], H).is_none());
}

fn crash_bars(n: usize) -> Vec<Bar> {
    let mut p = 100.0;
    (0..n)
        .map(|i| {
            let open = p;
            p *= 1.0 + if i % 160 < 25 { -0.018 } else { 0.0015 } + ((i * 7919) % 11) as f64 * 1e-4;
            Bar {
                open_time_ms: i as i64 * H,
                open,
                high: open.max(p) * 1.002,
                low: open.min(p) * 0.998,
                close: p,
            }
        })
        .collect()
}

#[test]
fn incremental_long_scan_matches_the_offline_combo_events() {
    let bars = crash_bars(1100);
    let tf = build_tf(&bars);
    let ind = make_ind(&tf, BasisKind::Ss, 200, DevKind::Range);
    let aux = Aux::new(&tf);
    let offline = combo_events(&tf, &ind, &aux, &COMBOS[PROXY_COMBO], false);
    assert!(!offline.is_empty());
    // Bar by bar, resuming the arming state as the live scanner does.
    let mut state: Option<(usize, bool)> = None;
    let mut live = vec![];
    for n in MIN_BARS..=bars.len() {
        let (start, armed) = state.map_or((WARM_BARS, true), |(last, a)| (last + 1, a));
        let s = long_scan(&bars[..n], start, armed);
        if s.hit {
            live.push(n - 1);
            assert!(s.zone_r.is_finite() && s.dev > 0.0);
        }
        state = Some((n - 1, s.armed));
    }
    let expect: Vec<usize> = offline.into_iter().filter(|i| *i >= MIN_BARS - 1).collect();
    assert_eq!(live, expect);
}

#[test]
fn columns_view_the_candles() {
    let bars = h1_bars(3, 105.0);
    let cols = Columns::from_candles(&bars);
    let v = cols.hlc();
    assert_eq!(v.c, &[100.0, 100.0, 105.0]);
    assert_eq!(v.l, &[99.0; 3]);
}
