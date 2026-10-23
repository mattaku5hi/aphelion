//! Live use on a rolling window of closed candles: moving Mean/R levels, the zone of the
//! last closed bar and the proxy long signal scan (`outer_rsi_macd`, first event per
//! excursion). Callers supply candles through [`Candle`]; nothing here reads a clock.

use crate::{
    combo::{COMBOS, WARM_BARS, WINDOW, combo_flags, combo_step, macd_signal},
    oscillators::Aux,
    smc::Hlc,
    zones::{BasisKind, DevKind, Tf, done_map, make_ind},
};

/// Bars of signal-timeframe history needed before a signal or a level is trusted.
pub const MIN_BARS: usize = WARM_BARS + 50;
/// The live proxy's COMBO definition (`outer_rsi_macd`), index into [`COMBOS`].
pub const PROXY_COMBO: usize = 1;

/// A closed candle: open time and OHLC.
pub trait Candle {
    fn open_time_ms(&self) -> i64;
    fn open(&self) -> f64;
    fn high(&self) -> f64;
    fn low(&self) -> f64;
    fn close(&self) -> f64;
}

/// Plain candle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bar {
    pub open_time_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

impl Candle for Bar {
    fn open_time_ms(&self) -> i64 {
        self.open_time_ms
    }
    fn open(&self) -> f64 {
        self.open
    }
    fn high(&self) -> f64 {
        self.high
    }
    fn low(&self) -> f64 {
        self.low
    }
    fn close(&self) -> f64 {
        self.close
    }
}

/// Closed bars of one timeframe used on their own (identity base mapping).
pub fn build_tf<B: Candle>(bars: &[B]) -> Tf {
    let n = bars.len();
    let entry: Vec<usize> = (1..=n).collect();
    Tf {
        h: bars.iter().map(Candle::high).collect(),
        l: bars.iter().map(Candle::low).collect(),
        c: bars.iter().map(Candle::close).collect(),
        done: done_map(&entry, n),
        entry,
    }
}

/// Owned high/low/close columns of candles, for the 5m SMC check.
#[derive(Clone, Debug, Default)]
pub struct Columns {
    pub h: Vec<f64>,
    pub l: Vec<f64>,
    pub c: Vec<f64>,
}

impl Columns {
    pub fn from_candles<B: Candle>(bars: &[B]) -> Self {
        Self {
            h: bars.iter().map(Candle::high).collect(),
            l: bars.iter().map(Candle::low).collect(),
            c: bars.iter().map(Candle::close).collect(),
        }
    }
    pub fn hlc(&self) -> Hlc<'_> {
        Hlc {
            h: &self.h,
            l: &self.l,
            c: &self.c,
        }
    }
}

/// Mean and R of every closed signal bar; `at(t)` is the last bar closed by time `t`
/// (no look-ahead). A non-finite value or R <= 0 gives None: the caller keeps its levels.
#[derive(Clone, Debug)]
pub struct Levels {
    width: i64,
    open: Vec<i64>,
    basis: Vec<f64>,
    dev: Vec<f64>,
}

impl Levels {
    /// Levels of contiguous closed bars of width `width_ms`; None below `MIN_BARS` bars.
    pub fn new<B: Candle>(bars: &[B], width_ms: i64) -> Option<Self> {
        if bars.len() < MIN_BARS {
            return None;
        }
        let ind = make_ind(&build_tf(bars), BasisKind::Ss, 200, DevKind::Range);
        Some(Self {
            width: width_ms,
            open: bars.iter().map(Candle::open_time_ms).collect(),
            basis: ind.basis,
            dev: ind.dev,
        })
    }
    /// (Mean, R) of the last bar closed at `t_ms`.
    pub fn at(&self, t_ms: i64) -> Option<(f64, f64)> {
        let i = self
            .open
            .partition_point(|o| o + self.width <= t_ms)
            .checked_sub(1)?;
        let (b, d) = (self.basis[i], self.dev[i]);
        (b.is_finite() && d.is_finite() && d > 0.0).then_some((b, d))
    }
}

/// Position of the last closed bar's close in the zones, in R above the Mean
/// (`(close - Mean) / R`, the mirror of the long zone's `(Mean - close) / R`). None when
/// the history is not warm (`MIN_BARS`), the last bar is not the latest closed bar of
/// width `width_ms` at `at_ms`, or the levels are not finite (R <= 0).
pub fn zone_above_mean<B: Candle>(bars: &[B], width_ms: i64, at_ms: i64) -> Option<f64> {
    let last = bars.last()?;
    if last.open_time_ms() != at_ms.div_euclid(width_ms) * width_ms - width_ms {
        return None;
    }
    let (mean, r) = Levels::new(bars, width_ms)?.at(at_ms)?;
    let z = (last.close() - mean) / r;
    z.is_finite().then_some(z)
}

/// Result of [`long_scan`] at the last bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LongScan {
    /// Arming state after the last bar.
    pub armed: bool,
    /// The last bar is a signal (first per excursion).
    pub hit: bool,
    /// Mean, R and the long zone depth `(Mean - close) / R` of the last bar.
    pub basis: f64,
    pub dev: f64,
    pub zone_r: f64,
    /// RSI of the last bar, and whether the RSI / MACD condition held in the last `WINDOW` bars.
    pub rsi: f64,
    pub rsi_flag: bool,
    pub macd_flag: bool,
}

/// Proxy long: steps the `outer_rsi_macd` LONG arming over bars `start..` from
/// `armed` (a fresh history starts at `WARM_BARS`, armed). Callers pass at least `MIN_BARS`
/// bars; panics with fewer than `WINDOW`.
pub fn long_scan<B: Candle>(bars: &[B], start: usize, armed: bool) -> LongScan {
    let n = bars.len();
    let tf = build_tf(bars);
    let ind = make_ind(&tf, BasisKind::Ss, 200, DevKind::Range);
    let aux = Aux::new(&tf);
    let flags = combo_flags(&tf, &ind, &aux, &COMBOS[PROXY_COMBO], false);
    let mut armed = armed;
    let mut hit_last = false;
    for i in start..n {
        let (hit, next) = combo_step(armed, &flags, i);
        armed = next;
        hit_last = hit && i == n - 1;
    }
    let from = n - WINDOW;
    LongScan {
        armed,
        hit: hit_last,
        basis: ind.basis[n - 1],
        dev: ind.dev[n - 1],
        zone_r: flags.z[n - 1],
        rsi: aux.rsi[n - 1],
        rsi_flag: (from..n).any(|j| aux.zone_flag(j, false)),
        macd_flag: (from..n).any(|j| macd_signal(&aux, j, false)),
    }
}

#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;
