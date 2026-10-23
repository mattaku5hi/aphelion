use super::*;
use crate::zones::{BasisKind, DevKind, done_map, make_ind};

#[test]
fn windowed_any_spans_window() {
    let f = [false, true, false, false, false, false];
    assert_eq!(
        windowed_any(&f, 3),
        vec![false, true, true, true, false, false]
    );
}

fn flags(zw: &[bool], all: &[bool]) -> ComboFlags {
    let n = zw.len();
    ComboFlags {
        z: vec![1.0; n],
        zw: zw.to_vec(),
        rw: all.to_vec(),
        mw: vec![true; n],
        xw: vec![true; n],
    }
}

#[test]
fn step_fires_once_per_excursion_and_rearms_when_the_zone_lapses() {
    let zw = [true, true, true, false, true, true];
    let all = [false, true, true, false, true, true];
    let f = flags(&zw, &all);
    let mut armed = true;
    let mut hits = vec![];
    for i in 0..zw.len() {
        let (hit, next) = combo_step(armed, &f, i);
        armed = next;
        hits.push(hit);
    }
    assert_eq!(hits, vec![false, true, false, false, true, false]);
}

#[test]
fn events_are_the_arming_steps_from_the_warm_up() {
    let n = 900;
    let mut c = vec![];
    let mut p = 100.0;
    for i in 0..n {
        p *= 1.0 + if i % 150 < 20 { -0.02 } else { 0.002 } + ((i * 7919) % 13) as f64 * 1e-4;
        c.push(p);
    }
    let entry: Vec<usize> = (1..=n).collect();
    let tf = Tf {
        h: c.iter().map(|x| x * 1.003).collect(),
        l: c.iter().map(|x| x * 0.997).collect(),
        c,
        done: done_map(&entry, n),
        entry,
    };
    let ind = make_ind(&tf, BasisKind::Ss, 200, DevKind::Range);
    let aux = Aux::new(&tf);
    for def in &COMBOS {
        for short in [true, false] {
            let f = combo_flags(&tf, &ind, &aux, def, short);
            assert!(f.z[..WARM_BARS].iter().all(|z| z.is_nan()));
            let mut armed = true;
            let mut want = vec![];
            for i in WARM_BARS..n {
                let (hit, next) = combo_step(armed, &f, i);
                armed = next;
                if hit {
                    assert!(f.all(i));
                    want.push(i);
                }
            }
            assert_eq!(combo_events(&tf, &ind, &aux, def, short), want);
        }
    }
    let long = combo_events(&tf, &ind, &aux, &COMBOS[1], false);
    assert!(
        !long.is_empty(),
        "the crashes must trigger outer_rsi_macd long"
    );
}
