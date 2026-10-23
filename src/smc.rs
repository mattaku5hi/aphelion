//! Lower-timeframe (5m) SMC confirmation: liquidity sweep + change of character (CHoCH).

/// Look-back of the extreme, liquidity range, swing range before the extreme, and how
/// long to wait for the confirmation (5m bars).
pub const SMC_LOOK: usize = 12;
pub const SMC_LIQ: usize = 48;
pub const SMC_SWING: usize = 6;
pub const SMC_WAIT: usize = 12;

/// Closed 5m bars of one instrument as column slices of equal length.
#[derive(Clone, Copy, Debug)]
pub struct Hlc<'a> {
    pub h: &'a [f64],
    pub l: &'a [f64],
    pub c: &'a [f64],
}

/// Liquidity sweep + CHoCH on 5m bars, known at the close of bar `b`. Short: the highest
/// high `x` of the last `SMC_LOOK` bars took out the high of the preceding `SMC_LIQ`
/// bars (sweep), and bar `b` closes below the lowest low of the `SMC_SWING` bars before
/// `x` (change of character back toward the mean). Long mirrors it.
pub fn smc_ok(s: &Hlc, b: usize, short: bool) -> bool {
    if b < SMC_LOOK + SMC_LIQ + SMC_SWING || b >= s.c.len() {
        return false;
    }
    let lo = b + 1 - SMC_LOOK;
    let mut x = lo;
    for j in lo..=b {
        let better = if short {
            s.h[j] > s.h[x]
        } else {
            s.l[j] < s.l[x]
        };
        if better {
            x = j;
        }
    }
    if x >= b || x < SMC_LIQ + SMC_SWING {
        return false;
    }
    if short {
        let liq = s.h[x - SMC_LIQ..x].iter().copied().fold(f64::MIN, f64::max);
        let swing = s.l[x - SMC_SWING..x]
            .iter()
            .copied()
            .fold(f64::MAX, f64::min);
        s.h[x] > liq && s.c[b] < swing
    } else {
        let liq = s.l[x - SMC_LIQ..x].iter().copied().fold(f64::MAX, f64::min);
        let swing = s.h[x - SMC_SWING..x]
            .iter()
            .copied()
            .fold(f64::MIN, f64::max);
        s.l[x] < liq && s.c[b] > swing
    }
}

/// 5m index to enter at after a signal bar closed: the open of the bar after the first
/// confirmation within `SMC_WAIT` bars (`e` is the first 5m bar after the signal bar).
pub fn smc_entry(s: &Hlc, e: usize, short: bool) -> Option<usize> {
    let n = s.c.len();
    smc_confirm(s, e, short).map(|b| b + 1).filter(|x| *x < n)
}

/// First confirming 5m bar `b` (closed) within `SMC_WAIT` bars of `e`; the entry is at `b + 1`.
pub fn smc_confirm(s: &Hlc, e: usize, short: bool) -> Option<usize> {
    (e.saturating_sub(1)..e + SMC_WAIT).find(|b| smc_ok(s, *b, short))
}

#[cfg(test)]
#[path = "smc_tests.rs"]
mod tests;
