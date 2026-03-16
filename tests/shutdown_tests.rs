//! Tests for graceful shutdown with in-flight request draining

use rs3gw::{InFlightGuard, InFlightTracker};
use std::time::Duration;

#[tokio::test]
async fn test_in_flight_counter_increment_decrement() {
    let tracker = InFlightTracker::new();
    assert_eq!(tracker.active_count(), 0);
    tracker.track_start();
    assert_eq!(tracker.active_count(), 1);
    tracker.track_start();
    assert_eq!(tracker.active_count(), 2);
    tracker.track_end();
    assert_eq!(tracker.active_count(), 1);
    tracker.track_end();
    assert_eq!(tracker.active_count(), 0);
}

#[tokio::test]
async fn test_in_flight_guard_drop() {
    let tracker = InFlightTracker::new();
    {
        let _guard = InFlightGuard::new(&tracker);
        assert_eq!(tracker.active_count(), 1);
    }
    assert_eq!(tracker.active_count(), 0);
}

#[tokio::test]
async fn test_wait_drain_immediate_when_zero() {
    let tracker = InFlightTracker::new();
    // Should return immediately
    let start = std::time::Instant::now();
    tracker.wait_drain(Duration::from_secs(5)).await;
    assert!(start.elapsed() < Duration::from_millis(100));
}

#[tokio::test]
async fn test_wait_drain_with_timeout() {
    let tracker = InFlightTracker::new();
    tracker.track_start();

    // Spawn a task that will end the request after 100ms
    let tracker_clone = tracker.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        tracker_clone.track_end();
    });

    let start = std::time::Instant::now();
    tracker.wait_drain(Duration::from_secs(5)).await;
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(50));
    assert!(elapsed < Duration::from_secs(1));
    assert_eq!(tracker.active_count(), 0);
}
