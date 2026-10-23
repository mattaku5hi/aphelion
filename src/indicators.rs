//! Indicator helpers on closed bars: EMA, Wilder RMA, true range, ATR, rolling mean/std,
//! the Ehlers super smoother, Supertrend and fractal pivots. Pure functions; the value at
//! bar `i` uses bars `..=i` only (pivots are known `PIV` bars after the pivot).

use crate::zones::Tf;

/// Exponential moving average seeded with the first value (`a = 2 / (n + 1)`).
pub fn ema(x: &[f64], n: usize) -> Vec<f64> {
    let a = 2.0 / (n as f64 + 1.0);
    let mut out = Vec::with_capacity(x.len());
    let mut prev = f64::NAN;
    for &v in x {
        prev = if prev.is_nan() {
            v
        } else {
            prev + a * (v - prev)
        };
        out.push(prev);
    }
    out
}

/// Wilder RMA of `x[start..]`, seeded with the SMA of its first `n` values; NaN before.
pub fn wilder(x: &[f64], n: usize, start: usize) -> Vec<f64> {
    // x[start..] valid; seeded by SMA of first n valid values.
    let mut out = vec![f64::NAN; x.len()];
    if x.len() < start + n {
        return out;
    }
    let seed = x[start..start + n].iter().sum::<f64>() / n as f64;
    out[start + n - 1] = seed;
    for i in start + n..x.len() {
        out[i] = (out[i - 1] * (n as f64 - 1.0) + x[i]) / n as f64;
    }
    out
}

/// True range; the first bar uses high - low.
pub fn true_range(tf: &Tf) -> Vec<f64> {
    (0..tf.c.len())
        .map(|i| {
            if i == 0 {
                tf.h[0] - tf.l[0]
            } else {
                (tf.h[i] - tf.l[i])
                    .max((tf.h[i] - tf.c[i - 1]).abs())
                    .max((tf.l[i] - tf.c[i - 1]).abs())
            }
        })
        .collect()
}

/// ATR: Wilder RMA of the true range (from bar 1).
pub fn atr(tf: &Tf, n: usize) -> Vec<f64> {
    wilder(&true_range(tf), n, 1)
}

/// Rolling mean and population standard deviation over `n` bars (prefix sums on
/// series scaled by its mean); NaN before the window is full.
pub fn rolling_mean_std(c: &[f64], n: usize) -> (Vec<f64>, Vec<f64>) {
    let len = c.len();
    let scale = (c.iter().sum::<f64>() / len.max(1) as f64).max(1e-300);
    let (mut p1, mut p2) = (vec![0.0; len + 1], vec![0.0; len + 1]);
    for i in 0..len {
        let x = c[i] / scale;
        p1[i + 1] = p1[i] + x;
        p2[i + 1] = p2[i] + x * x;
    }
    let (mut mean, mut std) = (vec![f64::NAN; len], vec![f64::NAN; len]);
    for i in n.saturating_sub(1)..len {
        if i + 1 < n {
            continue;
        }
        let m = (p1[i + 1] - p1[i + 1 - n]) / n as f64;
        let v = ((p2[i + 1] - p2[i + 1 - n]) / n as f64 - m * m).max(0.0);
        mean[i] = m * scale;
        std[i] = v.sqrt() * scale;
    }
    (mean, std)
}

/// One step of the Ehlers 2-pole super smoother (coefficients with sqrt(2)).
pub fn ss_step(y1: f64, y2: f64, x: f64, period: f64) -> f64 {
    let w = std::f64::consts::SQRT_2 * std::f64::consts::PI / period;
    let a = (-w).exp();
    let c2 = 2.0 * a * w.cos();
    let c3 = -a * a;
    (1.0 - c2 - c3) * x + c2 * y1 + c3 * y2
}

/// Super smoother seeded with the first value (the warm-up of 3 months makes the seed irrelevant).
pub fn supersmoother(x: &[f64], period: f64) -> Vec<f64> {
    let mut y = Vec::with_capacity(x.len());
    for (i, &v) in x.iter().enumerate() {
        y.push(if i < 2 {
            x[0]
        } else {
            ss_step(y[i - 1], y[i - 2], v, period)
        });
    }
    y
}

/// Pivot half-width (bars on each side) and the allowed distance between two pivots of
/// a divergence.
pub const PIV: usize = 5;
pub const PIV_RANGE: std::ops::RangeInclusive<usize> = 5..=60;

/// Supertrend as in Pine (`ta.supertrend`): hl2 +- factor * RMA(true range), ratcheting
/// bands. Returns the direction (1 up, -1 down) and the line value.
pub fn supertrend_line(
    tf_h: &[f64],
    tf_l: &[f64],
    tf_c: &[f64],
    period: usize,
    factor: f64,
) -> (Vec<i8>, Vec<f64>) {
    let n = tf_c.len();
    let tr: Vec<f64> = (0..n)
        .map(|i| {
            if i == 0 {
                tf_h[0] - tf_l[0]
            } else {
                (tf_h[i] - tf_l[i])
                    .max((tf_h[i] - tf_c[i - 1]).abs())
                    .max((tf_l[i] - tf_c[i - 1]).abs())
            }
        })
        .collect();
    let a = wilder(&tr, period, 0);
    let (mut dir, mut line) = (vec![0i8; n], vec![f64::NAN; n]);
    let (mut up, mut lo) = (f64::NAN, f64::NAN);
    for i in 0..n {
        if a[i].is_nan() {
            continue;
        }
        let mid = (tf_h[i] + tf_l[i]) / 2.0;
        let (bu, bl) = (mid + factor * a[i], mid - factor * a[i]);
        if up.is_nan() {
            (up, lo) = (bu, bl);
            dir[i] = 1;
            line[i] = bl;
            continue;
        }
        let (pu, pl) = (up, lo);
        lo = if bl > pl || tf_c[i - 1] < pl { bl } else { pl };
        up = if bu < pu || tf_c[i - 1] > pu { bu } else { pu };
        let prev_up_line = dir[i - 1] == -1;
        dir[i] = if prev_up_line {
            if tf_c[i] > up { 1 } else { -1 }
        } else if tf_c[i] < lo {
            -1
        } else {
            1
        };
        line[i] = if dir[i] == 1 { lo } else { up };
    }
    (dir, line)
}

/// Strict fractal pivots (`PIV` bars on each side, unique extreme of the window).
pub fn pivots(x: &[f64], high: bool) -> Vec<bool> {
    let n = x.len();
    let mut out = vec![false; n];
    for t in PIV..n.saturating_sub(PIV) {
        let w = &x[t - PIV..=t + PIV];
        if w.iter().any(|v| v.is_nan()) {
            continue;
        }
        let best = if high {
            w.iter().copied().fold(f64::MIN, f64::max)
        } else {
            w.iter().copied().fold(f64::MAX, f64::min)
        };
        out[t] = x[t] == best && w.iter().filter(|v| **v == best).count() == 1;
    }
    out
}

#[cfg(test)]
#[path = "indicators_tests.rs"]
mod tests;
