//! Regression detection baseline system
//!
//! This module defines what "good" metrics look like and detects deviations
//! that indicate performance regressions, reliability issues, or integration failures.

use super::test_data::DiagnosticSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// What constitutes "good" metrics for Node initialization and operation
///
/// These baselines are derived from successful production runs and define
/// acceptable ranges for performance, reliability, and integration health.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceBaseline {
    /// Maximum acceptable initialization duration (milliseconds)
    ///
    /// GOOD: < 5000ms (5 seconds)
    /// - Storage init: ~50ms
    /// - Router init: ~20ms
    /// - DHT init: ~10ms
    /// - Ledger init: ~100ms
    /// - Total: < 5s for cold start
    ///
    /// BAD: > 10000ms (10 seconds)
    /// - Indicates disk I/O issues
    /// - Database contention
    /// - Network delays in bootstrap
    pub max_init_duration_ms: u64,

    /// Maximum acceptable per-phase durations
    ///
    /// GOOD per-phase times:
    /// - StorageInit: < 100ms (disk ready)
    /// - RouterInit: < 50ms (in-memory setup)
    /// - DHT init: < 200ms (bootstrap lookup)
    /// - LedgerInit: < 500ms (DB open + migration)
    ///
    /// BAD indicators:
    /// - Storage > 1s: Disk thrashing or corruption
    /// - Router > 100ms: Memory pressure
    /// - DHT > 5s: Network unreachable
    pub max_phase_durations: HashMap<String, u64>,

    /// When this baseline was established (UNIX timestamp)
    pub baseline_date: u64,

    /// Version this baseline applies to
    pub version: String,
}

impl Default for PerformanceBaseline {
    fn default() -> Self {
        let mut phase_durations = HashMap::new();
        phase_durations.insert("StorageInit".to_string(), 100);
        phase_durations.insert("RouterInit".to_string(), 50);
        phase_durations.insert("RouterCallbacksSet".to_string(), 10);
        phase_durations.insert("DhtInit".to_string(), 200);
        phase_durations.insert("LedgerInit".to_string(), 500);
        phase_durations.insert("Complete".to_string(), 5000);

        PerformanceBaseline {
            max_init_duration_ms: 5000,
            max_phase_durations: phase_durations,
            baseline_date: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// What constitutes "good" reliability metrics
///
/// Defines acceptable failure rates and error thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReliabilityBaseline {
    /// Maximum acceptable failure rate per 100 operations
    ///
    /// GOOD: 0 failures/100 (100% success rate)
    /// - All initialization phases complete
    /// - No critical errors
    /// - Clean startup/shutdown
    ///
    /// ACCEPTABLE: < 5 failures/100 (95% success rate)
    /// - Occasional network timeouts (DHT bootstrap)
    /// - Transient disk errors (retryable)
    /// - Non-critical subsystem warnings
    ///
    /// BAD: > 10 failures/100 (< 90% success rate)
    /// - Indicates systemic issues
    /// - Component integration broken
    /// - Environment problems (permissions, resources)
    pub max_failures_per_100: usize,

    /// Failure types that are NEVER acceptable (zero tolerance)
    ///
    /// These failures indicate critical bugs that must be fixed:
    /// - InitializationFailure: Component failed to start
    /// - RouterIntegrationFailure: Callbacks not set up
    /// - Memory corruption
    /// - Data loss
    pub zero_tolerance_failures: Vec<String>,

    /// Acceptable failure types (with limits)
    ///
    /// Some failures are expected in distributed systems:
    /// - NetworkFailure: Transient connectivity (< 10%)
    /// - ConfigurationError: User error, not code bug
    /// - Resource exhaustion: Temporary, should recover
    pub acceptable_failures: HashMap<String, usize>,
}

impl Default for ReliabilityBaseline {
    fn default() -> Self {
        let mut acceptable = HashMap::new();
        acceptable.insert("NetworkFailure".to_string(), 10); // < 10% network failures OK
        acceptable.insert("ConfigurationError".to_string(), 0); // User errors logged but not counted

        ReliabilityBaseline {
            max_failures_per_100: 5,
            zero_tolerance_failures: vec![
                "InitializationFailure".to_string(),
                "RouterIntegrationFailure".to_string(),
            ],
            acceptable_failures: acceptable,
        }
    }
}

/// What constitutes "good" integration health
///
/// Defines required component connections and lifecycle states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationBaseline {
    /// Router must have all callbacks configured
    ///
    /// GOOD (all true):
    /// - has_local_delivery_channel: Messages routed to local apps
    /// - has_confirmation_callback: Delivery tracked in ledger
    /// - has_message_sender: Can transmit to network
    /// - has_dht: Peer discovery enabled
    ///
    /// BAD (any false):
    /// - Missing callbacks = incomplete integration
    /// - Router won't function correctly
    /// - Messages will be dropped
    pub require_complete_router_integration: bool,

    /// Background processors must be running
    ///
    /// GOOD (all running):
    /// - queue_processor_running: Retry logic active
    /// - local_delivery_processor_running: Can receive messages
    ///
    /// BAD (any stopped):
    /// - Queue processor stopped = no retries, messages lost
    /// - Local delivery stopped = can't receive messages
    pub require_queue_processor: bool,
    pub require_local_delivery_processor: bool,

    /// Required lifecycle event sequence
    ///
    /// GOOD sequence:
    /// 1. NodeStarting
    /// 2. QueueProcessorStarted
    /// 3. LocalDeliveryProcessorStarted (optional if configured)
    /// 4. NodeReady
    ///
    /// BAD:
    /// - Missing events = incomplete startup
    /// - Out of order = race condition
    /// - Never reaches NodeReady = hung initialization
    pub required_lifecycle_sequence: Vec<String>,

    /// Required initialization phases (must all succeed)
    ///
    /// GOOD:
    /// - All phases complete with success=true
    /// - No error messages
    ///
    /// BAD:
    /// - Any phase with success=false
    /// - Missing phases (incomplete init)
    pub required_init_phases: Vec<String>,
}

impl Default for IntegrationBaseline {
    fn default() -> Self {
        IntegrationBaseline {
            require_complete_router_integration: true,
            require_queue_processor: true,
            require_local_delivery_processor: true,
            required_lifecycle_sequence: vec![
                "NodeStarting".to_string(),
                "QueueProcessorStarted".to_string(),
                "NodeReady".to_string(),
            ],
            required_init_phases: vec![
                "StorageInit".to_string(),
                "RouterInit".to_string(),
                "RouterCallbacksSet".to_string(),
                "Complete".to_string(),
            ],
        }
    }
}

/// Complete baseline configuration
///
/// Combines all baseline categories to define "good" system behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineConfig {
    pub performance: PerformanceBaseline,
    pub reliability: ReliabilityBaseline,
    pub integration: IntegrationBaseline,

    /// When this baseline was last updated
    pub last_updated: u64,

    /// Number of successful runs this baseline represents
    pub sample_size: usize,
}

impl Default for BaselineConfig {
    fn default() -> Self {
        BaselineConfig {
            performance: PerformanceBaseline::default(),
            reliability: ReliabilityBaseline::default(),
            integration: IntegrationBaseline::default(),
            last_updated: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            sample_size: 1,
        }
    }
}

/// Regression detection results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionReport {
    /// Overall assessment
    pub passed: bool,

    /// Performance violations
    pub performance_violations: Vec<PerformanceViolation>,

    /// Reliability violations
    pub reliability_violations: Vec<ReliabilityViolation>,

    /// Integration violations
    pub integration_violations: Vec<IntegrationViolation>,

    /// Summary statistics
    pub summary: RegressionSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceViolation {
    pub metric: String,
    pub actual_value: f64,
    pub baseline_value: f64,
    pub severity: Severity,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReliabilityViolation {
    pub failure_type: String,
    pub count: usize,
    pub baseline_max: usize,
    pub severity: Severity,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationViolation {
    pub component: String,
    pub issue: String,
    pub severity: Severity,
    pub explanation: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionSummary {
    pub total_violations: usize,
    pub critical_violations: usize,
    pub warning_violations: usize,
    pub snapshots_analyzed: usize,
    pub baseline_version: String,
}

/// Baseline manager - loads, stores, and applies baselines
pub struct BaselineManager {
    baseline: BaselineConfig,
    baseline_path: PathBuf,
}

fn get_baseline_path() -> PathBuf {
    // Try multiple paths to find the test data directory
    let candidates = vec![
        PathBuf::from("test_data/diagnostics/baseline.json"), // When run from project root
        PathBuf::from("../../test_data/diagnostics/baseline.json"), // When run from crate dir
        PathBuf::from("../../../test_data/diagnostics/baseline.json"), // When run from tests dir
    ];

    for path in &candidates {
        if let Some(parent) = path.parent() {
            if parent.exists() {
                return path.clone();
            }
        }
    }

    // Default fallback
    PathBuf::from("test_data/diagnostics/baseline.json")
}

impl BaselineManager {
    /// Create baseline manager with default baseline
    pub fn new() -> Self {
        BaselineManager {
            baseline: BaselineConfig::default(),
            baseline_path: get_baseline_path(),
        }
    }

    /// Create baseline manager with custom path
    pub fn with_path(path: PathBuf) -> Self {
        BaselineManager {
            baseline: BaselineConfig::default(),
            baseline_path: path,
        }
    }

    /// Get reference to baseline configuration
    pub fn baseline(&self) -> &BaselineConfig {
        &self.baseline
    }

    /// Load baseline from disk (or create default if missing/corrupted)
    pub fn load(&mut self) -> anyhow::Result<()> {
        if !self.baseline_path.exists() {
            // No baseline exists, use default and save it
            self.save()?;
            return Ok(());
        }

        // Try to load existing baseline
        match fs::read_to_string(&self.baseline_path) {
            Ok(contents) if !contents.trim().is_empty() => {
                // Try to parse the baseline
                match serde_json::from_str(&contents) {
                    Ok(baseline) => {
                        self.baseline = baseline;
                        Ok(())
                    }
                    Err(e) => {
                        // Corrupted baseline file - recreate with defaults
                        eprintln!(
                            "Warning: Baseline file corrupted, recreating with defaults: {}",
                            e
                        );
                        self.baseline = BaselineConfig::default();
                        self.save()?;
                        Ok(())
                    }
                }
            }
            _ => {
                // Empty or unreadable file - recreate with defaults
                eprintln!("Warning: Baseline file empty or unreadable, recreating with defaults");
                self.baseline = BaselineConfig::default();
                self.save()?;
                Ok(())
            }
        }
    }

    /// Save baseline to disk
    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.baseline_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(&self.baseline)?;
        fs::write(&self.baseline_path, json)?;
        Ok(())
    }

    /// Analyze snapshot against baseline
    pub fn analyze(&self, snapshot: &DiagnosticSnapshot) -> RegressionReport {
        let mut performance_violations = Vec::new();
        let mut reliability_violations = Vec::new();
        let mut integration_violations = Vec::new();

        // Check performance
        if let Some(metadata) = &snapshot.startup_metadata {
            let actual = metadata.total_init_duration_ms;
            let baseline = self.baseline.performance.max_init_duration_ms;

            if actual > baseline {
                let severity = if actual > baseline * 2 {
                    Severity::Critical
                } else {
                    Severity::Warning
                };

                performance_violations.push(PerformanceViolation {
                    metric: "total_init_duration_ms".to_string(),
                    actual_value: actual as f64,
                    baseline_value: baseline as f64,
                    severity,
                    explanation: format!(
                        "Initialization took {}ms, baseline is {}ms. GOOD: <{}ms, BAD: >{}ms. \
                        Possible causes: Disk I/O bottleneck, database lock contention, network delays in DHT bootstrap.",
                        actual, baseline, baseline, baseline * 2
                    ),
                });
            }
        }

        // Check per-phase durations
        for checkpoint in &snapshot.init_checkpoints {
            let phase_name = format!("{:?}", checkpoint.checkpoint);
            if let Some(&baseline_duration) = self
                .baseline
                .performance
                .max_phase_durations
                .get(&phase_name)
            {
                if checkpoint.duration_ms > baseline_duration {
                    let severity = if checkpoint.duration_ms > baseline_duration * 3 {
                        Severity::Critical
                    } else {
                        Severity::Warning
                    };

                    performance_violations.push(PerformanceViolation {
                        metric: format!("{}_duration", phase_name),
                        actual_value: checkpoint.duration_ms as f64,
                        baseline_value: baseline_duration as f64,
                        severity,
                        explanation: Self::explain_phase_slowness(
                            &phase_name,
                            checkpoint.duration_ms,
                            baseline_duration,
                        ),
                    });
                }
            }
        }

        // Check reliability (failures)
        let failure_counts = Self::count_failures(&snapshot.failures);
        for (failure_type, count) in &failure_counts {
            // Check zero-tolerance failures
            if self
                .baseline
                .reliability
                .zero_tolerance_failures
                .contains(failure_type)
                && *count > 0
            {
                reliability_violations.push(ReliabilityViolation {
                    failure_type: failure_type.clone(),
                    count: *count,
                    baseline_max: 0,
                    severity: Severity::Critical,
                    explanation: format!(
                        "{} is a zero-tolerance failure type. NEVER acceptable. \
                        Indicates critical bug: component failed to initialize, callbacks not set, or data corruption. \
                        Must be fixed before release.",
                        failure_type
                    ),
                });
            }

            // Check acceptable failures
            if let Some(&max_acceptable) = self
                .baseline
                .reliability
                .acceptable_failures
                .get(failure_type)
            {
                if *count > max_acceptable {
                    reliability_violations.push(ReliabilityViolation {
                        failure_type: failure_type.clone(),
                        count: *count,
                        baseline_max: max_acceptable,
                        severity: Severity::Warning,
                        explanation: format!(
                            "{} count ({}) exceeds baseline ({}). GOOD: 0, ACCEPTABLE: <{}, BAD: >{}. \
                            While some {} failures are expected in distributed systems, this rate is too high.",
                            failure_type, count, max_acceptable, max_acceptable, max_acceptable * 2, failure_type
                        ),
                    });
                }
            }
        }

        // Check integration health
        if let Some(health) = &snapshot.router_health {
            if self
                .baseline
                .integration
                .require_complete_router_integration
            {
                if !health.has_local_delivery_channel {
                    integration_violations.push(IntegrationViolation {
                        component: "Router".to_string(),
                        issue: "Missing local delivery channel".to_string(),
                        severity: Severity::Critical,
                        explanation: "Router must have local delivery channel configured. Without this, messages destined for local applications cannot be delivered. GOOD: Channel set during Node::new(). BAD: Missing callback = messages dropped.".to_string(),
                    });
                }

                if !health.has_confirmation_callback {
                    integration_violations.push(IntegrationViolation {
                        component: "Router".to_string(),
                        issue: "Missing confirmation callback".to_string(),
                        severity: Severity::Critical,
                        explanation: "Router must have confirmation callback for ledger integration. Without this, message delivery confirmations are not recorded in the blockchain. GOOD: Callback set during Node::new(). BAD: Lost audit trail.".to_string(),
                    });
                }

                if !health.has_message_sender {
                    integration_violations.push(IntegrationViolation {
                        component: "Router".to_string(),
                        issue: "Missing message sender callback".to_string(),
                        severity: Severity::Critical,
                        explanation: "Router must have message sender callback to transmit to network. Without this, messages cannot be forwarded to next hop. GOOD: Set by AdapterManager. BAD: Router cannot send messages.".to_string(),
                    });
                }
            }

            if self.baseline.integration.require_queue_processor && !health.queue_processor_running
            {
                integration_violations.push(IntegrationViolation {
                    component: "QueueProcessor".to_string(),
                    issue: "Queue processor not running".to_string(),
                    severity: Severity::Critical,
                    explanation: "Queue processor handles message retries with exponential backoff. Without this, failed transmissions are never retried and messages are lost. GOOD: Started in Node::run(). BAD: No retry logic = message loss.".to_string(),
                });
            }

            if self.baseline.integration.require_local_delivery_processor
                && !health.local_delivery_processor_running
            {
                integration_violations.push(IntegrationViolation {
                    component: "LocalDeliveryProcessor".to_string(),
                    issue: "Local delivery processor not running".to_string(),
                    severity: Severity::Critical,
                    explanation: "Local delivery processor handles incoming messages for this node. Without this, the node cannot receive messages. GOOD: Started in Node::run(). BAD: Node is deaf to network.".to_string(),
                });
            }
        }

        // Check required initialization phases
        for required_phase in &self.baseline.integration.required_init_phases {
            let phase_found = snapshot
                .init_checkpoints
                .iter()
                .any(|c| format!("{:?}", c.checkpoint) == *required_phase && c.success);

            if !phase_found {
                integration_violations.push(IntegrationViolation {
                    component: "Initialization".to_string(),
                    issue: format!("Missing or failed required phase: {}", required_phase),
                    severity: Severity::Critical,
                    explanation: format!(
                        "{} is a required initialization phase. GOOD: Phase completes successfully. BAD: Missing or failed = incomplete initialization. Node will not function correctly.",
                        required_phase
                    ),
                });
            }
        }

        // Calculate summary
        let critical_count = performance_violations
            .iter()
            .filter(|v| v.severity == Severity::Critical)
            .count()
            + reliability_violations
                .iter()
                .filter(|v| v.severity == Severity::Critical)
                .count()
            + integration_violations
                .iter()
                .filter(|v| v.severity == Severity::Critical)
                .count();

        let warning_count = performance_violations
            .iter()
            .filter(|v| v.severity == Severity::Warning)
            .count()
            + reliability_violations
                .iter()
                .filter(|v| v.severity == Severity::Warning)
                .count()
            + integration_violations
                .iter()
                .filter(|v| v.severity == Severity::Warning)
                .count();

        let total = performance_violations.len()
            + reliability_violations.len()
            + integration_violations.len();

        RegressionReport {
            passed: critical_count == 0,
            performance_violations,
            reliability_violations,
            integration_violations,
            summary: RegressionSummary {
                total_violations: total,
                critical_violations: critical_count,
                warning_violations: warning_count,
                snapshots_analyzed: 1,
                baseline_version: self.baseline.performance.version.clone(),
            },
        }
    }

    fn explain_phase_slowness(phase: &str, actual: u64, baseline: u64) -> String {
        match phase {
            "StorageInit" => format!(
                "Storage initialization took {}ms (baseline: {}ms). GOOD: <100ms (disk ready). BAD: >1000ms. \
                Possible causes: Disk thrashing, slow I/O, corrupted database files, insufficient IOPS.",
                actual, baseline
            ),
            "RouterInit" => format!(
                "Router initialization took {}ms (baseline: {}ms). GOOD: <50ms (in-memory setup). BAD: >100ms. \
                Possible causes: Memory pressure, CPU contention, lock contention in queue initialization.",
                actual, baseline
            ),
            "RouterCallbacksSet" => format!(
                "Router callbacks setup took {}ms (baseline: {}ms). GOOD: <10ms (function pointers). BAD: >50ms. \
                Possible causes: Callback initialization blocking, Arc clone contention, memory allocation delays.",
                actual, baseline
            ),
            "DhtInit" => format!(
                "DHT initialization took {}ms (baseline: {}ms). GOOD: <200ms (bootstrap lookup). BAD: >5000ms. \
                Possible causes: Network unreachable, DNS failure, bootstrap nodes offline, firewall blocking.",
                actual, baseline
            ),
            "LedgerInit" => format!(
                "Ledger initialization took {}ms (baseline: {}ms). GOOD: <500ms (DB open + migration). BAD: >2000ms. \
                Possible causes: Database lock held, slow disk, large database size, migration running.",
                actual, baseline
            ),
            "Complete" => format!(
                "Complete initialization took {}ms (baseline: {}ms). GOOD: <5000ms (all phases done). BAD: >10000ms. \
                This is total time from start to finish. Check individual phase durations for bottlenecks.",
                actual, baseline
            ),
            _ => format!(
                "{} took {}ms (baseline: {}ms). GOOD: <{}ms, BAD: >{}ms. Slower than expected - investigate this phase.",
                phase, actual, baseline, baseline, baseline * 2
            ),
        }
    }

    fn count_failures(failures: &[super::FailureEvent]) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for failure in failures {
            let type_str = format!("{:?}", failure.failure_type);
            *counts.entry(type_str).or_insert(0) += 1;
        }
        counts
    }

    /// Update baseline from successful snapshots
    pub fn update_from_snapshots(&mut self, snapshots: &[DiagnosticSnapshot]) {
        // Only use successful snapshots for baseline
        let successful: Vec<_> = snapshots
            .iter()
            .filter(|s| s.failures.is_empty())
            .filter(|s| s.init_checkpoints.iter().all(|c| c.success))
            .collect();

        if successful.is_empty() {
            return;
        }

        // Update sample size
        self.baseline.sample_size = successful.len();

        // Calculate average init duration and use 95th percentile as max
        let mut durations: Vec<u64> = successful
            .iter()
            .filter_map(|s| s.startup_metadata.as_ref())
            .map(|m| m.total_init_duration_ms)
            .collect();

        if !durations.is_empty() {
            durations.sort();
            let p95_index = (durations.len() as f64 * 0.95) as usize;
            self.baseline.performance.max_init_duration_ms =
                durations[p95_index.min(durations.len() - 1)];
        }

        self.baseline.last_updated = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }
}

impl Default for BaselineManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RegressionReport {
    /// Print human-readable report
    pub fn print(&self) {
        println!("\n=== Regression Detection Report ===");
        println!(
            "Overall: {}",
            if self.passed {
                "✅ PASSED"
            } else {
                "❌ FAILED"
            }
        );
        println!("Total violations: {}", self.summary.total_violations);
        println!("  Critical: {}", self.summary.critical_violations);
        println!("  Warnings: {}", self.summary.warning_violations);
        println!("Baseline version: {}", self.summary.baseline_version);

        if !self.performance_violations.is_empty() {
            println!("\n--- Performance Violations ---");
            for v in &self.performance_violations {
                println!(
                    "  [{:?}] {}: {:.2} (baseline: {:.2})",
                    v.severity, v.metric, v.actual_value, v.baseline_value
                );
                println!("    {}", v.explanation);
            }
        }

        if !self.reliability_violations.is_empty() {
            println!("\n--- Reliability Violations ---");
            for v in &self.reliability_violations {
                println!(
                    "  [{:?}] {}: {} failures (max: {})",
                    v.severity, v.failure_type, v.count, v.baseline_max
                );
                println!("    {}", v.explanation);
            }
        }

        if !self.integration_violations.is_empty() {
            println!("\n--- Integration Violations ---");
            for v in &self.integration_violations {
                println!("  [{:?}] {}: {}", v.severity, v.component, v.issue);
                println!("    {}", v.explanation);
            }
        }

        if self.passed {
            println!("\n✅ All metrics within baseline. No regressions detected.");
        } else {
            println!("\n❌ Critical violations detected. Review and fix before release.");
        }
    }
}
