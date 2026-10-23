use super::*;

fn flat(n: usize, px: f64) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    (vec![px * 1.0005; n], vec![px * 0.9995; n], vec![px; n])
}

#[test]
fn smc_short_needs_sweep_then_choch() {
    let n = 200;
    let (mut h, l, mut c) = flat(n, 100.0);
    // Sweep: a spike above the 4h range, then a close below the swing low before it.
    let x = 150;
    h[x] = 103.0;
    c[x] = 102.0;
    for v in c.iter_mut().take(156).skip(x + 1) {
        *v = 101.0;
    }
    c[156] = 99.0;
    let s = Hlc {
        h: &h,
        l: &l,
        c: &c,
    };
    assert!(!smc_ok(&s, 152, true));
    assert!(smc_ok(&s, 156, true));
    assert!(!smc_ok(&s, 156, false));
    assert_eq!(smc_confirm(&s, 150, true), Some(156));
    assert_eq!(smc_entry(&s, 150, true), Some(157));
    // Out of the waiting window.
    assert_eq!(smc_confirm(&s, 156 - SMC_WAIT, true), None);
    assert!(!smc_ok(&s, n, true));
}

#[test]
fn smc_long_mirrors_the_short() {
    let n = 200;
    let (h, mut l, mut c) = flat(n, 100.0);
    let x = 150;
    l[x] = 97.0;
    c[x] = 98.0;
    for v in c.iter_mut().take(156).skip(x + 1) {
        *v = 99.0;
    }
    c[156] = 101.0;
    let s = Hlc {
        h: &h,
        l: &l,
        c: &c,
    };
    assert!(smc_ok(&s, 156, false));
    assert!(!smc_ok(&s, 156, true));
    // No entry bar after the last bar.
    assert_eq!(
        smc_entry(
            &Hlc {
                h: &h[..157],
                l: &l[..157],
                c: &c[..157]
            },
            150,
            false
        ),
        None
    );
}
