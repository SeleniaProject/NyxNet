#![cfg(feature = "telemetry")]
use nyx_telemetry::inc_request_total;

#[test]
fn telemetry_request_counter_increments() {
    // Gather before
    let before_val = prometheus::gather()
        .iter()
        .find(|mf| mf.get_name() == "nyx_exporter_requests_total")
        .and_then(|mf| mf.get_metric().get(0).map(|m| m.get_counter().get_value()))
        .unwrap_or(0.0);
    inc_request_total();
    let after_val = prometheus::gather()
        .iter()
        .find(|mf| mf.get_name() == "nyx_exporter_requests_total")
        .and_then(|mf| mf.get_metric().get(0).map(|m| m.get_counter().get_value()))
        .unwrap();
    assert_eq!(after_val, before_val + 1.0);
} 