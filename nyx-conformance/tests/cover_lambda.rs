use nyx_mix::CoverGenerator;

#[test]
fn cover_generator_lambda_one() {
    let gen = CoverGenerator::new(1.0);
    let samples = 2000;
    let mut acc = 0.0;
    for _ in 0..samples {
        acc += gen.next_delay().as_secs_f64();
    }
    let mean = acc / samples as f64;
    assert!((mean - 1.0).abs() < 0.2, "mean delay {} deviates", mean);
} 