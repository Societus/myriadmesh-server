# MyriadMesh Server Repository Roadmap

**Status:** Blocked - Waiting for Core and Node
**Last Updated:** 2025-12-07
**Priority:** #4 of 5 repositories
**Target:** REST API, web dashboard, ledger for mesh management

---

## Strategic Position

Server cannot provide real functionality until:
1. **Core** provides Router for message transmission
2. **Node** provides adapters for network connectivity

However, **API stubs and web UI scaffolding can start now** to reduce integration time later.

**Recommended approach:**
1. Finalize API design now (doesn't require Core/Node)
2. Build web UI against mock data
3. Integrate when Core/Node ready

---

## Current State Assessment

### What's Complete ✅
- `myriadnode-full/src/api.rs` - Axum API skeleton with route stubs
- `myriadnode-full/src/main.rs` - Server entry point
- `web-ui/` - Svelte project structure
- `crates/myriadmesh-ledger/` - Ledger module structure
- `crates/myriadmesh-appliance/` - Appliance management structure
- `crates/myriadmesh-updates/` - Update distribution structure

### What's Missing (Blocked) ❌
- Message send API integration (needs Router)
- DHT status API (needs DHT)
- Real-time metrics (needs adapters)
- Ledger event hooks (needs message flow)

### What Can Start Now ✅
- API endpoint design and documentation
- Web UI components with mock data
- Configuration management endpoints
- Static content serving

---

## Phase 0: API Design & Mock UI [Can Start Now]

**Estimated Effort:** 30-40 hours
**Dependencies:** None
**This reduces integration time later**

### Task 0.1: API Specification

**File:** `docs/api.md` (create)
**Effort:** 8-10 hours

Define all endpoints before implementation:

```yaml
openapi: 3.0.0
info:
  title: MyriadMesh Server API
  version: 1.0.0

paths:
  /api/status:
    get:
      summary: Node health and version
      responses:
        200:
          content:
            application/json:
              schema:
                type: object
                properties:
                  version: { type: string }
                  uptime: { type: integer }
                  node_id: { type: string }
                  adapters_active: { type: integer }
                  peers_connected: { type: integer }

  /api/messages/send:
    post:
      summary: Send a message
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                destination: { type: string }
                payload: { type: string }
                ttl: { type: integer }
      responses:
        202:
          description: Message queued

  /api/messages/inbox:
    get:
      summary: Get received messages
      parameters:
        - name: limit
          in: query
          schema: { type: integer, default: 50 }
        - name: since
          in: query
          schema: { type: string, format: date-time }

  /api/adapters:
    get:
      summary: List adapters and their status

  /api/adapters/{id}/enable:
    post:
      summary: Enable an adapter

  /api/adapters/{id}/disable:
    post:
      summary: Disable an adapter

  /api/peers:
    get:
      summary: List known peers

  /api/config:
    get:
      summary: Get configuration
    put:
      summary: Update configuration

  /api/logs:
    get:
      summary: Get recent logs

  /api/logs/stream:
    get:
      summary: WebSocket stream of logs
```

**Implementation Checklist:**
- [ ] Document all endpoints with request/response schemas
- [ ] Define error response format
- [ ] Specify authentication (if any)
- [ ] Create OpenAPI spec file

### Task 0.2: Mock API Responses

**File:** `myriadnode-full/src/api.rs`
**Effort:** 10-12 hours

Implement endpoints that return realistic mock data:

```rust
async fn get_status() -> Json<Status> {
    Json(Status {
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime: 3600, // Mock: 1 hour
        node_id: "mock_node_abc123".to_string(),
        adapters_active: 2,
        peers_connected: 5,
    })
}

async fn list_adapters() -> Json<Vec<AdapterInfo>> {
    Json(vec![
        AdapterInfo {
            id: "udp".to_string(),
            adapter_type: "UDP".to_string(),
            enabled: true,
            metrics: AdapterMetrics {
                bytes_sent: 1024000,
                bytes_received: 2048000,
                messages_sent: 100,
                messages_received: 150,
                avg_latency_ms: 45,
            },
        },
        // More mock adapters...
    ])
}
```

**Implementation Checklist:**
- [ ] Mock status endpoint
- [ ] Mock adapters list
- [ ] Mock peers list
- [ ] Mock message send (accept and log)
- [ ] Mock inbox (return sample messages)

### Task 0.3: Web UI with Mock Data

**Directory:** `web-ui/`
**Effort:** 15-20 hours

Build UI components that work with mock API:

```svelte
<!-- Dashboard.svelte -->
<script>
  import { onMount } from 'svelte';

  let status = null;

  onMount(async () => {
    const res = await fetch('/api/status');
    status = await res.json();
  });
</script>

<div class="dashboard">
  {#if status}
    <StatusCard title="Uptime" value={formatUptime(status.uptime)} />
    <StatusCard title="Peers" value={status.peers_connected} />
    <StatusCard title="Adapters" value={status.adapters_active} />
  {/if}
</div>
```

**Implementation Checklist:**
- [ ] Dashboard page with status cards
- [ ] Adapters page with enable/disable
- [ ] Peers list with search
- [ ] Simple log viewer
- [ ] Settings page (read-only for now)

---

## Phase 1: API Integration [After Core/Node Ready]

**Estimated Effort:** 50-60 hours
**Dependencies:** Core Router, Node Adapters
**Start when:** Core Phase 2 and Node Phase 3 complete

### Task 1.1: Message Send API

**File:** `myriadnode-full/src/api.rs`
**Effort:** 12-15 hours

Replace mock with real implementation:

```rust
async fn send_message(
    State(app): State<AppState>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<SendMessageResponse>, ApiError> {
    // Validate request
    let dest = NodeId::from_str(&req.destination)?;

    // Create message
    let msg = Message::new(
        app.node.identity(),
        dest,
        req.payload.as_bytes(),
        req.ttl.unwrap_or(3600),
    );

    // Route via router
    let status = app.router.queue_message(msg).await?;

    Ok(Json(SendMessageResponse {
        message_id: msg.id().to_string(),
        status: status.to_string(),
        timestamp: Utc::now(),
    }))
}
```

**Implementation Checklist:**
- [ ] Validate destination node ID format
- [ ] Validate payload size (max 64KB)
- [ ] Rate limiting per client IP
- [ ] Queue message in router
- [ ] Return message ID and status

### Task 1.2: DHT Status API

**File:** `myriadnode-full/src/api.rs`
**Effort:** 10-12 hours

```rust
async fn get_dht_status(
    State(app): State<AppState>,
) -> Json<DhtStatus> {
    let dht = &app.node.dht;

    Json(DhtStatus {
        routing_table_size: dht.routing_table().len(),
        buckets: dht.routing_table().bucket_stats(),
        cache_entries: dht.cache().len(),
        cache_hit_rate: dht.cache().hit_rate(),
    })
}
```

**Implementation Checklist:**
- [ ] Query DHT routing table
- [ ] Get bucket statistics
- [ ] Cache status and hit rate
- [ ] Recent lookup latencies

### Task 1.3: Real Adapter Status

**Effort:** 8-10 hours

**Implementation Checklist:**
- [ ] Query adapter manager for real stats
- [ ] Per-adapter enable/disable controls
- [ ] Live metrics updates

### Task 1.4: Peers from DHT

**Effort:** 8-10 hours

**Implementation Checklist:**
- [ ] Query DHT for known peers
- [ ] Include last-seen timestamps
- [ ] Show distance metrics

---

## Phase 2: Web Dashboard Polish [After API Works]

**Estimated Effort:** 40-50 hours
**Dependencies:** Phase 1 complete

### Task 2.1: Real-Time Updates

**Effort:** 12-15 hours

```svelte
<script>
  import { onMount, onDestroy } from 'svelte';

  let status = null;
  let interval;

  onMount(() => {
    fetchStatus();
    interval = setInterval(fetchStatus, 1000);
  });

  onDestroy(() => clearInterval(interval));

  async function fetchStatus() {
    const res = await fetch('/api/status');
    status = await res.json();
  }
</script>
```

**Implementation Checklist:**
- [ ] Polling for status updates (1s interval)
- [ ] WebSocket for log streaming
- [ ] Visual indicators for state changes
- [ ] Error handling and reconnection

### Task 2.2: Network Visualization

**Effort:** 15-20 hours

**Implementation Checklist:**
- [ ] D3.js or similar for topology graph
- [ ] Show nodes and connections
- [ ] Highlight routing paths
- [ ] Click for node details

### Task 2.3: Message Management

**Effort:** 12-15 hours

**Implementation Checklist:**
- [ ] Send message form
- [ ] Inbox with pagination
- [ ] Message detail view
- [ ] Delivery status tracking

---

## Phase 3: Ledger Integration [After Core Flow Works]

**Estimated Effort:** 35-45 hours
**Dependencies:** Messages flowing through system

### Task 3.1: Ledger Event Hooks

**File:** `crates/myriadmesh-ledger/src/hooks.rs`
**Effort:** 15-20 hours

```rust
impl LedgerHooks {
    pub fn on_message_sent(&self, msg: &Message, status: DeliveryStatus) {
        self.ledger.append(LedgerEntry::MessageSent {
            message_hash: msg.hash(),
            destination: msg.destination(),
            timestamp: Utc::now(),
            status,
        });
    }

    pub fn on_message_received(&self, msg: &Message) {
        self.ledger.append(LedgerEntry::MessageReceived {
            message_hash: msg.hash(),
            source: msg.source(),
            timestamp: Utc::now(),
        });
    }
}
```

**Implementation Checklist:**
- [ ] Hook into router for message events
- [ ] Record sent/received/failed events
- [ ] Include minimal metadata (hash, not content)
- [ ] Append-only log structure

### Task 3.2: Ledger Query API

**Effort:** 10-12 hours

**Implementation Checklist:**
- [ ] GET /api/ledger/entries (with filters)
- [ ] GET /api/ledger/blocks
- [ ] Export to JSON/CSV
- [ ] Search by message hash or node ID

### Task 3.3: Ledger Verification

**Effort:** 10-12 hours

**Implementation Checklist:**
- [ ] Merkle tree for block integrity
- [ ] Block chaining (hash links)
- [ ] Verification endpoint

---

## Phase 4: Production Hardening [Before Release]

**Estimated Effort:** 30-40 hours

### Task 4.1: Observability

**Effort:** 15-20 hours

**Implementation Checklist:**
- [ ] Prometheus metrics endpoint (/metrics)
- [ ] Structured logging (JSON format option)
- [ ] Tracing spans for requests
- [ ] Health check endpoint (/health)

### Task 4.2: Security

**Effort:** 10-15 hours

**Implementation Checklist:**
- [ ] CORS configuration
- [ ] Rate limiting (tower-governor)
- [ ] Request validation
- [ ] Optional API key auth

### Task 4.3: Deployment

**Effort:** 8-10 hours

**Implementation Checklist:**
- [ ] Docker image
- [ ] Docker Compose example
- [ ] Kubernetes manifests
- [ ] Systemd service file

---

## Dependency Graph

```
myriadmesh-server
    │
    ├── Depends on myriadmesh-core:
    │   • Router for message sending
    │   • DHT for status APIs
    │
    ├── Depends on myriadmesh-node:
    │   • Adapters for connectivity
    │   • Node for bootstrap
    │
    └── Provides to myriadmesh-clients:
        • REST API for all operations
        • WebSocket for streaming
```

---

## Work Summary

| Task | Effort | Priority | Status | Dependencies |
|------|--------|----------|--------|--------------|
| API Specification | 8-10h | P0 | Ready | None |
| Mock API | 10-12h | P0 | Ready | None |
| Mock Web UI | 15-20h | P0 | Ready | Mock API |
| Message Send API | 12-15h | P0 | Blocked | Core Router |
| DHT Status API | 10-12h | P0 | Blocked | Core DHT |
| Real Adapter Status | 8-10h | P0 | Blocked | Node Adapters |
| Peers from DHT | 8-10h | P1 | Blocked | Core DHT |
| Real-Time Updates | 12-15h | P1 | After API | - |
| Network Visualization | 15-20h | P2 | After API | - |
| Message Management | 12-15h | P1 | After API | - |
| Ledger Hooks | 15-20h | P1 | After messages | - |
| Ledger API | 10-12h | P1 | After hooks | - |
| Ledger Verification | 10-12h | P2 | After API | - |
| Observability | 15-20h | P1 | Before release | - |
| Security | 10-15h | P1 | Before release | - |
| Deployment | 8-10h | P1 | Before release | - |

**Can Start Now:** ~35-45 hours (Phase 0)
**Blocked by Core/Node:** ~40-50 hours (Phase 1)
**After Integration:** ~105-140 hours (Phases 2-4)

---

## Getting Started

### Step 1: API Design
```bash
cd myriadmesh-server
mkdir -p docs
# Create docs/api.md with OpenAPI spec
```

### Step 2: Mock API
```bash
cargo run -p myriadnode-full
# Test with: curl http://localhost:8080/api/status
```

### Step 3: Web UI Development
```bash
cd web-ui
npm install
npm run dev
# Opens http://localhost:5173
```

### Step 4: Wait for Core/Node
When Core and Node are ready, replace mocks with real implementations.

---

## Success Criteria

### Phase 0 Complete When:
- [ ] All API endpoints documented
- [ ] Mock API returns realistic data
- [ ] Web UI shows dashboard with mock data

### Phase 1 Complete When:
- [ ] Messages actually send via router
- [ ] DHT status reflects real network
- [ ] Adapter metrics are live

### Phase 2 Complete When:
- [ ] Dashboard updates in real-time
- [ ] Network topology visualization works
- [ ] Users can send/receive messages via UI

### Ready for Production When:
- [ ] Prometheus metrics exported
- [ ] Docker image builds and runs
- [ ] Rate limiting configured
- [ ] Documentation complete

---

## Files to Create/Modify

**Create:**
- `docs/api.md` - OpenAPI specification
- `myriadnode-full/src/mock.rs` - Mock data generators
- `web-ui/src/lib/api.ts` - API client
- `web-ui/src/routes/+page.svelte` - Dashboard

**Modify:**
- `myriadnode-full/src/api.rs` - Implement endpoints
- `myriadnode-full/src/main.rs` - Wire up state
- `crates/myriadmesh-ledger/src/lib.rs` - Event hooks

---

## Notes

- **Mock first** - Build UI against mocks, swap later
- **Document API early** - Helps client team plan
- **Simple is better** - Don't over-engineer the dashboard
- **Defer ledger** - Focus on core functionality first

---

**Owner:** Server Developer
**Next Milestone:** Mock API and UI complete
**Review:** When Phase 0 complete
