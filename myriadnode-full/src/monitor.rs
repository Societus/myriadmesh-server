use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio::task::JoinHandle;
use tokio::time::{interval, Duration, Instant};
use tracing::{debug, info, warn};

use crate::config::MonitoringConfig;
use crate::storage::Storage;
use myriadmesh_network::AdapterManager;

/// Network performance monitor
pub struct NetworkMonitor {
    config: MonitoringConfig,
    adapter_manager: Arc<RwLock<AdapterManager>>,
    storage: Arc<RwLock<Storage>>,
    // RESOURCE M4: Task handle management for graceful shutdown
    shutdown_tx: broadcast::Sender<()>,
    ping_task: Option<JoinHandle<()>>,
    throughput_task: Option<JoinHandle<()>>,
    reliability_task: Option<JoinHandle<()>>,
}

impl NetworkMonitor {
    pub fn new(
        config: MonitoringConfig,
        adapter_manager: Arc<RwLock<AdapterManager>>,
        storage: Arc<RwLock<Storage>>,
    ) -> Self {
        // RESOURCE M4: Create shutdown channel for graceful task termination
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        Self {
            config,
            adapter_manager,
            storage,
            shutdown_tx,
            ping_task: None,
            throughput_task: None,
            reliability_task: None,
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("Starting network monitoring tasks...");

        // RESOURCE M4: Start ping monitor with shutdown handling
        let ping_interval = self.config.ping_interval_secs;
        let adapter_manager = Arc::clone(&self.adapter_manager);
        let storage = Arc::clone(&self.storage);
        let mut shutdown_rx1 = self.shutdown_tx.subscribe();
        self.ping_task = Some(tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(ping_interval));
            loop {
                tokio::select! {
                    _ = shutdown_rx1.recv() => {
                        debug!("Ping monitor shutting down");
                        break;
                    }
                    _ = ticker.tick() => {
                        if let Err(e) = Self::run_ping_tests(&adapter_manager, &storage).await {
                            warn!("Ping test failed: {}", e);
                        }
                    }
                }
            }
        }));
        debug!("Ping monitor started (interval: {}s)", ping_interval);

        // RESOURCE M4: Start throughput monitor with shutdown handling
        let throughput_interval = self.config.throughput_interval_secs;
        let adapter_manager = Arc::clone(&self.adapter_manager);
        let storage = Arc::clone(&self.storage);
        let mut shutdown_rx2 = self.shutdown_tx.subscribe();
        self.throughput_task = Some(tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(throughput_interval));
            loop {
                tokio::select! {
                    _ = shutdown_rx2.recv() => {
                        debug!("Throughput monitor shutting down");
                        break;
                    }
                    _ = ticker.tick() => {
                        if let Err(e) = Self::run_throughput_tests(&adapter_manager, &storage).await {
                            warn!("Throughput test failed: {}", e);
                        }
                    }
                }
            }
        }));
        debug!(
            "Throughput monitor started (interval: {}s)",
            throughput_interval
        );

        // RESOURCE M4: Start reliability monitor with shutdown handling
        let reliability_interval = self.config.reliability_interval_secs;
        let adapter_manager = Arc::clone(&self.adapter_manager);
        let storage = Arc::clone(&self.storage);
        let mut shutdown_rx3 = self.shutdown_tx.subscribe();
        self.reliability_task = Some(tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(reliability_interval));
            loop {
                tokio::select! {
                    _ = shutdown_rx3.recv() => {
                        debug!("Reliability monitor shutting down");
                        break;
                    }
                    _ = ticker.tick() => {
                        if let Err(e) = Self::run_reliability_tests(&adapter_manager, &storage).await {
                            warn!("Reliability test failed: {}", e);
                        }
                    }
                }
            }
        }));
        debug!(
            "Reliability monitor started (interval: {}s)",
            reliability_interval
        );

        Ok(())
    }

    /// Gracefully shutdown all monitoring tasks and wait for completion
    /// RESOURCE M4: Prevents task handle leaks and ensures cleanup
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping network monitoring tasks...");

        // Send shutdown signal to all tasks
        let _ = self.shutdown_tx.send(());

        // Wait for each task to complete
        if let Some(task) = self.ping_task.take() {
            let _ = task.await;
        }

        if let Some(task) = self.throughput_task.take() {
            let _ = task.await;
        }

        if let Some(task) = self.reliability_task.take() {
            let _ = task.await;
        }

        Ok(())
    }

    async fn run_ping_tests(
        adapter_manager: &Arc<RwLock<AdapterManager>>,
        storage: &Arc<RwLock<Storage>>,
    ) -> Result<()> {
        debug!("Running ping tests...");

        let manager = adapter_manager.read().await;
        let adapter_ids = manager.adapter_ids();

        for adapter_id in adapter_ids {
            let start = Instant::now();

            // Attempt a simple connection test
            match manager.get_adapter(&adapter_id) {
                Some(adapter) => {
                    let status = adapter.read().await.get_status();
                    let latency = start.elapsed();

                    debug!(
                        "Adapter '{}': status={:?}, latency={:?}",
                        adapter_id, status, latency
                    );

                    // Store ping metrics in database
                    let storage_guard = storage.read().await;
                    if let Err(e) = storage_guard
                        .store_metrics(
                            &adapter_id,
                            "ping_test",
                            Some(latency.as_secs_f64() * 1000.0), // Convert to milliseconds
                            None,
                            None,
                        )
                        .await
                    {
                        warn!("Failed to store ping metrics for '{}': {}", adapter_id, e);
                    }
                }
                None => {
                    warn!("Adapter '{}' not found during ping test", adapter_id);
                }
            }
        }

        Ok(())
    }

    async fn run_throughput_tests(
        adapter_manager: &Arc<RwLock<AdapterManager>>,
        storage: &Arc<RwLock<Storage>>,
    ) -> Result<()> {
        debug!("Running throughput tests...");

        let manager = adapter_manager.read().await;
        let adapter_ids = manager.adapter_ids();

        for adapter_id in adapter_ids {
            match manager.get_adapter(&adapter_id) {
                Some(adapter) => {
                    let adapter_guard = adapter.read().await;
                    let capabilities = adapter_guard.get_capabilities();

                    debug!(
                        "Adapter '{}': bandwidth={} bps, latency={} ms",
                        adapter_id,
                        capabilities.typical_bandwidth_bps,
                        capabilities.typical_latency_ms
                    );

                    // Store throughput metrics in database
                    // For now, use capability-based metrics until we implement actual throughput testing
                    let storage_guard = storage.read().await;
                    if let Err(e) = storage_guard
                        .store_metrics(
                            &adapter_id,
                            "throughput_test",
                            Some(capabilities.typical_latency_ms),
                            Some(capabilities.typical_bandwidth_bps as f64),
                            None,
                        )
                        .await
                    {
                        warn!(
                            "Failed to store throughput metrics for '{}': {}",
                            adapter_id, e
                        );
                    }

                    // TODO: Perform actual throughput test by sending test frames
                    // This would involve sending a series of test frames and measuring actual bandwidth
                }
                None => {
                    warn!("Adapter '{}' not found during throughput test", adapter_id);
                }
            }
        }

        Ok(())
    }

    async fn run_reliability_tests(
        adapter_manager: &Arc<RwLock<AdapterManager>>,
        storage: &Arc<RwLock<Storage>>,
    ) -> Result<()> {
        debug!("Running reliability tests...");

        let manager = adapter_manager.read().await;
        let adapter_ids = manager.adapter_ids();

        for adapter_id in adapter_ids {
            match manager.get_adapter(&adapter_id) {
                Some(adapter) => {
                    let adapter_guard = adapter.read().await;
                    let status = adapter_guard.get_status();
                    let capabilities = adapter_guard.get_capabilities();

                    debug!(
                        "Adapter '{}': status={:?}, reliability={}",
                        adapter_id, status, capabilities.reliability
                    );

                    // Store reliability metrics in database
                    // For now, use capability-based metrics until we implement actual packet loss testing
                    let storage_guard = storage.read().await;
                    if let Err(e) = storage_guard
                        .store_metrics(
                            &adapter_id,
                            "reliability_test",
                            None,
                            None,
                            Some(capabilities.reliability),
                        )
                        .await
                    {
                        warn!(
                            "Failed to store reliability metrics for '{}': {}",
                            adapter_id, e
                        );
                    }

                    // TODO: Perform packet loss test
                    // This would involve sending test packets and measuring loss rate
                }
                None => {
                    warn!("Adapter '{}' not found during reliability test", adapter_id);
                }
            }
        }

        Ok(())
    }
}
