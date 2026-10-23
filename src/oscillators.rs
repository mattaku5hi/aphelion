//! Oscillators shared by the signal rules: RSI(14, Wilder), MACD(12, 26, 9) histogram,
//! ATR(14), the simple RSI divergence, the RSI pivot divergence, RSI MA and Supertrend.

use crate::{
    indicators::{PIV, PIV_RANGE, atr, ema, pivots, supertrend_line, wilder},
    zones::Tf,
};

/// Oscillator series of one timeframe and the per-bar signal flags derived from them.
pub struct Aux {
    /// RSI(14, Wilder) of the close; NaN until seeded.
    pub rsi: Vec<f64>,
    /// MACD(12, 26, 9) histogram.
    pub hist: Vec<f64>,
    /// ATR(14).
    pub atr14: Vec<f64>,
    /// Simple divergence against the extreme of bars `i-30..=i-3` (bearish / bullish).
    pub div_s: Vec<bool>,
    pub div_l: Vec<bool>,
    /// SMA(14) of the RSI ("RSI MA").
    pub rsi_ma: Vec<f64>,
    /// At the confirmation bar (pivot + 5): the pivot bar of a regular bearish / bullish divergence.
    pub piv_s: Vec<Option<usize>>,
    pub piv_l: Vec<Option<usize>>,
    /// Supertrend(10, 3, hl2) direction: 1 up, -1 down, 0 unknown.
    pub st_dir: Vec<i8>,
}

impl Aux {
    /// Oscillators of the closes (and highs/lows for divergences) of `tf`.
    pub fn new(tf: &Tf) -> Self {
        let n = tf.c.len();
        let (mut up, mut dn) = (vec![0.0; n], vec![0.0; n]);
        for i in 1..n {
            let d = tf.c[i] - tf.c[i - 1];
            up[i] = d.max(0.0);
            dn[i] = (-d).max(0.0);
        }
        let (au, ad) = (wilder(&up, 14, 1), wilder(&dn, 14, 1));
        let rsi: Vec<f64> = (0..n)
            .map(|i| {
                if au[i].is_nan() {
                    f64::NAN
                } else if ad[i] == 0.0 {
                    100.0
                } else {
                    100.0 - 100.0 / (1.0 + au[i] / ad[i])
                }
            })
            .collect();
        let (e12, e26) = (ema(&tf.c, 12), ema(&tf.c, 26));
        let macd: Vec<f64> = (0..n).map(|i| e12[i] - e26[i]).collect();
        let sig = ema(&macd, 9);
        let hist: Vec<f64> = (0..n).map(|i| macd[i] - sig[i]).collect();
        let (mut div_s, mut div_l) = (vec![false; n], vec![false; n]);
        for i in 40..n {
            if rsi[i].is_nan() {
                continue;
            }
            let (mut js, mut jl) = (i - 30, i - 30);
            for j in i - 30..=i - 3 {
                if tf.h[j] > tf.h[js] {
                    js = j;
                }
                if tf.l[j] < tf.l[jl] {
                    jl = j;
                }
            }
            div_s[i] = tf.h[i] > tf.h[js] && rsi[js] > 60.0 && rsi[i] < rsi[js] - 2.0;
            div_l[i] = tf.l[i] < tf.l[jl] && rsi[jl] < 40.0 && rsi[i] > rsi[jl] + 2.0;
        }
        let mut rsi_ma = vec![f64::NAN; n];
        for i in 0..n {
            if i >= 13 && rsi[i + 1 - 14..=i].iter().all(|v| !v.is_nan()) {
                rsi_ma[i] = rsi[i + 1 - 14..=i].iter().sum::<f64>() / 14.0;
            }
        }
        let (ph, pl) = (pivots(&rsi, true), pivots(&rsi, false));
        let (mut piv_s, mut piv_l) = (vec![None; n], vec![None; n]);
        for (flags, high, out) in [(&ph, true, &mut piv_s), (&pl, false, &mut piv_l)] {
            let mut last: Option<usize> = None;
            for t in (0..n).filter(|t| flags[*t]) {
                let ok = last.is_some_and(|p| {
                    PIV_RANGE.contains(&(t - p))
                        && if high {
                            tf.h[t] > tf.h[p] && rsi[t] < rsi[p]
                        } else {
                            tf.l[t] < tf.l[p] && rsi[t] > rsi[p]
                        }
                });
                if ok {
                    out[t + PIV] = Some(t);
                }
                last = Some(t);
            }
        }
        let st_dir = supertrend_line(&tf.h, &tf.l, &tf.c, 10, 3.0).0;
        Self {
            rsi,
            hist,
            atr14: atr(tf, 14),
            div_s,
            div_l,
            rsi_ma,
            piv_s,
            piv_l,
            st_dir,
        }
    }
    /// RSI beyond 75 / 25.
    pub fn rsi_flag(&self, i: usize, short: bool) -> bool {
        if short {
            self.rsi[i] > 75.0
        } else {
            self.rsi[i] < 25.0
        }
    }
    /// MACD histogram turning against the move on its side of zero.
    pub fn macd_flag(&self, i: usize, short: bool) -> bool {
        if i < 2 {
            return false;
        }
        let (a, b, c) = (self.hist[i], self.hist[i - 1], self.hist[i - 2]);
        if short {
            b > 0.0 && a < b && b >= c
        } else {
            b < 0.0 && a > b && b <= c
        }
    }
    /// Simple divergence of the side.
    pub fn div_flag(&self, i: usize, short: bool) -> bool {
        if short { self.div_s[i] } else { self.div_l[i] }
    }
    /// RSI beyond 70 / 30.
    pub fn zone_flag(&self, i: usize, short: bool) -> bool {
        if short {
            self.rsi[i] > 70.0
        } else {
            self.rsi[i] < 30.0
        }
    }
    /// RSI crosses back through its SMA (down for shorts, up for longs).
    pub fn xma_flag(&self, i: usize, short: bool) -> bool {
        if i == 0 || self.rsi_ma[i].is_nan() || self.rsi_ma[i - 1].is_nan() {
            return false;
        }
        let (r0, r1, m0, m1) = (
            self.rsi[i - 1],
            self.rsi[i],
            self.rsi_ma[i - 1],
            self.rsi_ma[i],
        );
        if short {
            r0 >= m0 && r1 < m1
        } else {
            r0 <= m0 && r1 > m1
        }
    }
    /// Supertrend flips against the extension (to down for shorts, to up for longs).
    pub fn st_flip_flag(&self, i: usize, short: bool) -> bool {
        if i == 0 || self.st_dir[i - 1] == 0 {
            return false;
        }
        let want = if short { -1 } else { 1 };
        self.st_dir[i] == want && self.st_dir[i - 1] == -want
    }
}

#[cfg(test)]
#[path = "oscillators_tests.rs"]
mod tests;
