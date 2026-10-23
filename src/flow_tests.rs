use super::*;

#[test]
fn taker_delta_and_flow_exhaust() {
    assert_eq!(taker_delta(100.0, 75.0), 0.5);
    assert!(taker_delta(0.0, 0.0).is_nan());
    // Long: the 4h flow sold into the drop, the last hour sold less.
    assert!(flow_exhaust(-0.05, -0.2, false));
    assert!(!flow_exhaust(-0.3, -0.2, false));
    assert!(!flow_exhaust(-0.05, 0.2, false));
    assert!(flow_exhaust(0.05, 0.2, true));
    assert!(!flow_exhaust(f64::NAN, -0.2, false));
}
