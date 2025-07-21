use nyx_mix::AdaptiveCoverGenerator;

#[test]
fn adaptive_cover_low_power_scales_lambda() {
    let mut gen = AdaptiveCoverGenerator::new(10.0, 0.5);
    let baseline = gen.current_lambda();
    gen.set_low_power(true);
    // cause internal lambda recalculation
    gen.next_delay();
    assert!(gen.current_lambda() < baseline * 0.5, "lambda not reduced sufficiently");
} 