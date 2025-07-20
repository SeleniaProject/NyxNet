use nyx_mix::CoverGenerator;

#[test]
fn cover_lambda5_mean() {
    let gen = CoverGenerator::new(5.0);
    let mut acc = 0.0;
    for _ in 0..1000 { acc += gen.next_delay().as_secs_f64(); }
    let mean = acc / 1000.0;
    assert!((mean - 0.2).abs() < 0.05);
} 