//! Taker-flow predicates.

/// Taker delta (buy - sell) / turnover from quote volume `q` and taker-buy quote volume `tb`.
pub fn taker_delta(q: f64, tb: f64) -> f64 {
    if q > 0.0 {
        (2.0 * tb - q) / q
    } else {
        f64::NAN
    }
}

/// `flow_exhaust`: the 4h flow went with the move (`d4`) and the last hour (`d1`) is weaker.
pub fn flow_exhaust(d1: f64, d4: f64, short: bool) -> bool {
    let m = if short { 1.0 } else { -1.0 };
    d1.is_finite() && d4.is_finite() && m * d4 > 0.0 && m * d1 < m * d4
}

#[cfg(test)]
#[path = "flow_tests.rs"]
mod tests;
