//! X band, X labels and COMBO on X.
//!
//! Pure functions of the closed bars of one timeframe; nothing looks ahead (signal at bar
//! `i` uses bars `..=i`).
//!
//! * Zones: Mean = SuperSmoother(200) of hlc3, R = SuperSmoother(200) of the true range
//!   (as in [`crate::zones`]).
//! * Gradation per side: an UNCLIPPED EMA
//!   `g = a*g_prev + k*(S - S0)` (it may be negative, which is what keeps the history) with
//!   `S` = stochastic position of the close in the high/low range of the last 21 bars (log
//!   prices; the buy side uses `100 - S`).
//! * Band ("подсветка"): switches on only when `|open - Mean| >= pi*R` on the extended side
//!   and `g > GRAD_ON`; once on it stays on while `g > GRAD_ON`.
//! * X: the first bar on which the band turned off, if `|close - Mean| >= 5.587*R` on the
//!   extended side (above for SHORT, below for LONG) and the repeat filter (5 bars,
//!   1%) lets it through: a same-direction X is blocked until the price
//!   moved 1% from the previous X OR 5 bars passed.
//! * COMBO: X plus any two of {RSI signal, MACD signal, divergence} within the last
//!   `COMBO_LOOKBACK` bars (10). RSI and MACD use the COMBO proxies' signals ([`crate::combo::macd_signal`]);
//!   divergence is the RSI/price pivot divergence (`Aux::piv_*`).

use crate::{
    combo::macd_signal,
    oscillators::Aux,
    zones::{BasisKind, DevKind, Ind, LADDER, Tf, done_map, make_ind},
};

/// Unclipped EMA `g = a*g_prev + k*(S - S0)` with `S` from one stochastic
/// length (21).
pub const GRAD_A: f64 = 0.75;
pub const GRAD_K: f64 = 1.0;
pub const GRAD_S0: f64 = 76.0;
/// Stochastic lengths averaged into `S`.
pub const STOCH_LENS: [usize; 1] = [21];
/// The band is on while the gradation is above this.
pub const GRAD_ON: f64 = 15.0;
/// Start gate and X zone gate, in R: inner band (pi) and the small zone (5.587).
pub const START_R: f64 = LADDER[0];
pub const ZONE_R: f64 = LADDER[1];
/// Repeat filter: bars and fraction of price.
pub const REPEAT_BARS: usize = 5;
pub const REPEAT_PCT: f64 = 0.01;
/// COMBO confirmation lookback in bars of the signal timeframe.
pub const COMBO_LOOKBACK: usize = 10;

/// Closed bars of one timeframe.
pub struct Bars<'a> {
    pub o: &'a [f64],
    pub h: &'a [f64],
    pub l: &'a [f64],
    pub c: &'a [f64],
}

/// Mean line and band unit R of the given bars ([`crate::zones`]).
pub fn zones(h: &[f64], l: &[f64], c: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = c.len();
    let entry: Vec<usize> = (1..=n).collect();
    let tf = Tf {
        h: h.to_vec(),
        l: l.to_vec(),
        c: c.to_vec(),
        done: done_map(&entry, n),
        entry,
    };
    let Ind { basis, dev } = make_ind(&tf, BasisKind::Ss, 200, DevKind::Range);
    (basis, dev)
}

/// Sell-side stochastic score `S` (0..100): mean over `STOCH_LENS` of the position of the
/// close in the high/low range of the last `L` bars in log prices, including the current
/// bar. NaN until the longest window is filled.
pub fn stoch_mix(h: &[f64], l: &[f64], c: &[f64]) -> Vec<f64> {
    let n = c.len();
    let (lh, ll, lc): (Vec<f64>, Vec<f64>, Vec<f64>) = (
        h.iter().map(|x| x.ln()).collect(),
        l.iter().map(|x| x.ln()).collect(),
        c.iter().map(|x| x.ln()).collect(),
    );
    let longest = STOCH_LENS.iter().copied().max().unwrap_or(1);
    let mut out = vec![f64::NAN; n];
    for i in longest.saturating_sub(1)..n {
        let mut s = 0.0;
        for len in STOCH_LENS {
            let w = i + 1 - len..=i;
            let hh = lh[w.clone()].iter().copied().fold(f64::MIN, f64::max);
            let lo = ll[w].iter().copied().fold(f64::MAX, f64::min);
            s += if hh > lo {
                100.0 * (lc[i] - lo) / (hh - lo)
            } else {
                0.0
            };
        }
        out[i] = s / STOCH_LENS.len() as f64;
    }
    out
}

/// Unclipped gradation of one side from the sell-side score `s` (NaN counts as 0).
pub fn gradation(s: &[f64], short: bool) -> Vec<f64> {
    let mut g = vec![0.0; s.len()];
    for i in 1..s.len() {
        if s[i].is_finite() {
            let v = if short { s[i] } else { 100.0 - s[i] };
            g[i] = GRAD_A * g[i - 1] + GRAD_K * (v - GRAD_S0);
        }
    }
    g
}

/// Signed distance of `x` from the Mean in R, positive on the extended side.
fn dist(x: f64, mean: f64, r: f64, short: bool) -> f64 {
    if !(mean.is_finite() && r.is_finite() && r > 0.0) {
        return f64::NAN;
    }
    let d = (x - mean) / r;
    if short { d } else { -d }
}

/// Highlight band: on when `g > GRAD_ON`, and it may only start when the bar's open is at
/// least `START_R` away from the Mean on the extended side.
pub fn band(g: &[f64], o: &[f64], mean: &[f64], r: &[f64], short: bool) -> Vec<bool> {
    let mut on = vec![false; g.len()];
    for i in 0..g.len() {
        let prev = i > 0 && on[i - 1];
        on[i] = g[i] > GRAD_ON && (prev || dist(o[i], mean[i], r[i], short) >= START_R);
    }
    on
}

/// Same-direction repeat filter: keeps flags that are at least `bars` bars or `pct` of price
/// (close to close) away from the previously kept one.
pub fn repeat_filter(flags: &[bool], c: &[f64], bars: usize, pct: f64) -> Vec<bool> {
    let mut out = vec![false; flags.len()];
    let mut last: Option<usize> = None;
    for i in 0..flags.len() {
        if !flags[i] {
            continue;
        }
        let ok = last.is_none_or(|p| i - p >= bars || (c[i] / c[p] - 1.0).abs() >= pct);
        if ok {
            out[i] = true;
            last = Some(i);
        }
    }
    out
}

/// X bars (indices of the first bar whose band is off), from bar `warm` on.
pub fn x_events(b: &Bars, mean: &[f64], r: &[f64], short: bool, warm: usize) -> Vec<usize> {
    let g = gradation(&stoch_mix(b.h, b.l, b.c), short);
    let on = band(&g, b.o, mean, r, short);
    let cand: Vec<bool> = (0..on.len())
        .map(|i| {
            i >= warm.max(1) && on[i - 1] && !on[i] && dist(b.c[i], mean[i], r[i], short) >= ZONE_R
        })
        .collect();
    let kept = repeat_filter(&cand, b.c, REPEAT_BARS, REPEAT_PCT);
    (0..kept.len()).filter(|i| kept[*i]).collect()
}

/// COMBO bars: X bars with at least two of the three confirmation flags (RSI, MACD,
/// divergence) seen within the last `lookback` bars, the X bar included.
pub fn combo_events(x: &[usize], conf: [&[bool]; 3], lookback: usize) -> Vec<usize> {
    x.iter()
        .copied()
        .filter(|&i| {
            let lo = (i + 1).saturating_sub(lookback);
            conf.iter().filter(|f| f[lo..=i].iter().any(|v| *v)).count() >= 2
        })
        .collect()
}

/// X and COMBO bars of one side.
pub struct XSignals {
    pub x: Vec<usize>,
    pub combo: Vec<usize>,
}

/// Signals on a study timeframe: `open` are the bar opens, `ind` the Mean/R of the zones.
pub fn signals(tf: &Tf, open: &[f64], ind: &Ind, aux: &Aux, short: bool, warm: usize) -> XSignals {
    let n = tf.c.len();
    let bars = Bars {
        o: open,
        h: &tf.h,
        l: &tf.l,
        c: &tf.c,
    };
    let x = x_events(&bars, &ind.basis, &ind.dev, short, warm);
    let rsi: Vec<bool> = (0..n).map(|i| aux.zone_flag(i, short)).collect();
    let macd: Vec<bool> = (0..n).map(|i| macd_signal(aux, i, short)).collect();
    let piv = if short { &aux.piv_s } else { &aux.piv_l };
    let div: Vec<bool> = (0..n).map(|i| piv[i].is_some()).collect();
    let rsi = repeat_filter(&rsi, &tf.c, REPEAT_BARS, REPEAT_PCT);
    let macd = repeat_filter(&macd, &tf.c, REPEAT_BARS, REPEAT_PCT);
    let combo = combo_events(&x, [&rsi, &macd, &div], COMBO_LOOKBACK);
    XSignals { x, combo }
}

#[cfg(test)]
#[path = "x_tests.rs"]
mod tests;
