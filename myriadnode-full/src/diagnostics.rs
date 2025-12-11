//! Runtime diagnostics and telemetry system
//!
//! This module captures real-world usage data, lifecycle events, and failures
//! to feed into the testing environment for regression detection and integration verification.
//!
//! SECURITY NOTE: Uses random session IDs instead of NodeIds to preserve privacy.
//! Diagnostics are disabled by default and must be explicitly enabled.

pub mod baseline;
pub mod test_data;

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Maximum number of events to keep in memory
const MAX_EVENTS: usize = 10000;

/// Maximum number of initialization checkpoints to store
const MAX_INIT_CHECKPOINTS: usize = 100;

/// Diagnostics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsConfig {
    /// Enable full diagnostics collection (disabled by default for privacy)
    pub enabled: bool,

    /// Enable basic health checks (E2E failures, critical errors) even when disabled
    pub enable_health_checks: bool,

    /// Export diagnostics to disk for test consumption
    pub export_to_disk: bool,

    /// Directory for diagnostic exports
    pub export_directory: Option<String>,
}

impl Default for DiagnosticsConfig {
    fn default() -> Self {
        DiagnosticsConfig {
            enabled: false,             // Disabled by default for privacy
            enable_health_checks: true, // Always warn about critical issues
            export_to_disk: false,
            export_directory: None,
        }
    }
}

/// Diagnostics collector for runtime telemetry
#[derive(Clone)]
pub struct DiagnosticsCollector {
    inner: Arc<RwLock<DiagnosticsInner>>,
    config: DiagnosticsConfig,
}

struct DiagnosticsInner {
    /// Unique session ID (NOT the NodeId - privacy-preserving)
    session_id: String,

    /// Initialization checkpoints
    init_checkpoints: VecDeque<InitCheckpoint>,

    /// Lifecycle events
    lifecycle_events: VecDeque<LifecycleEvent>,

    /// Failure events (always collected for health checks)
    failures: VecDeque<FailureEvent>,

    /// Router integration health
    router_health: Option<RouterHealthSnapshot>,

    /// Node startup metadata
    startup_metadata: Option<StartupMetadata>,
}

/// Initialization checkpoint - tracks progress through Node::new()
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitCheckpoint {
    pub timestamp: u64,
    pub checkpoint: InitPhase,
    pub duration_ms: u64,
    pub success: bool,
    pub error: Option<String>,
}

/// Phases of node initialization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum InitPhase {
    StorageInit,
    AdapterManagerInit,
    MessageQueueInit,
    DhtInit,
    RouterInit,
    RouterCallbacksSet,
    LedgerInit,
    NetworkMonitorInit,
    FailoverManagerInit,
    HeartbeatServiceInit,
    ApiServerInit,
    ApplianceManagerInit,
    Complete,
}

/// Lifecycle event - tracks runtime state transitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleEvent {
    pub timestamp: u64,
    pub event: LifecycleEventType,
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LifecycleEventType {
    NodeStarting,
    NodeReady,
    QueueProcessorStarted,
    LocalDeliveryProcessorStarted,
    RouterConfigured,
    MessageRouted { priority: u8, success: bool },
    QueueRetry { retry_count: u32 },
    OfflineCache { message_count: usize },
    NodeStopping,
    NodeStopped,
}

/// Failure event - captures runtime errors for regression testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureEvent {
    pub timestamp: u64,
    pub failure_type: FailureType,
    pub context: String,
    pub error: String,
    pub stack_trace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FailureType {
    InitializationFailure,
    RouterIntegrationFailure,
    MessageRoutingFailure,
    QueueProcessorFailure,
    NetworkFailure,
    ConfigurationError,
    Unknown,
}

/// Router health snapshot - captures Router integration state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterHealthSnapshot {
    pub timestamp: u64,
    pub has_local_delivery_channel: bool,
    pub has_confirmation_callback: bool,
    pub has_message_sender: bool,
    pub has_dht: bool,
    pub queue_processor_running: bool,
    pub local_delivery_processor_running: bool,
    pub messages_routed: u64,
    pub messages_queued: u64,
    pub messages_failed: u64,
}

/// Startup metadata - key info for test reproduction
/// SECURITY: Uses random session_id instead of NodeId to preserve privacy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupMetadata {
    /// Random session ID (NOT the NodeId - privacy-preserving)
    pub session_id: String,
    pub start_time: u64,
    pub config_hash: String,
    pub version: String,
    pub total_init_duration_ms: u64,
}

/// Test case generated from real-world usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedTestCase {
    pub name: String,
    pub description: String,
    pub preconditions: Vec<String>,
    pub actions: Vec<TestAction>,
    pub expected_outcomes: Vec<String>,
    pub source_event: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestAction {
    InitializeNode { config_hash: String },
    RouteMessage { priority: u8, size: usize },
    TriggerFailure { failure_type: String },
    WaitForQueueProcessor { duration_ms: u64 },
    CheckRouterHealth,
}

impl DiagnosticsCollector {
    /// Create a new diagnostics collector with default config (disabled for privacy)
    pub fn new() -> Self {
        Self::with_config(DiagnosticsConfig::default())
    }

    /// Create a new diagnostics collector with custom config
    pub fn with_config(config: DiagnosticsConfig) -> Self {
        DiagnosticsCollector {
            inner: Arc::new(RwLock::new(DiagnosticsInner {
                session_id: Uuid::new_v4().to_string(), // Random session ID for privacy
                init_checkpoints: VecDeque::with_capacity(MAX_INIT_CHECKPOINTS),
                lifecycle_events: VecDeque::with_capacity(MAX_EVENTS),
                failures: VecDeque::with_capacity(MAX_EVENTS),
                router_health: None,
                startup_metadata: None,
            })),
            config,
        }
    }

    /// Get the session ID (random UUID, not NodeId)
    pub async fn session_id(&self) -> String {
        let inner = self.inner.read().await;
        inner.session_id.clone()
    }

    /// Record an initialization checkpoint (only if diagnostics enabled)
    pub async fn checkpoint(
        &self,
        phase: InitPhase,
        duration_ms: u64,
        success: bool,
        error: Option<String>,
    ) {
        if !self.config.enabled {
            return;
        }

        let mut inner = self.inner.write().await;

        let checkpoint = InitCheckpoint {
            timestamp: current_timestamp(),
            checkpoint: phase,
            duration_ms,
            success,
            error,
        };

        inner.init_checkpoints.push_back(checkpoint);
        if inner.init_checkpoints.len() > MAX_INIT_CHECKPOINTS {
            inner.init_checkpoints.pop_front();
        }
    }

    /// Record a lifecycle event (only if diagnostics enabled)
    pub async fn event(&self, event: LifecycleEventType, metadata: Option<String>) {
        if !self.config.enabled {
            return;
        }

        let mut inner = self.inner.write().await;

        let lifecycle_event = LifecycleEvent {
            timestamp: current_timestamp(),
            event,
            metadata,
        };

        inner.lifecycle_events.push_back(lifecycle_event);
        if inner.lifecycle_events.len() > MAX_EVENTS {
            inner.lifecycle_events.pop_front();
        }
    }

    /// Record a failure (ALWAYS collected for health checks, even when diagnostics disabled)
    pub async fn failure(&self, failure_type: FailureType, context: String, error: String) {
        // Log critical failures immediately (E2E health check)
        if self.config.enable_health_checks {
            match failure_type {
                FailureType::InitializationFailure | FailureType::RouterIntegrationFailure => {
                    tracing::error!("HEALTH CHECK FAILURE: {} - {}", context, error);
                }
                _ => {}
            }
        }

        let mut inner = self.inner.write().await;

        let failure = FailureEvent {
            timestamp: current_timestamp(),
            failure_type,
            context,
            error,
            stack_trace: None, // TODO: Capture stack trace
        };

        inner.failures.push_back(failure);
        if inner.failures.len() > MAX_EVENTS {
            inner.failures.pop_front();
        }
    }

    /// Update router health snapshot
    pub async fn update_router_health(&self, health: RouterHealthSnapshot) {
        let mut inner = self.inner.write().await;
        inner.router_health = Some(health);
    }

    /// Set startup metadata
    pub async fn set_startup_metadata(&self, metadata: StartupMetadata) {
        let mut inner = self.inner.write().await;
        inner.startup_metadata = Some(metadata);
    }

    /// Get all initialization checkpoints
    pub async fn get_init_checkpoints(&self) -> Vec<InitCheckpoint> {
        let inner = self.inner.read().await;
        inner.init_checkpoints.iter().cloned().collect()
    }

    /// Get recent lifecycle events
    pub async fn get_lifecycle_events(&self, limit: usize) -> Vec<LifecycleEvent> {
        let inner = self.inner.read().await;
        inner
            .lifecycle_events
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get recent failures
    pub async fn get_failures(&self, limit: usize) -> Vec<FailureEvent> {
        let inner = self.inner.read().await;
        inner.failures.iter().rev().take(limit).cloned().collect()
    }

    /// Get current router health
    pub async fn get_router_health(&self) -> Option<RouterHealthSnapshot> {
        let inner = self.inner.read().await;
        inner.router_health.clone()
    }

    /// Get startup metadata
    pub async fn get_startup_metadata(&self) -> Option<StartupMetadata> {
        let inner = self.inner.read().await;
        inner.startup_metadata.clone()
    }

    /// Analyze failures and generate test cases
    pub async fn generate_regression_tests(&self) -> Vec<GeneratedTestCase> {
        let inner = self.inner.read().await;
        let mut test_cases = Vec::new();

        // Analyze initialization failures
        for checkpoint in inner.init_checkpoints.iter() {
            if !checkpoint.success {
                test_cases.push(GeneratedTestCase {
                    name: format!("test_init_{:?}_failure", checkpoint.checkpoint),
                    description: format!(
                        "Regression test for {:?} initialization failure",
                        checkpoint.checkpoint
                    ),
                    preconditions: vec!["Node initialization in progress".to_string()],
                    actions: vec![TestAction::InitializeNode {
                        config_hash: "default".to_string(),
                    }],
                    expected_outcomes: vec![format!(
                        "{:?} should complete successfully",
                        checkpoint.checkpoint
                    )],
                    source_event: format!("InitCheckpoint at {}", checkpoint.timestamp),
                });
            }
        }

        // Analyze router integration issues
        if let Some(health) = &inner.router_health {
            if !health.has_local_delivery_channel
                || !health.has_confirmation_callback
                || !health.has_message_sender
            {
                test_cases.push(GeneratedTestCase {
                    name: "test_router_integration_incomplete".to_string(),
                    description: "Router missing required callbacks".to_string(),
                    preconditions: vec!["Node initialized".to_string()],
                    actions: vec![TestAction::CheckRouterHealth],
                    expected_outcomes: vec![
                        "Router should have local delivery channel".to_string(),
                        "Router should have confirmation callback".to_string(),
                        "Router should have message sender callback".to_string(),
                    ],
                    source_event: format!("RouterHealth at {}", health.timestamp),
                });
            }
        }

        // Analyze message routing failures
        for failure in inner.failures.iter() {
            if matches!(failure.failure_type, FailureType::MessageRoutingFailure) {
                test_cases.push(GeneratedTestCase {
                    name: "test_message_routing_failure_regression".to_string(),
                    description: format!("Regression test for: {}", failure.error),
                    preconditions: vec![failure.context.clone()],
                    actions: vec![TestAction::RouteMessage {
                        priority: 128,
                        size: 1024,
                    }],
                    expected_outcomes: vec!["Message should route successfully".to_string()],
                    source_event: format!("Failure at {}", failure.timestamp),
                });
            }
        }

        test_cases
    }

    /// Export diagnostics for test environment
    pub async fn export_for_tests(&self) -> DiagnosticsExport {
        let inner = self.inner.read().await;

        DiagnosticsExport {
            init_checkpoints: inner.init_checkpoints.iter().cloned().collect(),
            recent_events: inner
                .lifecycle_events
                .iter()
                .rev()
                .take(100)
                .cloned()
                .collect(),
            failures: inner.failures.iter().cloned().collect(),
            router_health: inner.router_health.clone(),
            startup_metadata: inner.startup_metadata.clone(),
            generated_tests: std::mem::drop(inner), // Release lock before async call
        }
    }

    /// Check for potential regressions
    pub async fn detect_regressions(&self, baseline: &DiagnosticsBaseline) -> Vec<RegressionAlert> {
        let inner = self.inner.read().await;
        let mut alerts = Vec::new();

        // Check initialization duration regression
        if let Some(metadata) = &inner.startup_metadata {
            if metadata.total_init_duration_ms > baseline.max_init_duration_ms * 2 {
                alerts.push(RegressionAlert {
                    severity: Severity::Warning,
                    category: "Performance".to_string(),
                    message: format!(
                        "Initialization took {}ms, baseline is {}ms (2x slower)",
                        metadata.total_init_duration_ms, baseline.max_init_duration_ms
                    ),
                });
            }
        }

        // Check failure rate regression
        let recent_failures = inner.failures.iter().rev().take(100).count();
        if recent_failures > baseline.max_failures_per_100_events {
            alerts.push(RegressionAlert {
                severity: Severity::Critical,
                category: "Reliability".to_string(),
                message: format!(
                    "Failure rate: {}/100, baseline: {}/100",
                    recent_failures, baseline.max_failures_per_100_events
                ),
            });
        }

        // Check router health regression
        if let Some(health) = &inner.router_health {
            if baseline.require_complete_router_integration
                && (!health.has_local_delivery_channel
                    || !health.has_confirmation_callback
                    || !health.has_message_sender)
            {
                alerts.push(RegressionAlert {
                    severity: Severity::Critical,
                    category: "Integration".to_string(),
                    message: "Router integration incomplete".to_string(),
                });
            }

            if !health.queue_processor_running && baseline.require_queue_processor {
                alerts.push(RegressionAlert {
                    severity: Severity::Critical,
                    category: "Lifecycle".to_string(),
                    message: "Queue processor not running".to_string(),
                });
            }
        }

        alerts
    }
}

/// Exported diagnostics data for test consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsExport {
    pub init_checkpoints: Vec<InitCheckpoint>,
    pub recent_events: Vec<LifecycleEvent>,
    pub failures: Vec<FailureEvent>,
    pub router_health: Option<RouterHealthSnapshot>,
    pub startup_metadata: Option<StartupMetadata>,
    pub generated_tests: (),
}

/// Baseline metrics for regression detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsBaseline {
    pub max_init_duration_ms: u64,
    pub max_failures_per_100_events: usize,
    pub require_complete_router_integration: bool,
    pub require_queue_processor: bool,
    pub require_local_delivery_processor: bool,
}

impl Default for DiagnosticsBaseline {
    fn default() -> Self {
        DiagnosticsBaseline {
            max_init_duration_ms: 5000, // 5 seconds
            max_failures_per_100_events: 5,
            require_complete_router_integration: true,
            require_queue_processor: true,
            require_local_delivery_processor: true,
        }
    }
}

/// Regression alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionAlert {
    pub severity: Severity,
    pub category: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

/// Helper to get current timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl Default for DiagnosticsCollector {
    fn default() -> Self {
        Self::new()
    }
}
