//! Aphelion: a mean-reversion indicator. Mean = SuperSmoother of hlc3, R = SuperSmoother of
//! the true range, zones at `Mean +- k*R`, the X band and labels, COMBO confirmations and
//! the proxy long signal.
//!
//! Pure, I/O-free computation on closed bars of one timeframe (nothing looks ahead):
//!
//! * [`zones`]: Mean (SuperSmoother(200) of hlc3), band unit R (SuperSmoother(200) of the
//!   true range) and the zone ladder;
//! * [`indicators`] and [`oscillators`]: EMA, Wilder, true range, ATR, Supertrend, pivots,
//!   RSI, MACD, divergences;
//! * [`x`]: the X highlight band, X labels, the repeat filter and COMBO on X;
//! * [`combo`]: the COMBO confluence proxies and their "first per excursion" arming;
//! * [`smc`]: the 5m liquidity sweep + CHoCH confirmation;
//! * [`flow`]: taker-flow delta and the `flow_exhaust` predicate;
//! * [`live`]: moving levels, zone of the last bar and the proxy long scan over candles.

pub mod combo;
pub mod flow;
pub mod indicators;
pub mod live;
pub mod oscillators;
pub mod smc;
pub mod x;
pub mod zones;

pub use live::{Bar, Candle};
pub use oscillators::Aux;
pub use zones::{BasisKind, DevKind, Ind, LADDER, Tf, ZONE_NAMES, done_map, make_ind};
