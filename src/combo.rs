//! COMBO proxies: close beyond a zone, an RSI signal and a MACD signal within a short
//! window, first signal per excursion: pre-registered confluence definitions on the zones.
//! `outer_rsi_macd` (index 1 of [`COMBOS`]) is the rule behind the proxy long signal
//! ([`crate::live::long_scan`]).

use crate::{oscillators::Aux, zones::Ind, zones::LADDER, zones::Tf};

/// Signal-timeframe bars before any signal (warm-up of the super smoothers).
pub const WARM_BARS: usize = 400;
/// Confluence window in signal-timeframe bars.
pub const WINDOW: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RsiMode {
    /// RSI beyond 70 / 30 on a bar of the window.
    Zone,
    /// RSI x MA cross back, or a confirmed regular divergence whose pivot was beyond the zone.
    Reversal,
}

#[derive(Clone, Copy, Debug)]
pub struct ComboDef {
    pub name: &'static str,
    /// Index into `LADDER` of the zone the close must be beyond.
    pub rung: usize,
    pub rsi: RsiMode,
    /// "X" variant: Supertrend(10, 3) flip against the move inside the window as well.
    pub x: bool,
}

/// Pre-registered proxy set (6 definitions). Every definition also needs a MACD(12, 26, 9)
/// signal against the move (histogram turning or MACD x signal cross).
pub const COMBOS: [ComboDef; 6] = [
    ComboDef {
        name: "inner_rsi_macd",
        rung: 0,
        rsi: RsiMode::Zone,
        x: false,
    },
    ComboDef {
        name: "outer_rsi_macd",
        rung: 2,
        rsi: RsiMode::Zone,
        x: false,
    },
    ComboDef {
        name: "inner_rev_macd",
        rung: 0,
        rsi: RsiMode::Reversal,
        x: false,
    },
    ComboDef {
        name: "outer_rev_macd",
        rung: 2,
        rsi: RsiMode::Reversal,
        x: false,
    },
    ComboDef {
        name: "inner_rsi_macd_x",
        rung: 0,
        rsi: RsiMode::Zone,
        x: true,
    },
    ComboDef {
        name: "outer_rsi_macd_x",
        rung: 2,
        rsi: RsiMode::Zone,
        x: true,
    },
];

pub fn windowed_any(flags: &[bool], w: usize) -> Vec<bool> {
    let mut out = vec![false; flags.len()];
    let mut last: Option<usize> = None;
    for (i, f) in flags.iter().enumerate() {
        if *f {
            last = Some(i);
        }
        out[i] = last.is_some_and(|l| i - l < w);
    }
    out
}

/// Per-bar flags of one COMBO proxy (closed bars only): `z` is the zone depth in band units
/// (NaN before the warm-up), the rest are the window-smoothed conditions.
pub struct ComboFlags {
    pub z: Vec<f64>,
    pub zw: Vec<bool>,
    pub rw: Vec<bool>,
    pub mw: Vec<bool>,
    pub xw: Vec<bool>,
}

impl ComboFlags {
    pub fn all(&self, i: usize) -> bool {
        self.z[i].is_finite() && self.zw[i] && self.rw[i] && self.mw[i] && self.xw[i]
    }
}

/// MACD(12, 26, 9) signal against the move: histogram turning or MACD x signal cross.
pub fn macd_signal(aux: &Aux, i: usize, short: bool) -> bool {
    aux.macd_flag(i, short)
        || (i >= 1
            && if short {
                aux.hist[i - 1] > 0.0 && aux.hist[i] <= 0.0
            } else {
                aux.hist[i - 1] < 0.0 && aux.hist[i] >= 0.0
            })
}

pub fn combo_flags(tf: &Tf, ind: &Ind, aux: &Aux, def: &ComboDef, short: bool) -> ComboFlags {
    let n = tf.c.len();
    let k = LADDER[def.rung];
    let z: Vec<f64> = (0..n)
        .map(|i| {
            let (b, d) = (ind.basis[i], ind.dev[i]);
            if i < WARM_BARS || b.is_nan() || d.is_nan() || d <= 0.0 || aux.rsi[i].is_nan() {
                f64::NAN
            } else if short {
                (tf.c[i] - b) / d
            } else {
                (b - tf.c[i]) / d
            }
        })
        .collect();
    let zf: Vec<bool> = z.iter().map(|v| *v >= k).collect();
    let piv = if short { &aux.piv_s } else { &aux.piv_l };
    let div_ok = |i: usize| piv[i].is_some_and(|t| zf[t]);
    let zext: Vec<bool> = (0..n).map(|i| zf[i] || div_ok(i)).collect();
    let rsi: Vec<bool> = (0..n)
        .map(|i| match def.rsi {
            RsiMode::Zone => aux.zone_flag(i, short),
            RsiMode::Reversal => aux.xma_flag(i, short) || div_ok(i),
        })
        .collect();
    let macd: Vec<bool> = (0..n).map(|i| macd_signal(aux, i, short)).collect();
    let x: Vec<bool> = (0..n)
        .map(|i| !def.x || aux.st_flip_flag(i, short))
        .collect();
    ComboFlags {
        z,
        zw: windowed_any(&zext, WINDOW),
        rw: windowed_any(&rsi, WINDOW),
        mw: windowed_any(&macd, WINDOW),
        xw: windowed_any(&x, WINDOW),
    }
}

/// One step of the "first signal per excursion" arming: returns (signal, armed after).
pub fn combo_step(armed: bool, f: &ComboFlags, i: usize) -> (bool, bool) {
    let armed = armed || !f.zw[i];
    if armed && f.all(i) {
        (true, false)
    } else {
        (false, armed)
    }
}

/// Signal bars of one COMBO proxy: the first bar of an excursion on which the zone,
/// RSI and MACD (and Supertrend for X) conditions all held within the last `WINDOW` bars.
/// Only closed bars are used; a new signal needs the zone condition to lapse first.
pub fn combo_events(tf: &Tf, ind: &Ind, aux: &Aux, def: &ComboDef, short: bool) -> Vec<usize> {
    let n = tf.c.len();
    let flags = combo_flags(tf, ind, aux, def, short);
    let mut out = Vec::new();
    let mut armed = true;
    for i in WARM_BARS..n {
        let (hit, next) = combo_step(armed, &flags, i);
        armed = next;
        if hit {
            out.push(i);
        }
    }
    out
}

#[cfg(test)]
#[path = "combo_tests.rs"]
mod tests;
