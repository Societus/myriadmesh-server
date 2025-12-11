// MyriadNode library - exposes modules for integration testing and potential reuse
//
// This library provides the core components of MyriadNode, allowing them to be
// tested via integration tests and potentially reused by other applications.

pub mod api;
pub mod backhaul;
pub mod config;
pub mod diagnostics;
pub mod failover;
pub mod heartbeat;
pub mod monitor;
pub mod node;
pub mod scoring;
pub mod storage;
pub mod websocket;

// Re-export commonly used types for convenience
pub use config::Config;
pub use diagnostics::DiagnosticsCollector;
pub use node::Node;
pub use scoring::{AdapterScorer, ScoringWeights};
