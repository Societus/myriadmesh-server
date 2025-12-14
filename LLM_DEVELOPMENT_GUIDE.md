# LLM Development Guide - myriadmesh-server

**Quick reference for LLMs working on the myriadmesh-server repository**

---

## Repository Purpose

myriadmesh-server provides full-featured relay infrastructure for large-scale MyriadMesh deployments. This includes:
- **Full Server Binary** (`myriadnode-full`): Complete feature set for production relays
- **Relay Session Management**: Managing multiple concurrent relay sessions
- **Consensus Participation**: Byzantine Fault Tolerant consensus for network decisions
- **Plugin System**: Extensible architecture for server customization
- **REST API**: Management and monitoring interface
- **Staking & Rewards**: Economic incentive system for relay operators

**Key Principle**: The server is designed for **infrastructure operators** who understand networking and need high reliability, high throughput, and advanced features.

---

## Target Audience

**Primary Audience**: Infrastructure operators and system administrators

**User Types**:
- Infrastructure engineers deploying relays
- DevOps engineers managing server infrastructure
- Network operators managing mesh infrastructure
- Server software developers extending the system

**Documentation Level**: Technical but accessible; assumes networking knowledge

---

## Documentation Standards

### Language & Tone

- Assume knowledge of networking
- Can use technical jargon (but explain first use)
- Focus on "how to set up and manage X"
- Provide configuration examples
- Include deployment guidance
- Document trade-offs and tuning options

### Structure for Feature Documentation

1. **Technical Overview** (What it does)
2. **When to Use/Not Use** (Decision guide)
3. **Prerequisites** (Knowledge, hardware, software)
4. **Configuration** (All options documented with examples)
5. **Deployment** (Multiple platforms: Docker, systemd, cloud)
6. **Troubleshooting** (Common operator problems)
7. **Performance Tuning** (How to optimize)
8. **Monitoring** (What metrics matter)

---

## Code Organization

```
myriadmesh-server/
├── myriadnode-full/              Full server binary
│   ├── src/
│   │   ├── main.rs              Entry point
│   │   ├── relay/               Relay session management
│   │   ├── consensus/           Consensus participation
│   │   ├── api/                 REST API
│   │   ├── plugin/              Plugin system
│   │   └── config/              Configuration
│   ├── Dockerfile               Docker build
│   └── examples/                Deployment examples
│
├── crates/                       Server-specific libraries
│   ├── server-api/              REST API implementation
│   ├── plugin-system/           Plugin architecture
│   ├── session-manager/         Session management
│   └── metrics/                 Monitoring and metrics
│
└── docs/                         Server documentation
    ├── DEPLOYMENT.md            Production deployment
    ├── CONFIGURATION.md         Configuration reference
    ├── PLUGIN_DEVELOPMENT.md    Plugin development guide
    └── TROUBLESHOOTING.md       Operator troubleshooting
```

---

## Common Development Tasks

### Adding a New Configuration Option

1. **Design the option**
   - What problem does it solve?
   - What are valid values?
   - What's the default?
   - How does it affect performance?

2. **Implement in configuration**
   - Add to config struct
   - Add validation logic
   - Add sensible defaults
   - Document with comments

3. **Use in code**
   - Reference config value where needed
   - Handle invalid values gracefully
   - Log configuration on startup

4. **Document thoroughly**
   - Add to configuration reference
   - Provide examples
   - Explain impact on performance/resources
   - Document when to change it

5. **Test configuration**
   - Test valid values
   - Test invalid values
   - Test defaults
   - Test in deployment scenarios

### Implementing a Server Plugin

Follow the plugin development guide to:
- Implement the plugin trait
- Add plugin hooks (startup, message, shutdown)
- Package plugin correctly
- Document plugin thoroughly
- Provide examples

### Optimizing Server Performance

1. **Identify bottleneck**
   - Profile the server
   - Measure resource usage
   - Identify slow operations

2. **Design optimization**
   - Measure before changes
   - Make minimal changes
   - Measure after changes
   - Compare performance

3. **Implement carefully**
   - Don't sacrifice correctness
   - Don't sacrifice security
   - Document the optimization
   - Keep trade-offs in mind

4. **Test thoroughly**
   - Unit tests pass
   - Integration tests pass
   - Load test at scale
   - Measure resource usage

### Fixing an Operator Issue

1. **Reproduce the issue**
   - Understand exactly what operator did
   - Set up similar environment
   - Try to reproduce

2. **Understand root cause**
   - Check logs
   - Check configuration
   - Check system resources
   - Check network

3. **Fix the issue**
   - Make minimal changes
   - Verify fix works
   - Test in deployment environment

4. **Document the solution**
   - Add to troubleshooting guide
   - Update configuration docs if relevant
   - Add known issues if permanent
   - Provide workaround if no fix

---

## Testing Requirements

### Unit Tests

- Test configuration parsing and validation
- Test session management
- Test API endpoints
- Test plugin system

```bash
cargo test --release
```

### Integration Tests

- Test server startup and shutdown
- Test relay session lifecycle
- Test consensus participation
- Test with core libraries

### Deployment Tests

- Test Docker deployment
- Test systemd deployment
- Test multi-instance setup
- Test configuration reloading

### Load Testing

- Test with many concurrent sessions
- Test message throughput
- Measure resource usage
- Identify performance limits

---

## Code Standards

### Configuration Handling

```rust
// Good: Validated configuration with sensible defaults
#[derive(Debug, Clone, Deserialize)]
pub struct RelayConfig {
    /// Maximum concurrent relay sessions (default: 1000)
    #[serde(default = "default_max_sessions")]
    pub max_sessions: usize,

    /// Session timeout in seconds (default: 300)
    #[serde(default = "default_session_timeout")]
    pub session_timeout: u64,
}

fn default_max_sessions() -> usize {
    1000
}

fn default_session_timeout() -> u64 {
    300
}
```

### Error Handling for Operators

```rust
// Good: Operator-friendly error messages
impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConfigInvalid(msg) => {
                write!(f, "Configuration error: {}. Check config.toml", msg)
            }
            Self::PortInUse(port) => {
                write!(f, "Port {} is already in use. Choose a different port or stop the process using it.", port)
            }
            Self::InsufficientMemory => {
                write!(f, "Server ran out of memory. Increase available memory or reduce max_sessions")
            }
        }
    }
}
```

### Logging for Operations

```rust
// Good: Structured logging for monitoring
use log::{info, warn, error};

info!("Server starting on port {}", config.port);
info!("Max relay sessions: {}", config.max_sessions);

warn!("Server memory usage at 80%");

error!("Failed to bind to port {}: {}", config.port, e);
```

---

## Operator-Focused Documentation

### Documentation That Operators Need

- **Deployment Guide**: How to get running on various platforms
- **Configuration Reference**: All options, what they do, defaults
- **Troubleshooting**: How to diagnose and fix problems
- **Performance Tuning**: How to optimize for their needs
- **Monitoring**: What to watch for, what metrics matter
- **Upgrade Guide**: How to safely upgrade
- **Plugin Development**: For operators who want to extend

### Example Documentation Topics

- "How to Set Up a Production Relay Server"
- "Configuring High-Throughput Relays"
- "Monitoring Relay Health"
- "Writing a Custom Authentication Plugin"
- "Scaling Relays for Many Sessions"
- "Upgrading Without Downtime"

---

## Integration with Core Libraries

The server uses core libraries for:
- **Crypto**: Signing and verifying messages
- **DHT**: Discovering other relays and nodes
- **Routing**: Determining best paths for messages

When core libraries change, the server must be updated.

---

## Common Pitfalls

### Pitfall #1: Confusing Operators with Jargon

❌ "Consensus participation requires Byzantine quorum"
✅ "The server participates in network decisions to maintain reliability"

### Pitfall #2: No Default Values

❌ Requiring all configuration to be specified
✅ Providing sensible defaults, allowing optional configuration

### Pitfall #3: No Error Recovery

❌ Crashing on any error
✅ Handling errors gracefully, logging issues, continuing operation

### Pitfall #4: No Monitoring Support

❌ No way to see what the server is doing
✅ Comprehensive logging and metrics for monitoring

### Pitfall #5: Breaking Configuration Changes

❌ Changing configuration format without migration
✅ Supporting old format with deprecation warnings

---

## Quick Commands

```bash
# Build server
cargo build -p myriadnode-full --release

# Run server
./target/release/myriadnode --mode=full --config config.toml

# Run tests
cargo test -p myriadnode-full --release

# Build Docker image
docker build -f myriadmesh-server/myriadnode-full/Dockerfile \
  -t myriadmesh/server:latest .

# Code quality
cargo clippy --all-targets --release
```

---

## Documentation Template

Use **OPERATOR_DOCUMENTATION_TEMPLATE.md**:
- Location: `/root/Projects/myriadmesh-split-workspace/docs/templates/`
- Prerequisites, configuration, deployment examples
- Troubleshooting and performance tuning
- Real deployment examples (Docker, systemd, cloud)

---

## File Locations

| Type | Location |
|------|----------|
| This guide | `LLM_DEVELOPMENT_GUIDE.md` |
| Server code | `myriadnode-full/src/` |
| Config | `myriadnode-full/examples/config.toml` |
| Docker | `myriadnode-full/Dockerfile` |
| Plugin docs | `docs/PLUGIN_DEVELOPMENT.md` |
| Templates | `/root/Projects/myriadmesh-split-workspace/docs/templates/` |

---

## Key Principles

1. **Operators First**
   - Easy to deploy
   - Clear configuration
   - Good error messages
   - Comprehensive logging

2. **Production Ready**
   - High reliability
   - Resource efficiency
   - Performance at scale
   - Easy to monitor

3. **Extensible**
   - Plugin system
   - Custom behaviors
   - Integration points
   - Clear APIs

4. **Observable**
   - Detailed logging
   - Metrics export
   - Health checks
   - Status endpoints

---

**Last Updated**: 2025-12-12

