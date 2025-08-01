// flow_controller_tests.rs - Flow controller tests
#[cfg(test)]
mod tests {
    use super::super::flow_controller::*;

    #[test]
    fn test_flow_controller_creation() {
        let controller = FlowController::new(1000);
        assert_eq!(controller.get_window_size(), 1000);
    }

    #[test] 
    fn test_congestion_controller_creation() {
        let controller = CongestionController::new();
        assert!(controller.get_state() == CongestionState::SlowStart);
    }

    #[test]
    fn test_window_updates() {
        let mut controller = FlowController::new(1000);
        controller.update_window(500);
        assert_eq!(controller.get_window_size(), 500);
    }
}
