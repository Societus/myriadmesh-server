# Emergency Abuse Prevention - Full Node Implementation

**Status**: Ready for Implementation (Core complete!)
**Estimated Time**: 1.5-2 hours
**Last Updated**: 2025-12-08

## Overview
Integrate emergency abuse prevention components into the full node implementation with configuration, monitoring, and API support.

**✅ Dependencies Complete**:
- EmergencyManager (myriadmesh-core)
- ConsensusValidator (myriadmesh-core)
- BandwidthMonitor (myriadmesh-core)
- Router Integration (myriadmesh-core)

## Implementation Checklist

### Phase 1: Configuration (Week 1)

#### 1.1 Emergency Configuration Schema

**File**: `myriadnode-full/src/config.rs`

- [ ] Add `EmergencyConfig` struct:
  ```rust
  #[derive(Debug, Clone, Deserialize)]
  pub struct EmergencyConfig {
      pub enabled: bool,
      pub size_modulation_enabled: bool,
      pub infrequent_allowance: u32,
      pub high_reputation_threshold: f64,

      // Bandwidth-aware exemption
      pub bandwidth_exemption_enabled: bool,
      pub high_speed_threshold_bps: u64,
      pub unused_bandwidth_threshold: f64,
      pub bandwidth_sampling_window_secs: u64,

      // Consensus validation
      pub consensus: Option<ConsensusConfig>,

      // Trusted authorities
      pub trusted_authorities: Vec<TrustedAuthority>,
  }

  #[derive(Debug, Clone, Deserialize)]
  pub struct ConsensusConfig {
      pub enabled: bool,
      pub required_confirmations: u32,  // K
      pub total_validators: u32,        // N
      pub timeout_secs: u64,
      pub use_dht_discovery: bool,
  }

  #[derive(Debug, Clone, Deserialize)]
  pub struct TrustedAuthority {
      pub key_id: String,
      pub description: String,
  }
  ```

- [ ] Add field to main `Config` struct:
  ```rust
  pub struct Config {
      // ... existing fields
      pub emergency: Option<EmergencyConfig>,
  }
  ```

- [ ] Implement default values:
  ```rust
  impl Default for EmergencyConfig {
      fn default() -> Self {
          Self {
              enabled: false,  // Opt-in feature
              size_modulation_enabled: true,
              infrequent_allowance: 3,
              high_reputation_threshold: 0.8,
              bandwidth_exemption_enabled: true,
              high_speed_threshold_bps: 10_000_000,
              unused_bandwidth_threshold: 0.6,
              bandwidth_sampling_window_secs: 60,
              consensus: None,
              trusted_authorities: Vec::new(),
          }
      }
  }
  ```

**Estimated Effort**: 1 day

---

#### 1.2 Configuration File Template

**File**: `myriadnode-full/config.toml`

- [ ] Add emergency configuration section:
  ```toml
  # Emergency Message Abuse Prevention
  [emergency]
  enabled = true
  size_modulation_enabled = true
  infrequent_allowance = 3              # First N emergencies/day bypass quotas
  high_reputation_threshold = 0.8       # Nodes with score >0.8 bypass quotas

  # Bandwidth-aware exemption (Individual/Family realms on high-speed, low-utilization adapters)
  bandwidth_exemption_enabled = true
  high_speed_threshold_bps = 10_000_000     # 10 Mbps (Wi-Fi HaLow minimum)
  unused_bandwidth_threshold = 0.6          # 60% unused = 40% utilized
  bandwidth_sampling_window_secs = 60       # 1-minute rolling window

  # Consensus validation for Global realm emergencies
  [emergency.consensus]
  enabled = true
  required_confirmations = 3  # K-of-N: require K confirmations
  total_validators = 5        # N: query N validators
  timeout_secs = 10
  use_dht_discovery = true

  # Trusted emergency authorities (optional)
  [[emergency.trusted_authorities]]
  key_id = "0x1234abcd..."
  description = "National Emergency Authority"
  ```

- [ ] Add comments explaining each option
- [ ] Document example configurations (minimal, balanced, full)

**Estimated Effort**: 0.5 days

---

### Phase 2: Component Initialization (Week 2)

#### 2.1 BandwidthMonitor Setup

**File**: `myriadnode-full/src/node.rs`

- [ ] Add BandwidthMonitor to node state:
  ```rust
  pub struct Node {
      // ... existing fields
      bandwidth_monitor: Option<Arc<BandwidthMonitor>>,
  }
  ```

- [ ] Initialize BandwidthMonitor in `new()`:
  ```rust
  let bandwidth_monitor = if config.emergency.as_ref()
      .map(|e| e.bandwidth_exemption_enabled)
      .unwrap_or(false)
  {
      let bw_config = BandwidthMonitorConfig {
          high_speed_threshold_bps: config.emergency.as_ref().unwrap().high_speed_threshold_bps,
          unused_threshold: config.emergency.as_ref().unwrap().unused_bandwidth_threshold,
          sampling_window_secs: config.emergency.as_ref().unwrap().bandwidth_sampling_window_secs,
      };
      Some(Arc::new(BandwidthMonitor::new(bw_config)))
  } else {
      None
  };
  ```

- [ ] Connect bandwidth monitor to adapter manager:
  ```rust
  if let Some(bw_monitor) = &bandwidth_monitor {
      let bw_monitor_clone = Arc::clone(bw_monitor);
      adapter_manager.set_bandwidth_callback(Arc::new(move |adapter_id, bytes, direction| {
          bw_monitor_clone.record_transfer(&adapter_id, bytes, direction);
      }));
  }
  ```

**Estimated Effort**: 2 days

---

#### 2.2 EmergencyManager Setup

**File**: `myriadnode-full/src/node.rs`

- [ ] Add EmergencyManager to node state:
  ```rust
  pub struct Node {
      // ... existing fields
      emergency_manager: Option<Arc<EmergencyManager>>,
  }
  ```

- [ ] Initialize EmergencyManager in `new()`:
  ```rust
  let emergency_manager = if let Some(emergency_config) = &config.emergency {
      if emergency_config.enabled {
          let em_config = EmergencyManagerConfig {
              enabled: true,
              size_modulation_enabled: emergency_config.size_modulation_enabled,
              infrequent_allowance: emergency_config.infrequent_allowance,
              high_reputation_threshold: emergency_config.high_reputation_threshold,
              bandwidth_exemption_enabled: emergency_config.bandwidth_exemption_enabled,
              high_speed_threshold_bps: emergency_config.high_speed_threshold_bps,
              unused_bandwidth_threshold: emergency_config.unused_bandwidth_threshold,
          };

          let consensus_validator = if let Some(consensus_config) = &emergency_config.consensus {
              // Initialize ConsensusValidator
              Some(Arc::new(ConsensusValidator::new(
                  consensus_config.clone(),
                  router.get_dht(),
                  message_sender.clone(),
              )))
          } else {
              None
          };

          Some(Arc::new(EmergencyManager::new(
              em_config,
              Some(Arc::clone(&reputation)),
              consensus_validator,
              bandwidth_monitor.clone(),
          )))
      } else {
          None
      }
  } else {
      None
  };
  ```

- [ ] Set EmergencyManager on Router:
  ```rust
  if let Some(em) = &emergency_manager {
      router.set_emergency_manager(Arc::clone(em));
  }
  ```

**Estimated Effort**: 2 days

---

#### 2.3 Message Sender Callback Enhancement

**File**: `myriadnode-full/src/node.rs`

- [ ] Update `create_message_sender()` to support bandwidth tracking (lines ~403-449):
  ```rust
  fn create_message_sender(
      adapter_manager: Arc<RwLock<AdapterManager>>,
      failover_manager: Arc<FailoverManager>,
      bandwidth_monitor: Option<Arc<BandwidthMonitor>>,  // NEW parameter
  ) -> MessageSenderCallback {
      Arc::new(move |destination: NodeId, message: Message| {
          let adapter_manager = Arc::clone(&adapter_manager);
          let failover_manager = Arc::clone(&failover_manager);
          let bandwidth_monitor = bandwidth_monitor.clone();  // NEW

          Box::pin(async move {
              let priority = message.priority.as_u8();

              // Select best adapter
              let selected_adapter_id = { /* ... existing logic ... */ };

              let adapter_id = selected_adapter_id.ok_or_else(|| {
                  "No adapters available for message transmission".to_string()
              })?;

              // NEW: Validate emergency payload size for ultra-low BW adapters
              {
                  let mgr = adapter_manager.read().await;
                  mgr.validate_emergency_payload(&adapter_id, priority, message.payload.len())
                      .map_err(|e| format!("Emergency validation failed: {}", e))?;
              }

              // Encode and send
              let encoded = bincode::serialize(&message).map_err(|e| {
                  format!("Failed to encode message: {}", e)
              })?;

              {
                  let mgr = adapter_manager.read().await;
                  mgr.send_to(&adapter_id, &destination, &encoded).await.map_err(|e| {
                      format!("Adapter send failed: {}", e)
                  })?;
              }

              // NEW: Bandwidth tracking (already done in adapter_manager via callback)
              // No need to call here - callback is invoked in send_to()

              Ok(())
          }) as Pin<Box<dyn Future<Output = Result<(), String>> + Send>>
      })
  }
  ```

- [ ] Update call site to pass bandwidth_monitor:
  ```rust
  let message_sender = Self::create_message_sender(
      Arc::clone(&adapter_manager),
      Arc::clone(&failover_manager),
      bandwidth_monitor.clone(),  // NEW
  );
  ```

**Estimated Effort**: 1 day

---

### Phase 3: API Integration (Week 3)

#### 3.1 Emergency Message API Endpoint

**File**: `myriadnode-full/src/api.rs`

- [ ] Update `SendMessageRequest` to support emergency realm:
  ```rust
  #[derive(Debug, Deserialize)]
  pub struct SendMessageRequest {
      pub destination: String,
      pub payload: String,
      pub priority: Option<u8>,
      pub emergency_realm: Option<EmergencyRealmRequest>,  // NEW
  }

  #[derive(Debug, Deserialize)]
  pub struct EmergencyRealmRequest {
      pub realm: u8,  // 0-5
      pub destination_count: u32,
      pub authority_signature: Option<String>,  // Hex-encoded
      pub authority_key_id: Option<String>,     // Hex-encoded
  }
  ```

- [ ] Update `send_message` handler to construct EmergencyRealm:
  ```rust
  // Convert API request to protocol EmergencyRealm
  let emergency_realm = request.emergency_realm.map(|er| {
      myriadmesh_protocol::EmergencyRealm {
          realm: RealmTier::from_u8(er.realm).unwrap(),
          destination_count: er.destination_count,
          authority_signature: er.authority_signature.and_then(|s| hex::decode(s).ok()),
          authority_key_id: er.authority_key_id.and_then(|s| {
              hex::decode(s).ok().and_then(|v| v.try_into().ok())
          }),
      }
  });

  let message = ProtocolMessage {
      // ... existing fields
      emergency_realm,
  };
  ```

- [ ] Add validation in send_message handler:
  ```rust
  // Validate priority matches emergency realm
  if let Some(ref realm) = message.emergency_realm {
      if priority_u8 < 224 {
          return Err((
              StatusCode::BAD_REQUEST,
              "Emergency realm requires priority ≥224".to_string(),
          ).into());
      }
  }
  ```

**Estimated Effort**: 2 days

---

#### 3.2 Emergency Statistics API

**File**: `myriadnode-full/src/api.rs`

- [ ] Add `/stats/emergency` endpoint:
  ```rust
  async fn get_emergency_stats(
      State(state): State<Arc<ApiState>>,
  ) -> Result<Json<EmergencyStatsResponse>, StatusCode> {
      if let Some(em) = &state.emergency_manager {
          let stats = em.get_stats().await;
          Ok(Json(EmergencyStatsResponse {
              total_validated: stats.total_validated,
              quota_violations: stats.quota_violations,
              downgrades: stats.downgrades,
              bandwidth_exemptions: stats.bandwidth_exemptions,
              consensus_requests: stats.consensus_requests,
              by_realm: stats.by_realm,
          }))
      } else {
          Err(StatusCode::NOT_FOUND)
      }
  }
  ```

- [ ] Add to router:
  ```rust
  .route("/stats/emergency", get(get_emergency_stats))
  ```

**Estimated Effort**: 1 day

---

#### 3.3 Bandwidth Utilization API

**File**: `myriadnode-full/src/api.rs`

- [ ] Add `/stats/bandwidth` endpoint:
  ```rust
  async fn get_bandwidth_stats(
      State(state): State<Arc<ApiState>>,
  ) -> Result<Json<BandwidthStatsResponse>, StatusCode> {
      if let Some(bw) = &state.bandwidth_monitor {
          let adapters = state.adapter_manager.read().await.adapter_ids();
          let mut stats = HashMap::new();

          for adapter_id in adapters {
              if let Some(caps) = state.adapter_manager.read().await.get_adapter_capabilities(&adapter_id).ok() {
                  if let Some(utilization) = bw.get_utilization(&adapter_id, caps.max_bandwidth_bps) {
                      stats.insert(adapter_id, AdapterBandwidthInfo {
                          max_bandwidth_bps: caps.max_bandwidth_bps,
                          current_utilization: utilization.current_utilization,
                          is_high_speed: utilization.is_high_speed,
                          has_unused_capacity: utilization.has_unused_capacity,
                      });
                  }
              }
          }

          Ok(Json(BandwidthStatsResponse { adapters: stats }))
      } else {
          Err(StatusCode::NOT_FOUND)
      }
  }
  ```

- [ ] Add to router:
  ```rust
  .route("/stats/bandwidth", get(get_bandwidth_stats))
  ```

**Estimated Effort**: 1 day

---

### Phase 4: Monitoring & Metrics (Week 4)

#### 4.1 Prometheus Metrics

**File**: `myriadnode-full/src/metrics.rs` (NEW or existing)

- [ ] Add emergency metrics:
  ```rust
  lazy_static! {
      static ref EMERGENCY_MESSAGES_VALIDATED: IntCounter = register_int_counter!(
          "emergency_messages_validated_total",
          "Total emergency messages validated"
      ).unwrap();

      static ref EMERGENCY_QUOTA_VIOLATIONS: IntCounter = register_int_counter!(
          "emergency_quota_violations_total",
          "Emergency messages that exceeded quota"
      ).unwrap();

      static ref EMERGENCY_DOWNGRADES: IntCounter = register_int_counter!(
          "emergency_downgrades_total",
          "Emergency messages downgraded to High priority"
      ).unwrap();

      static ref EMERGENCY_BANDWIDTH_EXEMPTIONS: IntCounter = register_int_counter!(
          "emergency_bandwidth_exemptions_total",
          "Individual/Family emergencies bypassed via bandwidth exemption"
      ).unwrap();

      static ref EMERGENCY_CONSENSUS_REQUESTS: IntCounter = register_int_counter!(
          "emergency_consensus_requests_total",
          "Global realm consensus requests"
      ).unwrap();

      static ref EMERGENCY_CONSENSUS_LATENCY: Histogram = register_histogram!(
          "emergency_consensus_latency_seconds",
          "Consensus validation latency"
      ).unwrap();

      static ref ADAPTER_BANDWIDTH_UTILIZATION: GaugeVec = register_gauge_vec!(
          "adapter_bandwidth_utilization",
          "Current bandwidth utilization per adapter (0.0-1.0)",
          &["adapter_id"]
      ).unwrap();
  }
  ```

- [ ] Update metrics from EmergencyManager and BandwidthMonitor
- [ ] Add periodic metrics updater task

**Estimated Effort**: 2 days

---

#### 4.2 Logging Integration

**File**: Various

- [ ] Add structured logging for emergency events:
  - [ ] Emergency validation (info level)
  - [ ] Bandwidth exemption granted (info level)
  - [ ] Quota violation (warn level)
  - [ ] Emergency downgrade (warn level)
  - [ ] Consensus request (debug level)
  - [ ] Consensus result (info level)

- [ ] Use tracing spans for emergency message lifecycle

**Estimated Effort**: 1 day

---

### Phase 5: Testing (Week 5-6)

#### 5.1 Integration Tests

**File**: `myriadnode-full/tests/emergency_abuse_prevention.rs` (NEW)

- [ ] Test complete emergency message flow:
  - [ ] Individual realm emergency via API → Router → validation → send
  - [ ] Family realm with bandwidth exemption
  - [ ] Group realm quota enforcement
  - [ ] Global realm consensus requirement

- [ ] Test configuration scenarios:
  - [ ] Emergency disabled → no validation
  - [ ] Bandwidth exemption disabled → quotas enforced
  - [ ] Consensus disabled → Global realm rejected

- [ ] Test edge cases:
  - [ ] Ultra-low BW adapter + large emergency → rejected
  - [ ] High reputation node → quota bypass
  - [ ] Infrequent usage → first 3/day allowed

**Estimated Effort**: 4 days

---

#### 5.2 End-to-End Tests

- [ ] Test multi-node emergency propagation
- [ ] Test consensus validation across nodes
- [ ] Test bandwidth exemption in mesh network scenario
- [ ] Test reputation decay after repeated abuse

**Estimated Effort**: 3 days

---

### Phase 6: Documentation (Week 7)

#### 6.1 User Documentation

- [ ] Create `docs/emergency_abuse_prevention.md`:
  - [ ] Feature overview
  - [ ] Configuration guide
  - [ ] Realm tier explanation
  - [ ] Bandwidth exemption criteria
  - [ ] API usage examples
  - [ ] Monitoring and metrics

- [ ] Update main README with emergency feature

**Estimated Effort**: 2 days

---

#### 6.2 Operator Guide

- [ ] Create `docs/operators/emergency_configuration.md`:
  - [ ] Recommended configurations (minimal, balanced, full)
  - [ ] Security best practices
  - [ ] Tuning guidance (quotas, thresholds)
  - [ ] Troubleshooting common issues
  - [ ] Performance tuning

**Estimated Effort**: 2 days

---

### Phase 7: Deployment (Week 8)

#### 7.1 Migration Guide

- [ ] Create migration guide for existing deployments
- [ ] Document configuration changes
- [ ] Provide example upgrade path

**Estimated Effort**: 1 day

---

#### 7.2 Staged Rollout Plan

- [ ] Phase 1: Deploy with emergency.enabled = false (1% of nodes)
- [ ] Phase 2: Enable for trusted nodes (10% of nodes)
- [ ] Phase 3: Gradual rollout (50% of nodes)
- [ ] Phase 4: Full deployment (100% of nodes)
- [ ] Monitor metrics at each stage

**Estimated Effort**: 1 day planning + ongoing monitoring

---

## Dependencies

- **myriadmesh-protocol**: EmergencyRealm message metadata
- **myriadmesh-core**: EmergencyManager, BandwidthMonitor, ConsensusValidator
- **myriadmesh-node**: Adapter bandwidth capabilities and tracking

## Estimated Total Effort
6-7 weeks for complete implementation, testing, and documentation

## Critical Success Criteria

✅ Configuration-driven deployment (disabled by default)
✅ Bandwidth exemption works for neighborhood Wi-Fi scenarios
✅ API supports emergency realm specification
✅ Metrics available for monitoring
✅ Documentation complete
✅ No performance regression (<5% overhead)
✅ Backwards compatible with existing nodes

## Related Files

- `myriadnode-full/src/config.rs` - Configuration schema
- `myriadnode-full/src/node.rs` - Component initialization
- `myriadnode-full/src/api.rs` - API endpoints
- `myriadnode-full/config.toml` - Configuration template
- `myriadnode-full/tests/emergency_abuse_prevention.rs` - Integration tests

## Rollback Plan

If issues arise during deployment:
1. Set `emergency.enabled = false` in config
2. Restart node (no code changes required)
3. Emergency messages will be treated as normal High priority
4. No data loss or protocol incompatibility

## Monitoring Checklist

Monitor these metrics during rollout:
- [ ] `emergency_messages_validated_total` - Should increase gradually
- [ ] `emergency_downgrades_total` - Should be <5% of validated
- [ ] `emergency_bandwidth_exemptions_total` - Should track Individual/Family on Wi-Fi
- [ ] `emergency_consensus_latency_seconds` - Should be <1s p99
- [ ] `adapter_bandwidth_utilization` - Should show realistic utilization
- [ ] CPU usage - Should increase <5%
- [ ] Memory usage - Should increase <10MB per 10K tracked senders

## Notes

- Emergency feature is opt-in (disabled by default)
- Configuration is hot-reloadable (restart required)
- Backwards compatible with old protocol (optional field)
- Can be deployed incrementally across mesh network
- Performance overhead is minimal (<5μs per emergency message)
