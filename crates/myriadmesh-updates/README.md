# MyriadMesh Updates

Coordinated update scheduling and peer-assisted update distribution system for MyriadMesh.

## Overview

This crate implements **Phase 4.6** (Coordinated Update Scheduling) and **Phase 4.7** (Peer-Assisted Update Distribution) of the MyriadMesh roadmap.

### Key Features

#### Phase 4.6: Coordinated Update Scheduling
- **Optimal Window Selection**: Automatically finds the best time to perform updates based on network load, peer schedules, and time of day
- **Neighbor Notification**: Coordinates with neighbors before updates to minimize disruption
- **Fallback Path Establishment**: Ensures alternative communication paths are available during updates
- **Conflict Resolution**: Handles schedule conflicts through negotiation and rescheduling

#### Phase 4.7: Peer-Assisted Update Distribution
- **Multi-Signature Verification**: Requires 3+ trusted peer signatures for peer-forwarded updates
- **6-Hour Verification Window**: Non-critical updates wait for community verification before installation
- **Critical CVE Override**: Security-critical updates bypass the verification period
- **Update Forwarding**: Distributes updates across the mesh network with signature chains
- **Payload Integrity**: BLAKE2b-512 hashing ensures update packages aren't tampered with

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                  UpdateCoordinator                       │
│                                                           │
│  ┌────────────────────┐  ┌──────────────────────────┐   │
│  │ Schedule Manager   │  │  Distribution Manager    │   │
│  │ - Pending schedules│  │  - Pending updates       │   │
│  │ - Request tracking │  │  - Verification tracking │   │
│  │ - Response mgmt    │  │  - Installation queue    │   │
│  └────────────────────┘  └──────────────────────────┘   │
│                                                           │
│  ┌────────────────────┐  ┌──────────────────────────┐   │
│  │ Window Selector    │  │  Signature Verifier      │   │
│  │ - Optimal timing   │  │  - Multi-sig verification│   │
│  │ - Conflict detection│  │  - Reputation checking  │   │
│  │ - Network load     │  │  - Hash validation       │   │
│  └────────────────────┘  └──────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

## Usage

### Creating a Coordinator

```rust
use myriadmesh_updates::UpdateCoordinator;
use myriadmesh_crypto::identity::Identity;
use std::sync::Arc;

let identity = Arc::new(Identity::new());
let coordinator = UpdateCoordinator::new(identity);
```

### Scheduling an Update (Phase 4.6)

```rust
use myriadmesh_protocol::types::AdapterType;
use myriadmesh_network::version_tracking::SemanticVersion;
use std::time::Duration;

// Register current adapter configuration
coordinator.register_adapter_version(
    AdapterType::Ethernet,
    SemanticVersion::new(1, 0, 0)
).await;

coordinator.register_available_adapters(
    [AdapterType::Ethernet, AdapterType::WiFi].into_iter().collect()
).await;

// Schedule an update
let schedule = coordinator.schedule_adapter_update(
    AdapterType::Ethernet,
    SemanticVersion::new(1, 1, 0),
    Duration::from_secs(60), // Estimated duration
).await?;

println!("Update scheduled for: {}", schedule.scheduled_start);
println!("Fallback adapters: {:?}", schedule.fallback_adapters);
```

### Handling Update Requests from Neighbors

```rust
// When receiving an update schedule request from a neighbor
let response = coordinator.handle_schedule_request(schedule).await?;

match response {
    UpdateScheduleResponse::Acknowledged { .. } => {
        println!("Update schedule acknowledged");
    }
    UpdateScheduleResponse::Reschedule { proposed_start, reason, .. } => {
        println!("Requesting reschedule to {} because: {}", proposed_start, reason);
    }
    UpdateScheduleResponse::Rejected { reason, .. } => {
        println!("Update schedule rejected: {}", reason);
    }
}
```

### Receiving and Verifying Update Packages (Phase 4.7)

```rust
use myriadmesh_updates::verification::SignatureVerifier;

// Set up signature verifier
let verifier = Arc::new(SignatureVerifier::new(
    |node_id| get_public_key(node_id),    // Your function to get public keys
    |node_id| get_reputation(node_id),    // Your function to get reputation
));

coordinator.set_verifier(verifier).await;

// Receive an update package
coordinator.receive_update_package(package).await?;

// Get updates ready to install
let ready_updates = coordinator.get_ready_updates().await;

for update in ready_updates {
    if update.is_critical_security_update() {
        println!("Critical security update available!");
    }
    // Proceed with installation...
}
```

### Forwarding Updates to Neighbors

```rust
// Forward an update package to trusted neighbors
coordinator.forward_update_package(package, neighbors).await?;
```

## Update Window Selection Algorithm

The optimal update window is selected based on multiple factors:

```
Base Score: 100

Modifiers:
+ 50 points: Off-peak hours (2am-5am UTC)
+ 30 points: Late night (10pm-1am UTC)
- 20 points: Business hours (9am-5pm UTC)
- 40 points: Conflicts with neighbor maintenance
- 30 points: High network load (proportional)
- 0.5 points per hour delay (prefer sooner for security)
```

## Security Model

### Signature Verification

**Official Updates**:
- Verified by publisher signature OR
- 3+ trusted peer signatures (reputation ≥ 0.8)

**Peer-Forwarded Updates**:
- Requires exactly 3+ trusted peer signatures
- Each signature verified with Ed25519
- Reputation threshold: 0.8 or higher

### Verification Period

**Non-Critical Updates**:
- 6-hour verification window
- Requires 5+ peer verifications
- Allows community validation

**Critical Security Updates**:
- Bypass verification period
- Still require 3+ signatures
- Installed immediately after signature verification

### Payload Integrity

- BLAKE2b-512 hash of payload
- Verified before processing
- Tampered packages rejected immediately

## Constants

```rust
/// Verification period for non-critical updates
pub const VERIFICATION_PERIOD: Duration = Duration::from_secs(6 * 60 * 60); // 6 hours

/// Minimum number of peer verifications required
pub const MIN_VERIFICATIONS: usize = 5;

/// Minimum number of trusted signatures
pub const MIN_TRUSTED_SIGNATURES: usize = 3;

/// Minimum reputation to be considered trusted
pub const MIN_TRUST_REPUTATION: f64 = 0.8;
```

## Examples

See the `tests` modules in each source file for comprehensive examples:
- `schedule.rs` - Update scheduling examples
- `distribution.rs` - Package distribution examples
- `verification.rs` - Signature verification examples
- `coordination.rs` - End-to-end coordinator examples

## Integration with Existing Systems

This crate integrates with:
- `myriadmesh-network`: Uses hot-reload infrastructure and version tracking
- `myriadmesh-crypto`: Uses Ed25519 signatures and identity management
- `myriadmesh-protocol`: Uses NodeId and AdapterType definitions
- `myriadmesh-dht`: Can leverage DHT for update discovery (future enhancement)

## Testing

Run tests with:

```bash
cargo test -p myriadmesh-updates
```

All core functionality has comprehensive test coverage:
- ✅ Schedule overlap detection
- ✅ Window conflict detection
- ✅ Response tracking
- ✅ Payload hash verification
- ✅ Critical CVE detection
- ✅ Verification period logic
- ✅ Signature verification
- ✅ Distribution manager operations

## Future Enhancements

- [ ] DHT-based update announcement
- [ ] Bandwidth-aware distribution (avoid flooding)
- [ ] Update rollback on mass failures
- [ ] Differential updates (binary diffs)
- [ ] Update package compression
- [ ] Cross-version compatibility matrix
- [ ] Automated CVE database integration
- [ ] Update analytics and telemetry

## License

GPL-3.0-only - See LICENSE for details
