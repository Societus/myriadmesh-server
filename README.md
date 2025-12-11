# MyriadMesh Server

**Full-featured server with API, web UI, and network management**

The server component provides centralized coordination, management, and monitoring for MyriadMesh networks. It combines node functionality with enterprise features for production deployments.

## Overview

MyriadMesh Server is a comprehensive application for:
- **Network operation** - Full mesh node with all adapters
- **Centralized management** - Web dashboard and REST API
- **Network monitoring** - Real-time metrics and performance tracking
- **Device management** - Node pairing and appliance control
- **Message relay** - Store-and-forward for offline nodes
- **System auditing** - Immutable ledger of network events
- **Software updates** - Secure update distribution

## Repository Structure

```
myriadmesh-server/
├── Cargo.toml                 Workspace root
├── README.md                  This file
├── DEPLOYMENT.md              Production deployment guide
├── API.md                     REST API documentation
├── CONFIG.md                  Configuration reference
│
├── crates/
│   ├── myriadmesh-ledger/     Blockchain-style audit ledger
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── block.rs       Ledger block structure
│   │   │   ├── consensus.rs   Consensus mechanism
│   │   │   ├── storage.rs     Persistent storage
│   │   │   ├── sync.rs        Ledger synchronization
│   │   │   └── error.rs
│   │   └── tests/
│   │
│   ├── myriadmesh-appliance/  Device and appliance management
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── manager.rs     Appliance manager
│   │   │   ├── device.rs      Device registration
│   │   │   ├── pairing.rs     QR code/PIN pairing
│   │   │   ├── power.rs       Power management
│   │   │   ├── cache.rs       Store-and-forward cache
│   │   │   └── types.rs
│   │   └── tests/
│   │
│   └── myriadmesh-updates/    Software update distribution
│       ├── Cargo.toml
│       ├── src/
│       │   ├── lib.rs
│       │   ├── coordination.rs Update orchestration
│       │   ├── distribution.rs Binary distribution
│       │   ├── schedule.rs     Update scheduling
│       │   ├── verification.rs Signature verification
│       │   └── error.rs
│       └── tests/
│
├── myriadnode-full/           Full server binary
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs            Entry point
│   │   ├── node.rs            Node implementation
│   │   ├── api.rs             REST API server (Axum)
│   │   ├── config.rs          Configuration management
│   │   ├── storage.rs         SQLite database
│   │   ├── monitor.rs         Network performance monitor
│   │   ├── heartbeat.rs       Node heartbeat system
│   │   ├── failover.rs        Failover logic
│   │   ├── scoring.rs         Path/adapter scoring
│   │   ├── diagnostics.rs     Diagnostics module
│   │   └── error.rs           Error types
│   ├── config.toml            Configuration template
│   ├── examples/
│   │   ├── docker-compose.yml Docker stack
│   │   └── nginx-proxy.conf   Nginx reverse proxy
│   └── tests/
│       ├── e2e_api.rs
│       ├── e2e_ui.rs
│       └── integration.rs
│
├── web-ui/                    Svelte web dashboard
│   ├── package.json
│   ├── vite.config.js
│   ├── src/
│   │   ├── App.svelte
│   │   ├── routes/
│   │   │   ├── +page.svelte       Dashboard
│   │   │   ├── adapters/          Adapter management
│   │   │   ├── devices/           Device management
│   │   │   ├── network/           Network topology
│   │   │   ├── settings/          System settings
│   │   │   └── logs/              Event logs
│   │   ├── components/
│   │   ├── lib/
│   │   │   └── api.js             API client
│   │   └── styles/
│   ├── static/
│   └── build/                 (Generated on build)
│
├── docs/
│   ├── ARCHITECTURE.md        Server architecture
│   ├── API_REFERENCE.md       API endpoints
│   ├── WEB_UI_GUIDE.md        Web interface guide
│   ├── DEPLOYMENT.md          Production deployment
│   ├── DOCKER.md              Docker containerization
│   ├── DATABASE.md            SQLite schema
│   └── TROUBLESHOOTING.md     Common issues
│
├── examples/
│   ├── docker-compose.yml
│   ├── kubernetes/
│   │   └── deployment.yaml
│   ├── systemd/
│   │   └── myriadmesh-server.service
│   └── nginx/
│       └── myriad-server.conf
│
└── tests/
    ├── e2e_web_ui.rs         Web UI tests
    ├── e2e_rest_api.rs       REST API tests
    └── integration.rs        Full stack integration
```

## Core Features

### 1. REST API Server

**Framework**: Axum (Rust async web framework)

**Key Endpoints**:
```
GET    /api/status              Node health and version
GET    /api/adapters            List available adapters
POST   /api/adapters/:id/start  Start adapter
POST   /api/adapters/:id/stop   Stop adapter

GET    /api/nodes               List known nodes
POST   /api/messages/send       Send message to node
GET    /api/messages/inbox      Retrieve messages

GET    /api/devices             List paired devices
POST   /api/devices/pair        Initiate device pairing
DELETE /api/devices/:id         Unpair device

GET    /api/ledger/entries      List audit log entries
GET    /api/ledger/blocks       List ledger blocks

GET    /api/metrics             Performance metrics
GET    /api/logs                System logs (WebSocket)

POST   /api/updates/check       Check for updates
POST   /api/updates/schedule    Schedule update
```

**Full API documentation**: See [API.md](API.md)

### 2. Web Dashboard (Svelte)

**Pages**:
- **Dashboard**: Real-time network status, metrics
- **Adapters**: Enable/disable adapters, monitor performance
- **Devices**: Manage paired devices, appliances
- **Network**: Topology visualization, peer discovery
- **Settings**: System configuration, security
- **Logs**: Audit trail, event history

**Features**:
- Real-time updates via WebSocket
- Responsive design (mobile, tablet, desktop)
- Dark/light themes
- Export capabilities
- Offline capability (service worker)

### 3. Ledger (Immutable Audit Trail)

**Purpose**: Create immutable record of network events

**Stores**:
- Node discovery events
- Message delivery confirmations
- Network performance test results
- Key exchange operations
- Security-relevant events

**Benefits**:
- Audit compliance
- Forensics analysis
- Network history
- Consensus mechanism (multi-node ledgers)

**Schema**:
```rust
pub struct LedgerBlock {
    pub index: u64,
    pub timestamp: u64,
    pub prev_hash: [u8; 32],
    pub entries: Vec<LedgerEntry>,
    pub nonce: u64,  // For PoW consensus
    pub hash: [u8; 32],
}

pub enum LedgerEntry {
    NodeDiscovered { node_id, timestamp },
    MessageDelivered { from, to, message_id },
    AdapterFailover { from_adapter, to_adapter },
    KeyExchange { node_id, public_key },
    // ... more entry types
}
```

### 4. Appliance Management

**Device Pairing**:
- QR code pairing (mobile)
- PIN-based pairing (default)
- Secure key exchange

**Capabilities**:
- Battery monitoring
- Data usage limits
- Power mode management
- Remote wakeup/shutdown
- Firmware updates

**Configuration**:
```toml
[appliance]
max_devices = 100
pairing_timeout = 3600  # seconds
cache_size = 50000      # bytes
enable_remote_commands = true
```

### 5. Software Updates

**Secure Distribution**:
- Signed update packages
- Differential updates (only changed files)
- Rollback capability
- Staged rollout (5% → 25% → 100%)
- Automatic verification

**Schedule**:
```rust
pub enum UpdateSchedule {
    Immediate,
    Daily { hour: u8, minute: u8 },
    Weekly { day: u8, hour: u8 },
    Monthly { day: u8, hour: u8 },
    OnDemand,
}
```

### 6. Monitoring and Diagnostics

**Metrics**:
- Message throughput (msgs/sec)
- Adapter bandwidth utilization
- Routing latency (p50, p95, p99)
- Memory usage trends
- CPU utilization
- Peer discovery rate
- Message delivery rate

**Diagnostics**:
- Network latency tests
- Bandwidth measurement
- Adapter capability detection
- Route quality assessment
- Dependency health checks

## Installation

### Prerequisites

```bash
# Ubuntu/Debian
sudo apt-get install libsodium-dev libssl-dev pkg-config sqlite3

# macOS
brew install libsodium openssl sqlite3

# Fedora
sudo dnf install libsodium-devel openssl-devel sqlite-devel
```

### From Source

```bash
git clone https://github.com/myriadmesh/server.git
cd myriadmesh-server

# Build backend
cargo build --release -p myriadnode-full

# Build web UI
cd myriadnode-full/web-ui
npm install
npm run build
cd ../..

# Combined binary with embedded UI
cargo build --release --features "server,web-ui,api"
```

### Docker

```bash
docker run -d \
  --name myriad-server \
  -p 8080:8080 \
  -v /data/myriad:/root/.myriad \
  -v /data/logs:/var/log/myriad \
  myriadmesh/server:latest
```

## Configuration

Create `config.toml`:

```toml
[server]
listen_address = "0.0.0.0"
listen_port = 8080
api_path = "/api"
web_root = "/web"

[node]
# Use full node with all adapters
mode = "full"
adapters = ["ethernet", "bluetooth", "lora", "cellular"]

[database]
type = "sqlite"
path = "./data/myriad.db"
# Or PostgreSQL for production:
# url = "postgresql://user:pass@localhost/myriadmesh"

[security]
# TLS/HTTPS
enable_https = false
cert_path = "/etc/myriad/server.crt"
key_path = "/etc/myriad/server.key"

# Authentication
api_key_required = true
api_key = "your-secure-key"  # Generate with: openssl rand -hex 32

[ledger]
enabled = true
consensus_enabled = false  # Set to true for multi-node consensus

[monitoring]
metrics_retention = 7  # days
log_level = "info"
enable_metrics_export = false  # Set to true for Prometheus
```

## Web UI Build

The web UI is built with SvelteKit and Vite, then embedded in the binary:

```bash
cd myriadnode-full/web-ui

# Development (hot reload)
npm run dev

# Build production
npm run build

# The binary then includes dist/ via include_bytes!
```

## Development

### Building

```bash
# Build all components
cargo build --release --features "server,web-ui,api"

# Build backend only (no UI)
cargo build --release -p myriadnode-full

# Build with specific features
cargo build --release --features "server,api"  # No web UI
cargo build --release --features "server,web-ui"  # No API
```

### Testing

```bash
# Run all tests
cargo test --release

# API integration tests
cargo test --release --test e2e_rest_api

# Web UI tests (with Playwright)
npm run test:e2e

# Load testing
cargo run --release --example load_test -- --clients 100 --duration 60
```

### Database

The server uses SQLite for local deployment (PostgreSQL recommended for production):

```bash
# View schema
sqlite3 data/myriad.db ".schema"

# Run migrations
cargo run --release -- migrate

# Export data
sqlite3 data/myriad.db "SELECT * FROM ledger_blocks;" > ledger.csv
```

## Deployment

### Single Machine

```bash
# Create system user
sudo useradd -r -s /bin/false myriad

# Install binary
sudo cp target/release/myriadnode-full /usr/local/bin/

# Create data directory
sudo mkdir -p /var/lib/myriad
sudo chown myriad:myriad /var/lib/myriad

# Install systemd service
sudo cp examples/systemd/myriadmesh-server.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable myriadmesh-server
sudo systemctl start myriadmesh-server
```

### Docker Compose Stack

```bash
docker-compose -f examples/docker-compose.yml up -d
```

Includes:
- MyriadMesh Server
- PostgreSQL database
- Nginx reverse proxy
- Prometheus monitoring (optional)

### Kubernetes

```bash
kubectl apply -f examples/kubernetes/
```

Creates:
- Deployment (3 replicas)
- Service (LoadBalancer)
- PersistentVolumeClaim (database)
- ConfigMap (configuration)

### Behind Nginx

```nginx
upstream myriad_server {
    server localhost:8080;
}

server {
    listen 443 ssl http2;
    server_name mesh.example.com;

    ssl_certificate /etc/letsencrypt/live/mesh.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/mesh.example.com/privkey.pem;

    location / {
        proxy_pass http://myriad_server;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_redirect off;
    }

    location /api {
        proxy_pass http://myriad_server/api;
        # ... API-specific settings
    }
}
```

## Monitoring

### Health Checks

```bash
# Check if server is running
curl http://localhost:8080/api/status

# Check database
curl http://localhost:8080/api/health/database

# Check adapters
curl http://localhost:8080/api/adapters
```

### Metrics

```bash
# Prometheus format (if enabled)
curl http://localhost:8080/metrics

# Key metrics:
# - myriad_messages_total (counter)
# - myriad_message_latency (histogram)
# - myriad_adapters_active (gauge)
# - myriad_connected_nodes (gauge)
# - myriad_ledger_blocks (gauge)
```

### Logging

```bash
# View logs
tail -f /var/log/myriad/server.log

# JSON format (structured logging)
journalctl -u myriadmesh-server --output=json | jq .

# Export for analysis
journalctl -u myriadmesh-server > /tmp/logs.txt
```

## Performance Targets

| Metric | Target |
|--------|--------|
| API response time (p99) | <100 ms |
| Web UI load time | <2 seconds |
| Message processing | 1000+ msg/s |
| Concurrent connections | 1000+ clients |
| Memory usage | <1 GB (idle) |
| Database query time | <10 ms |

## Roadmap

### Short-term (1-2 months)
- [ ] WebRTC peer-to-peer mode
- [ ] GraphQL API (alternative to REST)
- [ ] LDAP/OAuth2 authentication
- [ ] Plugin system

### Medium-term (2-4 months)
- [ ] Cluster mode (multi-node coordination)
- [ ] Advanced analytics dashboard
- [ ] Backup and restore tools
- [ ] Network simulation mode

### Long-term (6+ months)
- [ ] Quantum-resistant encryption
- [ ] ML-based anomaly detection
- [ ] 5G/6G integration
- [ ] Edge cloud deployment

## Troubleshooting

### High Memory Usage
- Limit ledger retention: `ledger_retention = 7  # days`
- Reduce metrics retention: `metrics_retention = 1  # days`
- Clear message cache: `POST /api/cache/clear`

### Slow Database
- Use PostgreSQL for production
- Enable database optimization: `PRAGMA optimize;`
- Index frequently queried fields

### Web UI Not Loading
- Check if build succeeded: `cargo build --release`
- Clear browser cache: Ctrl+Shift+Delete
- Check web root path in config

See [TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) for more.

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) in main repository.

## License

Licensed under **GPL-3.0-only**.

## Support

- **Documentation**: See `docs/` directory
- **API Docs**: [API.md](API.md)
- **Issues**: GitHub Issues
- **Community**: Discord/Matrix

## Development Roadmap

See [ROADMAP.md](ROADMAP.md) for detailed timeline and work items.

**Current Phase**: API & Dashboard Implementation (6-8 weeks)

**Critical Path:**
1. **[BLOCKED] REST API message routing** (1 week) - Waiting for core team Router
2. **[BLOCKED] DHT status APIs** (1 week) - Waiting for core team DHT
3. **Web dashboard** (2-3 weeks) - After APIs ready
4. **Ledger integration** (2 weeks)
5. **Production hardening** (1-2 weeks)

**Key Deliverables:**
- [ ] Message send API integration (Week 1)
- [ ] DHT status and node statistics APIs (Week 2)
- [ ] Configuration management API (Week 2)
- [ ] Web dashboard with monitoring (Weeks 2-4)
- [ ] Ledger and audit trail (Weeks 3-4)
- [ ] Prometheus metrics and observability (Week 5)
- [ ] Production deployment guides (Week 6)

**Blocked by:**
- Core team: Router integration (P0.1.2), DHT queries (P0.2)
- Node team: Adapters working (P0.3)

**Blocks:**
- Client teams: Cannot connect without API

---

**Repository**: https://github.com/myriadmesh/server
**Crates.io**: https://crates.io/crates/myriadmesh-ledger, myriadmesh-appliance, myriadmesh-updates
