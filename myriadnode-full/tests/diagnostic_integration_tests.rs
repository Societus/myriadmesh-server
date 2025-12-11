use myriadnode::diagnostics::baseline::BaselineManager;
/**
 * Diagnostic Integration Tests
 *
 * These tests use real diagnostic snapshots from production/development runs
 * to verify Node initialization, Router integration, and lifecycle behavior.
 */
use myriadnode::diagnostics::test_data::{DiagnosticTestFixture, TestOutcome};

#[test]
fn test_load_diagnostic_fixtures() {
    // Load all diagnostic snapshots
    let fixture = DiagnosticTestFixture::load_all().expect("Should load diagnostic fixtures");

    // Generate report
    let report = fixture.generate_report();
    report.print();

    // Basic assertions
    println!("Loaded {} diagnostic snapshots", report.total_snapshots);
}

#[test]
fn test_no_initialization_failures_in_snapshots() {
    let fixture = DiagnosticTestFixture::load_all().expect("Should load diagnostic fixtures");

    let init_failures = fixture.initialization_failures();

    if !init_failures.is_empty() {
        println!("Found {} initialization failures:", init_failures.len());
        for snapshot in init_failures {
            println!(
                "  Session {}: {:?}",
                snapshot.session_id, snapshot.metadata.description
            );
            for checkpoint in &snapshot.init_checkpoints {
                if !checkpoint.success {
                    println!(
                        "    FAILED: {:?} - {:?}",
                        checkpoint.checkpoint, checkpoint.error
                    );
                }
            }
        }
        panic!("Initialization failures detected in diagnostic snapshots");
    }
}

#[test]
fn test_router_integration_health() {
    let fixture = DiagnosticTestFixture::load_all().expect("Should load diagnostic fixtures");

    let router_issues = fixture.router_integration_issues();

    if !router_issues.is_empty() {
        println!("Found {} router integration issues:", router_issues.len());
        for snapshot in router_issues {
            println!(
                "  Session {}: {:?}",
                snapshot.session_id, snapshot.metadata.description
            );
            if let Some(health) = &snapshot.router_health {
                println!(
                    "    Local delivery channel: {}",
                    health.has_local_delivery_channel
                );
                println!(
                    "    Confirmation callback: {}",
                    health.has_confirmation_callback
                );
                println!("    Message sender: {}", health.has_message_sender);
                println!("    Queue processor: {}", health.queue_processor_running);
            }
        }
        panic!("Router integration issues detected in diagnostic snapshots");
    }
}

#[test]
fn test_successful_runs_have_complete_lifecycle() {
    let fixture = DiagnosticTestFixture::load_all().expect("Should load diagnostic fixtures");

    let successful = fixture.filter_by_outcome(TestOutcome::Success);

    for snapshot in successful {
        // Verify complete initialization
        let has_storage_init = snapshot.init_checkpoints.iter().any(|c| {
            matches!(
                c.checkpoint,
                myriadnode::diagnostics::InitPhase::StorageInit
            ) && c.success
        });

        let has_router_init = snapshot.init_checkpoints.iter().any(|c| {
            matches!(c.checkpoint, myriadnode::diagnostics::InitPhase::RouterInit) && c.success
        });

        let has_complete = snapshot.init_checkpoints.iter().any(|c| {
            matches!(c.checkpoint, myriadnode::diagnostics::InitPhase::Complete) && c.success
        });

        assert!(
            has_storage_init,
            "Session {} missing successful Storage initialization",
            snapshot.session_id
        );

        assert!(
            has_router_init,
            "Session {} missing successful Router initialization",
            snapshot.session_id
        );

        assert!(
            has_complete,
            "Session {} did not complete initialization",
            snapshot.session_id
        );

        // Verify lifecycle progression
        let has_node_starting = snapshot.lifecycle_events.iter().any(|e| {
            matches!(
                e.event,
                myriadnode::diagnostics::LifecycleEventType::NodeStarting
            )
        });

        let has_queue_processor = snapshot.lifecycle_events.iter().any(|e| {
            matches!(
                e.event,
                myriadnode::diagnostics::LifecycleEventType::QueueProcessorStarted
            )
        });

        let has_node_ready = snapshot.lifecycle_events.iter().any(|e| {
            matches!(
                e.event,
                myriadnode::diagnostics::LifecycleEventType::NodeReady
            )
        });

        if has_node_starting {
            // If node started, it should have completed startup sequence
            assert!(
                has_queue_processor,
                "Session {} started but queue processor not started",
                snapshot.session_id
            );

            assert!(
                has_node_ready,
                "Session {} started but never reached ready state",
                snapshot.session_id
            );
        }
    }
}

#[test]
fn test_performance_regression_detection() {
    let fixture = DiagnosticTestFixture::load_all().expect("Should load diagnostic fixtures");

    let report = fixture.generate_report();

    // Performance baseline: initialization should complete in < 10 seconds
    const MAX_INIT_DURATION_MS: f64 = 10_000.0;

    if report.avg_init_duration_ms > MAX_INIT_DURATION_MS {
        panic!(
            "Performance regression detected: Average init duration {:.2}ms exceeds baseline {}ms",
            report.avg_init_duration_ms, MAX_INIT_DURATION_MS
        );
    }

    // Check individual snapshots for outliers
    for snapshot in fixture.all_snapshots() {
        if let Some(metadata) = &snapshot.startup_metadata {
            if metadata.total_init_duration_ms as f64 > MAX_INIT_DURATION_MS * 2.0 {
                println!(
                    "WARNING: Session {} took {:.2}ms to initialize (2x slower than baseline)",
                    snapshot.session_id, metadata.total_init_duration_ms
                );
            }
        }
    }
}

#[test]
fn test_failure_event_correlation() {
    let fixture = DiagnosticTestFixture::load_all().expect("Should load diagnostic fixtures");

    for snapshot in fixture.all_snapshots() {
        if !snapshot.failures.is_empty() {
            println!(
                "\nSession {} ({}) has {} failure(s):",
                snapshot.session_id,
                snapshot.metadata.description,
                snapshot.failures.len()
            );

            for failure in &snapshot.failures {
                println!(
                    "  {:?}: {} - {}",
                    failure.failure_type, failure.context, failure.error
                );
            }

            // Correlate failures with initialization checkpoints
            for checkpoint in &snapshot.init_checkpoints {
                if !checkpoint.success {
                    if let Some(error) = &checkpoint.error {
                        // Check if there's a corresponding failure event
                        let has_matching_failure =
                            snapshot.failures.iter().any(|f| f.error.contains(error));

                        if !has_matching_failure {
                            println!(
                                "  WARNING: Checkpoint {:?} failed but no matching failure event found",
                                checkpoint.checkpoint
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn test_export_diagnostic_report() {
    let fixture = DiagnosticTestFixture::load_all().expect("Should load diagnostic fixtures");

    let report = fixture.generate_report();

    // This would export a report for CI/CD dashboards
    let report_json = serde_json::to_string_pretty(&report).expect("Should serialize report");

    println!("Diagnostic Report JSON:\n{}", report_json);

    // In CI, this could be written to an artifacts directory
    // fs::write("target/diagnostic_report.json", report_json).ok();
}

// ==========================================
// Baseline Regression Detection Tests
// ==========================================

#[test]
fn test_baseline_manager_creation() {
    let mut manager = BaselineManager::new();

    // Load or create baseline
    manager.load().expect("Should load baseline");

    // Save baseline for future runs
    manager.save().expect("Should save baseline");

    println!("Baseline configuration:");
    println!(
        "  Max init duration: {}ms",
        manager.baseline().performance.max_init_duration_ms
    );
    println!(
        "  Max failures/100: {}",
        manager.baseline().reliability.max_failures_per_100
    );
    println!(
        "  Zero-tolerance failures: {:?}",
        manager.baseline().reliability.zero_tolerance_failures
    );
}

#[test]
fn test_baseline_regression_detection() {
    let mut manager = BaselineManager::new();
    manager.load().expect("Should load baseline");

    let fixture = DiagnosticTestFixture::load_all().expect("Should load diagnostic fixtures");

    for snapshot in fixture.all_snapshots() {
        let report = manager.analyze(snapshot);

        println!("\n=== Analyzing Session {} ===", snapshot.session_id);
        report.print();

        // Critical violations should fail the test
        if report.summary.critical_violations > 0 {
            panic!(
                "Critical regressions detected in session {}: {} violations",
                snapshot.session_id, report.summary.critical_violations
            );
        }
    }
}

#[test]
fn test_successful_snapshot_passes_baseline() {
    let mut manager = BaselineManager::new();
    manager.load().expect("Should load baseline");

    let fixture = DiagnosticTestFixture::load_all().expect("Should load diagnostic fixtures");

    let successful = fixture.filter_by_outcome(TestOutcome::Success);

    for snapshot in successful {
        let report = manager.analyze(snapshot);

        assert!(
            report.passed,
            "Successful snapshot {} should pass baseline checks. Violations: {:?}",
            snapshot.session_id, report.summary
        );

        assert_eq!(
            report.summary.critical_violations, 0,
            "Successful snapshot should have no critical violations"
        );
    }
}

#[test]
fn test_baseline_update_from_successful_runs() {
    let mut manager = BaselineManager::new();

    let fixture = DiagnosticTestFixture::load_all().expect("Should load diagnostic fixtures");

    let initial_max_duration = manager.baseline().performance.max_init_duration_ms;

    // Update baseline from successful runs
    manager.update_from_snapshots(fixture.all_snapshots());

    println!("Baseline updated:");
    println!("  Initial max duration: {}ms", initial_max_duration);
    println!(
        "  Updated max duration: {}ms",
        manager.baseline().performance.max_init_duration_ms
    );
    println!("  Sample size: {}", manager.baseline().sample_size);

    // Save updated baseline
    manager.save().expect("Should save updated baseline");
}

#[test]
fn test_performance_violation_explanations() {
    // Create a snapshot with slow initialization
    use myriadnode::diagnostics::test_data::DiagnosticSnapshot;
    use myriadnode::diagnostics::{InitCheckpoint, InitPhase, StartupMetadata};

    let slow_snapshot = DiagnosticSnapshot {
        captured_at: 1700000000,
        session_id: "slow-init-test".to_string(),
        init_checkpoints: vec![
            InitCheckpoint {
                timestamp: 1700000000,
                checkpoint: InitPhase::StorageInit,
                duration_ms: 2000, // BAD: > 100ms baseline
                success: true,
                error: None,
            },
            InitCheckpoint {
                timestamp: 1700000002,
                checkpoint: InitPhase::Complete,
                duration_ms: 15000, // BAD: > 5000ms baseline
                success: true,
                error: None,
            },
        ],
        lifecycle_events: vec![],
        failures: vec![],
        router_health: None,
        startup_metadata: Some(StartupMetadata {
            session_id: "slow-init-test".to_string(),
            start_time: 1700000000,
            config_hash: "test".to_string(),
            version: "0.1.0".to_string(),
            total_init_duration_ms: 15000, // BAD: > 5000ms
        }),
        metadata: myriadnode::diagnostics::test_data::SnapshotMetadata {
            description: "Slow initialization test".to_string(),
            scenario: "performance_regression".to_string(),
            expected_outcome:
                myriadnode::diagnostics::test_data::TestOutcome::PerformanceRegression,
            tags: vec!["slow".to_string()],
        },
    };

    let mut manager = BaselineManager::new();
    manager.load().expect("Should load baseline");

    let report = manager.analyze(&slow_snapshot);

    println!("\nPerformance regression analysis:");
    report.print();

    // Should detect performance violations
    assert!(!report.passed, "Slow snapshot should fail baseline");
    assert!(
        !report.performance_violations.is_empty(),
        "Should detect performance violations"
    );

    // Check that explanations are provided
    for violation in &report.performance_violations {
        assert!(
            !violation.explanation.is_empty(),
            "Violation should have explanation"
        );
        assert!(
            violation.explanation.contains("GOOD:"),
            "Explanation should define good metrics"
        );
        assert!(
            violation.explanation.contains("BAD:"),
            "Explanation should define bad metrics"
        );
        println!("\nViolation: {}", violation.metric);
        println!("Explanation: {}", violation.explanation);
    }
}

#[test]
fn test_integration_violation_detection() {
    use myriadnode::diagnostics::test_data::DiagnosticSnapshot;
    use myriadnode::diagnostics::RouterHealthSnapshot;

    let incomplete_integration = DiagnosticSnapshot {
        captured_at: 1700000000,
        session_id: "incomplete-integration".to_string(),
        init_checkpoints: vec![],
        lifecycle_events: vec![],
        failures: vec![],
        router_health: Some(RouterHealthSnapshot {
            timestamp: 1700000000,
            has_local_delivery_channel: false, // BAD: Missing
            has_confirmation_callback: true,
            has_message_sender: false, // BAD: Missing
            has_dht: false,
            queue_processor_running: false, // BAD: Not running
            local_delivery_processor_running: true,
            messages_routed: 0,
            messages_queued: 0,
            messages_failed: 0,
        }),
        startup_metadata: None,
        metadata: myriadnode::diagnostics::test_data::SnapshotMetadata {
            description: "Incomplete Router integration".to_string(),
            scenario: "integration_failure".to_string(),
            expected_outcome:
                myriadnode::diagnostics::test_data::TestOutcome::RouterIntegrationFailure,
            tags: vec!["router".to_string()],
        },
    };

    let mut manager = BaselineManager::new();
    manager.load().expect("Should load baseline");

    let report = manager.analyze(&incomplete_integration);

    println!("\nIntegration failure analysis:");
    report.print();

    // Should detect integration violations
    assert!(!report.passed, "Incomplete integration should fail");
    assert!(
        !report.integration_violations.is_empty(),
        "Should detect integration violations"
    );

    // Should explain what's wrong
    for violation in &report.integration_violations {
        assert!(!violation.explanation.is_empty());
        assert!(
            violation.explanation.contains("GOOD:"),
            "Should explain good state"
        );
        assert!(
            violation.explanation.contains("BAD:"),
            "Should explain bad state"
        );
        println!("\n{}: {}", violation.component, violation.issue);
        println!("Explanation: {}", violation.explanation);
    }
}
