use nyx_mix::adaptive::AdaptiveCoverGenerator;
use nyx_core::mobile::MobilePowerState::*;

#[test]
fn low_power_lambda_scales() {
    let mut gen = AdaptiveCoverGenerator::new(10.0, 0.4);
    let base = gen.current_lambda();
    // Enter low power (screen off)
    gen.apply_power_state(ScreenOff);
    assert!(gen.current_lambda() < base * 0.2);
    // Back to foreground
    gen.apply_power_state(Foreground);
    assert!(gen.current_lambda() >= base);
} 