#!/usr/bin/env python3
"""Parity check of the Aphelion Pine scripts (stdlib only).

Recomputes Mean/R/zones, the X band, X, COMBO and the Aphelion proxy long signal from the OHLC
columns of a TradingView CSV export with the same formulas as the Rust reference
(the `aphelion` crate: src/zones.rs make_ind, src/oscillators.rs Aux, src/x.rs,
src/combo.rs, src/smc.rs) and diffs them against the exported indicator plot columns.

Usage:
  python3 aphelion_parity.py EXPORT.csv                 # aphelion.pine export
  python3 aphelion_parity.py EXPORT.csv --mode proxy    # aphelion_proxy_long.pine export (setups)
  python3 aphelion_parity.py EXPORT.csv --mode proxy --m5 M5.csv   # + 1h entries (flow_exhaust off)
  python3 aphelion_parity.py EXPORT.csv --dump OUT.csv  # also write the recomputed series
  python3 aphelion_parity.py --selftest                 # synthetic checks mirroring the Rust tests

Exit code 0 when every compared column matches, 1 on a mismatch, 2 on bad input.
"""

import argparse
import csv
import math
import sys

NAN = float("nan")
PI = math.pi
OUTER_K = 2.415 * PI
# Zone ladder in R: inner band, small zone, outer band, deep zone, two extrapolated rungs.
LADDER = [PI, OUTER_K - 2.0, OUTER_K, OUTER_K + 2.0, OUTER_K + 4.0, OUTER_K + 6.0]


class Params:
    """Defaults equal to the Rust constants."""

    ss_period = 200.0
    warm = 400  # WARM_BARS
    stoch_len = 21  # STOCH_LENS = [21]
    grad_a = 0.75
    grad_k = 1.0
    grad_s0 = 76.0
    grad_on = 15.0
    start_r = LADDER[0]
    zone_r = LADDER[1]
    rep_bars = 5
    rep_pct = 0.01
    combo_lookback = 10
    combo_min = 2
    rsi_len = 14
    rsi_hi = 70.0  # zone_flag
    rsi_lo = 30.0
    macd_fast = 12
    macd_slow = 26
    macd_sig = 9
    piv = 5
    div_min = 5
    div_max = 60
    # Aphelion proxy outer_rsi_macd
    proxy_k = LADDER[2]
    proxy_window = 3
    # 5m sweep + CHoCH (src/smc.rs SMC_*)
    smc_look = 12
    smc_liq = 48
    smc_swing = 6
    smc_wait = 12


def isfin(x):
    return x is not None and math.isfinite(x)


# --------------------------------------------------------------------------- Rust ports


def supersmoother(x, period):
    """src/indicators.rs supersmoother/ss_step: seeded with x[0] on the first two bars."""
    w = math.sqrt(2.0) * math.pi / period
    a = math.exp(-w)
    c2 = 2.0 * a * math.cos(w)
    c3 = -a * a
    y = []
    for i, v in enumerate(x):
        if i < 2:
            y.append(x[0])
        else:
            y.append((1.0 - c2 - c3) * v + c2 * y[i - 1] + c3 * y[i - 2])
    return y


def true_range(h, l, c):
    out = []
    for i in range(len(c)):
        if i == 0:
            out.append(h[0] - l[0])
        else:
            out.append(max(max(h[i] - l[i], abs(h[i] - c[i - 1])), abs(l[i] - c[i - 1])))
    return out


def ema(x, n):
    """src/indicators.rs ema: seeded with the first value."""
    a = 2.0 / (n + 1.0)
    out, prev = [], NAN
    for v in x:
        prev = v if math.isnan(prev) else prev + a * (v - prev)
        out.append(prev)
    return out


def wilder(x, n, start):
    out = [NAN] * len(x)
    if len(x) < start + n:
        return out
    s = 0.0
    for v in x[start:start + n]:
        s += v
    out[start + n - 1] = s / n
    for i in range(start + n, len(x)):
        out[i] = (out[i - 1] * (n - 1.0) + x[i]) / n
    return out


def pivots(x, piv, high):
    n = len(x)
    out = [False] * n
    for t in range(piv, max(n - piv, 0)):
        w = x[t - piv:t + piv + 1]
        if any(math.isnan(v) for v in w):
            continue
        best = max(w) if high else min(w)
        out[t] = x[t] == best and sum(1 for v in w if v == best) == 1
    return out


def dist(x, mean, r, short):
    if not (isfin(mean) and isfin(r) and r > 0.0):
        return NAN
    d = (x - mean) / r
    return d if short else -d


def ge(x, k):
    return isfin(x) and x >= k


def repeat_filter(flags, c, bars, pct):
    out = [False] * len(flags)
    last = None
    for i, f in enumerate(flags):
        if not f:
            continue
        if last is None or i - last >= bars or abs(c[i] / c[last] - 1.0) >= pct:
            out[i] = True
            last = i
    return out


def windowed_any(flags, w):
    out, last = [False] * len(flags), None
    for i, f in enumerate(flags):
        if f:
            last = i
        out[i] = last is not None and i - last < w
    return out


def any_in_lookback(flags, lookback):
    """Per bar: some flag within the last `lookback` bars, the bar included."""
    return windowed_any(flags, lookback)


def compute(o, h, l, c, p=Params):
    n = len(c)
    hlc3 = [(h[i] + l[i] + c[i]) / 3.0 for i in range(n)]
    mean = supersmoother(hlc3, p.ss_period)
    r = supersmoother(true_range(h, l, c), p.ss_period)

    # Stochastic of the close in the log high/low range of the last stoch_len bars.
    lh, ll, lc = [math.log(v) for v in h], [math.log(v) for v in l], [math.log(v) for v in c]
    stoch = [NAN] * n
    for i in range(max(p.stoch_len - 1, 0), n):
        hh = max(lh[i + 1 - p.stoch_len:i + 1])
        lo = min(ll[i + 1 - p.stoch_len:i + 1])
        s = 0.0
        s += 100.0 * (lc[i] - lo) / (hh - lo) if hh > lo else 0.0
        stoch[i] = s / 1.0

    def gradation(short):
        g = [0.0] * n
        for i in range(1, n):
            if isfin(stoch[i]):
                v = stoch[i] if short else 100.0 - stoch[i]
                g[i] = p.grad_a * g[i - 1] + p.grad_k * (v - p.grad_s0)
        return g

    def band(g, short):
        on = [False] * n
        for i in range(n):
            prev = i > 0 and on[i - 1]
            on[i] = g[i] > p.grad_on and (prev or ge(dist(o[i], mean[i], r[i], short), p.start_r))
        return on

    # Aux: RSI (Wilder from bar 1), MACD (EMA seeded with the first value), RSI pivots.
    up, dn = [0.0] * n, [0.0] * n
    for i in range(1, n):
        d = c[i] - c[i - 1]
        up[i] = max(d, 0.0)
        dn[i] = max(-d, 0.0)
    au, ad = wilder(up, p.rsi_len, 1), wilder(dn, p.rsi_len, 1)
    rsi = []
    for i in range(n):
        if math.isnan(au[i]):
            rsi.append(NAN)
        elif ad[i] == 0.0:
            rsi.append(100.0)
        else:
            rsi.append(100.0 - 100.0 / (1.0 + au[i] / ad[i]))
    e1, e2 = ema(c, p.macd_fast), ema(c, p.macd_slow)
    macd = [e1[i] - e2[i] for i in range(n)]
    sig = ema(macd, p.macd_sig)
    hist = [macd[i] - sig[i] for i in range(n)]

    piv_s, piv_l = [None] * n, [None] * n
    for high, out in ((True, piv_s), (False, piv_l)):
        flags = pivots(rsi, p.piv, high)
        last = None
        for t in range(n):
            if not flags[t]:
                continue
            if last is not None and p.div_min <= t - last <= p.div_max:
                if (h[t] > h[last] and rsi[t] < rsi[last]) if high else (
                        l[t] < l[last] and rsi[t] > rsi[last]):
                    out[t + p.piv] = t
            last = t

    def macd_signal(i, short):
        turn = False
        if i >= 2:
            a, b, cc = hist[i], hist[i - 1], hist[i - 2]
            turn = (b > 0.0 and a < b and b >= cc) if short else (b < 0.0 and a > b and b <= cc)
        cross = i >= 1 and ((hist[i - 1] > 0.0 and hist[i] <= 0.0) if short else (
            hist[i - 1] < 0.0 and hist[i] >= 0.0))
        return turn or cross

    def zone_flag(i, short):
        return isfin(rsi[i]) and (rsi[i] > p.rsi_hi if short else rsi[i] < p.rsi_lo)

    res = {"mean": mean, "r": r, "stoch": stoch, "rsi": rsi, "hist": hist}
    res["z"] = [dist(c[i], mean[i], r[i], True) for i in range(n)]
    for short, side in ((True, "sell"), (False, "buy")):
        g = gradation(short)
        on = band(g, short)
        cand = [i >= max(p.warm, 1) and on[i - 1] and not on[i]
                and ge(dist(c[i], mean[i], r[i], short), p.zone_r) for i in range(n)]
        x = repeat_filter(cand, c, p.rep_bars, p.rep_pct)
        rsi_f = repeat_filter([zone_flag(i, short) for i in range(n)], c, p.rep_bars, p.rep_pct)
        macd_f = repeat_filter([macd_signal(i, short) for i in range(n)], c, p.rep_bars, p.rep_pct)
        piv = piv_s if short else piv_l
        div = [piv[i] is not None for i in range(n)]
        conf = [any_in_lookback(f, p.combo_lookback) for f in (rsi_f, macd_f, div)]
        combo = [x[i] and sum(1 for f in conf if f[i]) >= p.combo_min for i in range(n)]
        res["g_" + side], res["band_" + side] = g, on
        res["x_" + side], res["combo_" + side] = x, combo
        res["rsi_" + side], res["macd_" + side], res["div_" + side] = rsi_f, macd_f, div

    # Aphelion proxy outer_rsi_macd, LONG side (src/combo.rs combo_flags/combo_step).
    zl = []
    for i in range(n):
        b, d = mean[i], r[i]
        if i < p.warm or not isfin(b) or not isfin(d) or d <= 0.0 or math.isnan(rsi[i]):
            zl.append(NAN)
        else:
            zl.append((b - c[i]) / d)
    zf = [ge(v, p.proxy_k) for v in zl]
    zext = [zf[i] or (piv_l[i] is not None and zf[piv_l[i]]) for i in range(n)]
    zw = windowed_any(zext, p.proxy_window)
    rw = windowed_any([zone_flag(i, False) for i in range(n)], p.proxy_window)
    mw = windowed_any([macd_signal(i, False) for i in range(n)], p.proxy_window)
    pl = [False] * n
    armed = True
    for i in range(p.warm, n):
        armed = armed or not zw[i]
        if armed and isfin(zl[i]) and zw[i] and rw[i] and mw[i]:
            pl[i] = True
            armed = False
    res["z_long"], res["proxy_long"] = zl, pl
    return res


def smc_ok(h, l, c, b, short, p=Params):
    """src/smc.rs smc_ok: sweep of the extreme + CHoCH, known at the close of bar b."""
    if b < p.smc_look + p.smc_liq + p.smc_swing or b >= len(c):
        return False
    lo = b + 1 - p.smc_look
    x = lo
    for j in range(lo, b + 1):
        if (h[j] > h[x]) if short else (l[j] < l[x]):
            x = j
    if x >= b or x < p.smc_liq + p.smc_swing:
        return False
    if short:
        liq = max(h[x - p.smc_liq:x])
        swing = min(l[x - p.smc_swing:x])
        return h[x] > liq and c[b] < swing
    liq = min(l[x - p.smc_liq:x])
    swing = max(h[x - p.smc_swing:x])
    return l[x] < liq and c[b] > swing


def smc_confirm(h, l, c, e, short, p=Params):
    """First confirming bar in [e - 1, e + smc_wait); e = first 5m bar after the signal bar."""
    for b in range(max(e - 1, 0), e + p.smc_wait):
        if smc_ok(h, l, c, b, short, p):
            return b
    return None


# --------------------------------------------------------------------------- CSV parity

# Exported plot title -> (series key, kind). Kinds: "f" float, "b" 0/1 flag, "zone" (k, sign).
X_COLUMNS = {
    "Mean": ("mean", "f"),
    "R": ("r", "f"),
    "Z": ("z", "f"),
    "G sell": ("g_sell", "f"),
    "G buy": ("g_buy", "f"),
    "Band sell": ("band_sell", "b"),
    "Band buy": ("band_buy", "b"),
    "X sell": ("x_sell", "b"),
    "X buy": ("x_buy", "b"),
    "COMBO sell": ("combo_sell", "b"),
    "COMBO buy": ("combo_buy", "b"),
}
ZONE_COLUMNS = {
    "Inner +": LADDER[0], "Inner -": -LADDER[0],
    "Small +": LADDER[1], "Small -": -LADDER[1],
    "Outer +": LADDER[2], "Outer -": -LADDER[2],
    "Deep +": LADDER[3], "Deep -": -LADDER[3],
    "Extra +": LADDER[4], "Extra -": -LADDER[4],
}
PROXY_COLUMNS = {
    "Mean": ("mean", "f"),
    "R": ("r", "f"),
    "Z long": ("z_long", "f"),
    "Aphelion long": ("proxy_long", "b"),
}
# Plots gated by the warm-up (na before it) in the Pine scripts.
WARM_GATED = {"Mean"} | set(ZONE_COLUMNS)


def num(s):
    s = (s or "").strip()
    if s == "" or s.lower() in ("nan", "na", "null"):
        return NAN
    return float(s)


def read_export(path):
    with open(path, newline="", encoding="utf-8-sig") as f:
        rows = list(csv.reader(f))
    if not rows:
        raise ValueError("empty CSV")
    head = [x.strip() for x in rows[0]]
    cols = {}
    for j, name in enumerate(head):
        cols.setdefault(name.lower(), []).append(j)
    data = {}
    for name, idx in cols.items():
        data[name] = [num(r[idx[0]]) if idx[0] < len(r) else NAN for r in rows[1:]]
    dup = sorted(k for k, v in cols.items() if len(v) > 1)
    return head, data, dup


def parse_times(path):
    """Bar open times in ms from the 'time' column (unix seconds/ms or ISO 8601)."""
    from datetime import datetime
    with open(path, newline="", encoding="utf-8-sig") as f:
        rows = list(csv.reader(f))
    head = [x.strip().lower() for x in rows[0]]
    if "time" not in head:
        raise ValueError(f"{path}: no 'time' column")
    j = head.index("time")
    out = []
    for r in rows[1:]:
        v = r[j].strip()
        try:
            x = float(v)
            out.append(int(x * 1000) if x < 1e11 else int(x))
        except ValueError:
            out.append(int(datetime.fromisoformat(v.replace("Z", "+00:00")).timestamp() * 1000))
    return out


def entry_parity(path, m5_path, setups, n, p=Params):
    """Pine 'Entry' column (1h variant, flow_exhaust off) vs smc_confirm on a 5m export."""
    head, data, _ = read_export(path)
    pine = data.get("entry")
    if pine is None:
        print("  Entry        not in export (skipped)")
        return True
    t1 = parse_times(path)[:n]
    tf = sorted(t1[i + 1] - t1[i] for i in range(len(t1) - 1))[len(t1) // 2]
    _, m5, _ = read_export(m5_path)
    t5 = parse_times(m5_path)
    h5, l5, c5 = m5["high"], m5["low"], m5["close"]
    step = sorted(t5[i + 1] - t5[i] for i in range(len(t5) - 1))[len(t5) // 2]
    index5 = {t: i for i, t in enumerate(t5)}
    want = [False] * n
    covered = [False] * n
    for i in (k for k in range(n) if setups[k]):
        close_ms = t1[i] + tf
        e = index5.get(close_ms)
        if e is None or e < 1 or e + p.smc_wait > len(t5):
            continue
        for k in range(i, min(i + 3, n)):
            covered[k] = True
        b = smc_confirm(h5, l5, c5, e, False, p)
        if b is not None:
            row = next((k for k in range(i, n) if t1[k] <= t5[b] < t1[k] + tf), None)
            if row is not None:
                want[row] = True
    bad = [k for k in range(n) if covered[k] and (pine[k] >= 0.5) != want[k]]
    print(f"  Entry        {'OK ' if not bad else 'BAD'} python entries {sum(want)}, pine entries "
          f"{sum(1 for k in range(n) if pine[k] >= 0.5)}, mismatches {len(bad)}"
          + (f" (rows {bad[:10]})" if bad else "") + f"; 5m step {step // 1000}s")
    return not bad


def close_enough(a, b, rtol, atol):
    if math.isnan(a) and math.isnan(b):
        return True
    if math.isnan(a) or math.isnan(b):
        return False
    return abs(a - b) <= atol + rtol * max(abs(a), abs(b))


def parity(path, mode, skip, rtol, keep_last, dump, m5=None, p=Params):
    head, data, dup = read_export(path)
    for k in ("open", "high", "low", "close"):
        if k not in data:
            print(f"error: column '{k}' missing (header: {head})", file=sys.stderr)
            return 2
    if dup:
        print(f"warning: duplicate columns {dup}; the first one is used "
              "(export with only one Aphelion script on the chart)")
    o, h, l, c = data["open"], data["high"], data["low"], data["close"]
    if not keep_last and len(c) > 1:
        # The last exported row is usually the forming bar; Pine signals are closed-bar only.
        o, h, l, c = o[:-1], h[:-1], l[:-1], c[:-1]
    n = len(c)
    if any(math.isnan(v) or v <= 0 for v in o + h + l + c):
        print("error: non-positive or missing OHLC values", file=sys.stderr)
        return 2
    res = compute(o, h, l, c, p)
    if dump:
        write_dump(dump, o, h, l, c, res)

    bi = data.get("bar index")
    if bi and not math.isnan(bi[0]) and bi[0] != 0:
        print(f"warning: first exported bar has bar_index {bi[0]:.0f}, not 0: the export does "
              "not start at the first chart bar, so seeding and warm-up differ; use --skip "
              "(>= 1500 bars makes the SuperSmoother seed negligible)")

    spec = dict(X_COLUMNS if mode == "x" else PROXY_COLUMNS)
    ok_all, compared = True, 0
    for title, (key, kind) in spec.items():
        col = data.get(title.lower())
        if col is None:
            print(f"  {title:12s} not in export (skipped)")
            continue
        col = col[:n]
        ours = res[key]
        bad, worst, first_bad = 0, 0.0, None
        for i in range(skip, n):
            if title in WARM_GATED and i < p.warm:
                if not math.isnan(col[i]):
                    bad += 1
                    first_bad = first_bad if first_bad is not None else i
                continue
            if kind == "b":
                pine = (col[i] or 0.0) if not math.isnan(col[i]) else 0.0
                good = (pine >= 0.5) == bool(ours[i])
            else:
                good = close_enough(col[i], ours[i], rtol, 1e-12)
                if good and not math.isnan(col[i]) and not math.isnan(ours[i]):
                    worst = max(worst, abs(col[i] - ours[i]) / max(abs(ours[i]), 1e-300))
            if not good:
                bad += 1
                first_bad = first_bad if first_bad is not None else i
        compared += 1
        ok_all &= bad == 0
        extra = f", first at row {first_bad}" if bad else ""
        print(f"  {title:12s} {'OK ' if bad == 0 else 'BAD'} mismatches {bad}{extra}, "
              f"max rel diff {worst:.2e}")
    if mode == "x":
        for title, k in ZONE_COLUMNS.items():
            col = data.get(title.lower())
            if col is None or all(math.isnan(v) for v in col):
                continue
            col = col[:n]
            bad, drawn = 0, 0
            for i in range(skip, n):
                if math.isnan(col[i]):
                    # Not drawn on this bar (warm-up, hidden, or a deeper level away from price).
                    continue
                drawn += 1
                want = res["mean"][i] + k * res["r"][i] if i >= p.warm else NAN
                bad += not close_enough(col[i], want, rtol, 1e-12)
            compared += 1
            ok_all &= bad == 0
            print(f"  {title:12s} {'OK ' if bad == 0 else 'BAD'} mismatches {bad} (drawn on {drawn} bars)")
    if compared == 0:
        print("error: no indicator columns found", file=sys.stderr)
        return 2
    sides = ("sell", "buy") if mode == "x" else ()
    for s in sides:
        xs = [i for i in range(n) if res["x_" + s][i]]
        cs = [i for i in range(n) if res["combo_" + s][i]]
        print(f"  python: X {s} {len(xs)}, COMBO {s} {len(cs)}")
    if mode == "proxy":
        print(f"  python: Aphelion long {sum(res['proxy_long'])}")
        if m5:
            ok_all &= entry_parity(path, m5, res["proxy_long"], n, p)
    print("PARITY OK" if ok_all else "PARITY MISMATCH")
    return 0 if ok_all else 1


def write_dump(path, o, h, l, c, res):
    keys = ["mean", "r", "z", "g_sell", "g_buy", "band_sell", "band_buy", "x_sell", "x_buy",
            "combo_sell", "combo_buy", "rsi", "hist", "z_long", "proxy_long"]
    with open(path, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["bar", "open", "high", "low", "close"] + keys)
        for i in range(len(c)):
            row = [i, repr(o[i]), repr(h[i]), repr(l[i]), repr(c[i])]
            for k in keys:
                v = res[k][i]
                row.append(int(v) if isinstance(v, bool) else repr(v))
            w.writerow(row)


# --------------------------------------------------------------------------- self-test


def synthetic(mirror):
    """The path of src/x_tests.rs: flat, +20% rally in 40 bars, back in 30, flat."""
    c = [100.0 * (1.0 + (0.001 if i % 2 == 0 else -0.001)) for i in range(600)]
    c += [100.0 * 1.2 ** (i / 40.0) for i in range(1, 41)]
    c += [120.0 * (100.0 / 120.0) ** (i / 30.0) for i in range(1, 31)]
    c += [100.0] * 20
    if mirror:
        c = [10_000.0 / x for x in c]
    o = [c[0] if i == 0 else c[i - 1] for i in range(len(c))]
    h = [max(o[i], c[i]) * 1.001 for i in range(len(c))]
    l = [min(o[i], c[i]) * 0.999 for i in range(len(c))]
    return o, h, l, c


def selftest():
    fails = []

    def check(cond, msg):
        if not cond:
            fails.append(msg)

    res = compute(*synthetic(False))
    xs = [i for i, v in enumerate(res["x_sell"]) if v]
    check(len(xs) == 1 and 639 < xs[0] < 670, f"short X once after the peak: {xs}")
    check(not any(res["x_buy"]), "a rally gives no LONG X")
    mres = compute(*synthetic(True))
    xl = [i for i, v in enumerate(mres["x_buy"]) if v]
    check(len(xl) == 1 and xs and abs(xl[0] - xs[0]) <= 3, f"long X mirrors: {xl} vs {xs}")
    check(not any(mres["x_sell"]), "mirror gives no SHORT X")
    # Repeat filter (src/x_tests.rs).
    out = repeat_filter([True, True, True, True, False, False, True, True],
                        [100.0, 100.2, 100.4, 102.0, 100.0, 100.0, 100.1, 100.2], 5, 0.01)
    check(out == [True, False, False, True, False, False, True, False], f"repeat: {out}")
    late = repeat_filter([True, False, False, False, False, True], [1.0] * 6, 5, 0.01)
    check(late == [True, False, False, False, False, True], "5 bars is enough")
    # Gradation fixed point and NaN handling.
    s = [100.0] * 60
    fixed = Params.grad_k * (100.0 - Params.grad_s0) / (1.0 - Params.grad_a)
    g = [0.0] * 60
    for i in range(1, 60):
        g[i] = Params.grad_a * g[i - 1] + Params.grad_k * (s[i] - Params.grad_s0)
    check(abs(g[59] - fixed) < 0.5, "gradation converges")
    # No look-ahead: cutting the series does not change earlier X.
    full = xs
    for cut in (650, 660, 680):
        part = compute(*[v[:cut] for v in synthetic(False)])
        got = [i for i, v in enumerate(part["x_sell"]) if v]
        check(got == [i for i in full if i < cut], f"cut {cut}: {got}")
    # Warm-up: nothing before Params.warm.
    check(not any(res["x_sell"][:Params.warm]), "no X before warm-up")
    for m in fails:
        print("FAIL:", m)
    print("SELFTEST OK" if not fails else f"SELFTEST FAILED ({len(fails)})")
    return 0 if not fails else 1


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("csv", nargs="?", help="TradingView export (OHLC + indicator plots)")
    ap.add_argument("--mode", choices=("x", "proxy"), default="x",
                    help="x = aphelion.pine, proxy = aphelion_proxy_long.pine")
    ap.add_argument("--skip", type=int, default=0, help="ignore the first N rows")
    ap.add_argument("--rtol", type=float, default=1e-6, help="relative tolerance of floats")
    ap.add_argument("--keep-last", action="store_true",
                    help="keep the last row (dropped by default: usually the forming bar)")
    ap.add_argument("--dump", help="write the recomputed series to this CSV")
    ap.add_argument("--m5", help="proxy mode: 5m OHLC export to check the 1h entries "
                    "(set 'Require flow_exhaust' off in the script for this check)")
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args()
    if a.selftest:
        return selftest()
    if not a.csv:
        ap.error("CSV path required (or --selftest)")
    try:
        return parity(a.csv, a.mode, a.skip, a.rtol, a.keep_last, a.dump, a.m5)
    except (OSError, ValueError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
