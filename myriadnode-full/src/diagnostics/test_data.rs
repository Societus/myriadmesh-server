//! Test data injection from runtime diagnostics
//!
//! This module exports diagnostics data from production/development runs
//! and loads it into the test environment for regression testing.

use super::{
    DiagnosticsCollector, FailureEvent, InitCheckpoint, LifecycleEvent, RouterHealthSnapshot,
    StartupMetadata,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

/// Directory for diagnostic test data (relative to project root)
/// When running tests, we need to navigate up to project root
fn get_test_data_dir() -> PathBuf {
    // Try multiple paths to find the test data directory
    let candidates = vec![
        PathBuf::from("test_data/diagnostics"), // When run from project root
        PathBuf::from("../../test_data/diagnostics"), // When run from crate dir
        PathBuf::from("../../../test_data/diagnostics"), // When run from tests dir
    ];

    for path in candidates {
        if path.exists() {
            return path;
        }
    }

    // Default fallback
    PathBuf::from("test_data/diagnostics")
}

/// Exported diagnostic snapshot for test consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticSnapshot {
    /// When this snapshot was captured
    pub captured_at: u64,

    /// Session ID (NOT NodeId - privacy preserving)
    pub session_id: String,

    /// Initialization checkpoints
    pub init_checkpoints: Vec<InitCheckpoint>,

    /// Recent lifecycle events
    pub lifecycle_events: Vec<LifecycleEvent>,

    /// Failure events
    pub failures: Vec<FailureEvent>,

    /// Router health at snapshot time
    pub router_health: Option<RouterHealthSnapshot>,

    /// Startup metadata
    pub startup_metadata: Option<StartupMetadata>,

    /// Snapshot metadata
    pub metadata: SnapshotMetadata,
}

/// Metadata about the snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    /// Human-readable description
    pub description: String,

    /// Test scenario this represents
    pub scenario: String,

    /// Expected test outcome
    pub expected_outcome: TestOutcome,

    /// Tags for categorization
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TestOutcome {
    Success,
    InitializationFailure,
    RouterIntegrationFailure,
    MessageRoutingFailure,
    PerformanceRegression,
}

impl DiagnosticsCollector {
    /// Export diagnostics to disk for test consumption
    pub async fn export_to_file(
        &self,
        path: &Path,
        description: String,
        scenario: String,
    ) -> Result<()> {
        if !self.config.export_to_disk {
            return Ok(()); // Export disabled
        }

        let inner = self.inner.read().await;

        let snapshot = DiagnosticSnapshot {
            captured_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            session_id: inner.session_id.clone(),
            init_checkpoints: inner.init_checkpoints.iter().cloned().collect(),
            lifecycle_events: inner.lifecycle_events.iter().cloned().collect(),
            failures: inner.failures.iter().cloned().collect(),
            router_health: inner.router_health.clone(),
            startup_metadata: inner.startup_metadata.clone(),
            metadata: SnapshotMetadata {
                description,
                scenario,
                expected_outcome: Self::determine_outcome(&inner.failures),
                tags: Self::generate_tags(&inner.init_checkpoints, &inner.failures),
            },
        };

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Write as JSON
        let json = serde_json::to_string_pretty(&snapshot)
            .context("Failed to serialize diagnostic snapshot")?;
        fs::write(path, json).context("Failed to write diagnostic snapshot")?;

        Ok(())
    }

    /// Auto-export diagnostics on node shutdown (if enabled)
    pub async fn auto_export(&self) -> Result<()> {
        if !self.config.export_to_disk {
            return Ok(());
        }

        let export_dir = self
            .config
            .export_directory
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(get_test_data_dir);

        let session_id = self.session_id().await;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let filename = format!("session_{}_{}.json", session_id, timestamp);
        let path = export_dir.join(filename);

        self.export_to_file(
            &path,
            "Auto-exported session diagnostics".to_string(),
            "production_run".to_string(),
        )
        .await
    }

    fn determine_outcome(failures: &VecDeque<FailureEvent>) -> TestOutcome {
        for failure in failures.iter().rev().take(10) {
            match failure.failure_type {
                super::FailureType::InitializationFailure => {
                    return TestOutcome::InitializationFailure
                }
                super::FailureType::RouterIntegrationFailure => {
                    return TestOutcome::RouterIntegrationFailure
                }
                super::FailureType::MessageRoutingFailure => {
                    return TestOutcome::MessageRoutingFailure
                }
                _ => {}
            }
        }
        TestOutcome::Success
    }

    fn generate_tags(
        checkpoints: &VecDeque<InitCheckpoint>,
        failures: &VecDeque<FailureEvent>,
    ) -> Vec<String> {
        let mut tags = Vec::new();

        // Add checkpoint tags
        if checkpoints
            .iter()
            .any(|c| matches!(c.checkpoint, super::InitPhase::RouterInit))
        {
            tags.push("router".to_string());
        }

        // Add failure tags
        if !failures.is_empty() {
            tags.push("has_failures".to_string());
        }

        tags
    }
}

/// Test fixture loader for diagnostic snapshots
pub struct DiagnosticTestFixture {
    snapshots: Vec<DiagnosticSnapshot>,
}

impl DiagnosticTestFixture {
    /// Load all diagnostic snapshots from test data directory
    pub fn load_all() -> Result<Self> {
        let test_data_path = get_test_data_dir();

        if !test_data_path.exists() {
            return Ok(DiagnosticTestFixture {
                snapshots: Vec::new(),
            });
        }

        let mut snapshots = Vec::new();

        for entry in fs::read_dir(&test_data_path)? {
            let entry = entry?;
            let path = entry.path();

            // Skip baseline.json - it's not a diagnostic snapshot
            if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                if filename == "baseline.json" {
                    continue;
                }
            }

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                match Self::load_snapshot(&path) {
                    Ok(snapshot) => snapshots.push(snapshot),
                    Err(e) => {
                        eprintln!("Warning: Failed to load snapshot {:?}: {}", path, e);
                    }
                }
            }
        }

        Ok(DiagnosticTestFixture { snapshots })
    }

    /// Load a specific snapshot by filename
    pub fn load_snapshot(path: &Path) -> Result<DiagnosticSnapshot> {
        let contents = fs::read_to_string(path).context("Failed to read snapshot file")?;
        let snapshot: DiagnosticSnapshot =
            serde_json::from_str(&contents).context("Failed to parse snapshot JSON")?;
        Ok(snapshot)
    }

    /// Get all snapshots
    pub fn all_snapshots(&self) -> &[DiagnosticSnapshot] {
        &self.snapshots
    }

    /// Filter snapshots by outcome
    pub fn filter_by_outcome(&self, outcome: TestOutcome) -> Vec<&DiagnosticSnapshot> {
        self.snapshots
            .iter()
            .filter(|s| s.metadata.expected_outcome == outcome)
            .collect()
    }

    /// Filter snapshots by tag
    pub fn filter_by_tag(&self, tag: &str) -> Vec<&DiagnosticSnapshot> {
        self.snapshots
            .iter()
            .filter(|s| s.metadata.tags.contains(&tag.to_string()))
            .collect()
    }

    /// Filter snapshots by scenario
    pub fn filter_by_scenario(&self, scenario: &str) -> Vec<&DiagnosticSnapshot> {
        self.snapshots
            .iter()
            .filter(|s| s.metadata.scenario == scenario)
            .collect()
    }

    /// Get snapshots with initialization failures
    pub fn initialization_failures(&self) -> Vec<&DiagnosticSnapshot> {
        self.snapshots
            .iter()
            .filter(|s| s.init_checkpoints.iter().any(|c| !c.success))
            .collect()
    }

    /// Get snapshots with router integration issues
    pub fn router_integration_issues(&self) -> Vec<&DiagnosticSnapshot> {
        self.snapshots
            .iter()
            .filter(|s| {
                if let Some(health) = &s.router_health {
                    !health.has_local_delivery_channel
                        || !health.has_confirmation_callback
                        || !health.has_message_sender
                } else {
                    false
                }
            })
            .collect()
    }

    /// Generate test report from snapshots
    pub fn generate_report(&self) -> TestReport {
        let total = self.snapshots.len();
        let successful = self.filter_by_outcome(TestOutcome::Success).len();
        let init_failures = self
            .filter_by_outcome(TestOutcome::InitializationFailure)
            .len();
        let router_failures = self
            .filter_by_outcome(TestOutcome::RouterIntegrationFailure)
            .len();
        let routing_failures = self
            .filter_by_outcome(TestOutcome::MessageRoutingFailure)
            .len();

        TestReport {
            total_snapshots: total,
            successful_runs: successful,
            initialization_failures: init_failures,
            router_integration_failures: router_failures,
            message_routing_failures: routing_failures,
            avg_init_duration_ms: self.calculate_avg_init_duration(),
            unique_failure_types: self.collect_unique_failure_types(),
        }
    }

    fn calculate_avg_init_duration(&self) -> f64 {
        if self.snapshots.is_empty() {
            return 0.0;
        }

        let total: u64 = self
            .snapshots
            .iter()
            .filter_map(|s| s.startup_metadata.as_ref())
            .map(|m| m.total_init_duration_ms)
            .sum();

        total as f64 / self.snapshots.len() as f64
    }

    fn collect_unique_failure_types(&self) -> Vec<String> {
        use std::collections::HashSet;
        let mut types = HashSet::new();

        for snapshot in &self.snapshots {
            for failure in &snapshot.failures {
                types.insert(format!("{:?}", failure.failure_type));
            }
        }

        types.into_iter().collect()
    }
}

/// Test report generated from diagnostic snapshots
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestReport {
    pub total_snapshots: usize,
    pub successful_runs: usize,
    pub initialization_failures: usize,
    pub router_integration_failures: usize,
    pub message_routing_failures: usize,
    pub avg_init_duration_ms: f64,
    pub unique_failure_types: Vec<String>,
}

impl TestReport {
    /// Print human-readable report
    pub fn print(&self) {
        println!("=== Diagnostic Test Report ===");
        println!("Total sessions analyzed: {}", self.total_snapshots);
        println!("Successful runs: {}", self.successful_runs);
        println!("Initialization failures: {}", self.initialization_failures);
        println!(
            "Router integration failures: {}",
            self.router_integration_failures
        );
        println!(
            "Message routing failures: {}",
            self.message_routing_failures
        );
        println!("Average init duration: {:.2}ms", self.avg_init_duration_ms);
        println!("Unique failure types: {:?}", self.unique_failure_types);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixture_loading() {
        // This test will work once we have actual snapshot files
        let fixture = DiagnosticTestFixture::load_all();
        assert!(fixture.is_ok());
    }

    #[test]
    fn test_outcome_filtering() {
        let fixture = DiagnosticTestFixture { snapshots: vec![] };

        let successful = fixture.filter_by_outcome(TestOutcome::Success);
        assert_eq!(successful.len(), 0);
    }
}
