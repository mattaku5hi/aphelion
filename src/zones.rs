//! Zone geometry: the Mean line, the band unit R and the zone ladder.
//!
//! Mean = SuperSmoother(200) of hlc3, R = SuperSmoother(200) of the true range; zones sit
//! at `Mean +- rung * R` for the rungs of [`LADDER`]. The SMA/EMA bases and the
//! standard-deviation / ATR units are the classic band families kept for comparison.

use crate::indicators::{atr, ema, rolling_mean_std, supersmoother, true_range};

/// Basis (centre line) of a band family.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BasisKind {
    Sma,
    Ema,
    /// Ehlers 2-pole super smoother (period 200) of hlc3: the Mean line.
    Ss,
}
/// Band unit of a band family.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DevKind {
    Std,
    Atr,
    /// Super smoother (period 200) of the true range: the band unit R.
    Range,
}

/// Super smoother period of the Mean line and of R.
pub const SS_PERIOD: f64 = 200.0;
/// Zone ladder in band units (SS(200) of the true range): inner band, small zone,
/// outer band, deep zone, then two extrapolated rungs used only for stops.
pub const LADDER: [f64; 6] = [
    std::f64::consts::PI,
    OUTER_K - 2.0,
    OUTER_K,
    OUTER_K + 2.0,
    OUTER_K + 4.0,
    OUTER_K + 6.0,
];
/// Central ("outer") zone in R: 2.415*pi ~ 7.587.
pub const OUTER_K: f64 = 2.415 * std::f64::consts::PI;
/// Zone names of the first four rungs, used in reports.
pub const ZONE_NAMES: [&str; 4] = ["inner", "small", "outer", "deep"];

/// Closed bars of one timeframe (high, low, close) with their mapping onto the base
/// bars they were built from (identity for a timeframe used on its own).
pub struct Tf {
    pub h: Vec<f64>,
    pub l: Vec<f64>,
    pub c: Vec<f64>,
    /// 1h index of the first bar after this bar closes.
    pub entry: Vec<usize>,
    /// Per base index j (len n+1): last bar of this timeframe with `entry <= j`, or -1.
    pub done: Vec<i32>,
}

/// Per base index (len `n1h + 1`): the last bar with `entry <= j`, or -1.
pub fn done_map(entry: &[usize], n1h: usize) -> Vec<i32> {
    let mut done = vec![-1i32; n1h + 1];
    let mut m = -1i32;
    for (j, d) in done.iter_mut().enumerate() {
        while ((m + 1) as usize) < entry.len() && entry[(m + 1) as usize] <= j {
            m += 1;
        }
        *d = m;
    }
    done
}

/// Basis (Mean) and band unit (R) of every bar.
pub struct Ind {
    pub basis: Vec<f64>,
    pub dev: Vec<f64>,
}

/// Basis and band unit of `tf`; `n` is the SMA/EMA/std/ATR length (ignored by the
/// super smoother, which always uses `SS_PERIOD`).
pub fn make_ind(tf: &Tf, basis: BasisKind, n: usize, dev: DevKind) -> Ind {
    let basis = match basis {
        BasisKind::Sma => rolling_mean_std(&tf.c, n).0,
        BasisKind::Ema => ema(&tf.c, n),
        BasisKind::Ss => {
            let hlc3: Vec<f64> = (0..tf.c.len())
                .map(|i| (tf.h[i] + tf.l[i] + tf.c[i]) / 3.0)
                .collect();
            supersmoother(&hlc3, SS_PERIOD)
        }
    };
    let dev = match dev {
        DevKind::Std => rolling_mean_std(&tf.c, n).1,
        DevKind::Atr => atr(tf, n),
        DevKind::Range => supersmoother(&true_range(tf), SS_PERIOD),
    };
    Ind { basis, dev }
}

#[cfg(test)]
#[path = "zones_tests.rs"]
mod tests;
