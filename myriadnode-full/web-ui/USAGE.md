# MyriadNode Web UI - User Guide

## Quick Start

1. **Start MyriadNode backend**:
   ```bash
   cargo run --package myriadnode
   ```

2. **Access the Web UI**:
   - Open browser to `http://localhost:8080`
   - The UI will automatically connect and start polling

## Main Features

### 📊 Dashboard

The dashboard provides an at-a-glance view of your node's status:

**Statistics Cards:**
- **Active Adapters**: Number of network adapters currently running
- **Healthy Adapters**: Adapters operating without degradation
- **Known Nodes**: Total peers discovered in the mesh network
- **Heartbeat Metrics**: Network activity and connectivity stats

**Adapter Cards:**
Each adapter displays:
- Type (Ethernet, I2P, WiFi, Bluetooth LE, LoRa)
- Active status and health indicator
- Backhaul designation (primary internet connection)
- Performance metrics:
  - Latency (ms)
  - Bandwidth (Mbps)
  - Reliability (0-100%)
  - Power consumption
  - Privacy level (0-100%)

### ✉️ Messages

Send and track messages across the mesh network:

**Sending a Message:**
1. Enter the **Destination Node ID** (64-byte hex string, 128 characters)
   - Example: `a1b2c3d4e5f6789012345678901234567890abcdef...`
   - You can find Node IDs in the NodeMap page
2. Select an **Adapter** (or choose "Auto" for automatic selection)
3. Enter your **Message Content** (up to 1024 characters)
4. Click **Send Message**

**Message History:**
- View all sent messages with timestamps
- Status indicators:
  - 🟡 **Pending**: Message queued for sending
  - 🟢 **Sent**: Successfully transmitted
  - 🔴 **Failed**: Transmission failed
- Delete messages from history

**Real-time Updates:**
- Receive instant notifications when messages are sent
- WebSocket connection provides live status updates

### 🔌 Adapters

Manage network adapters:

**Adapter Information:**
- **Type**: Ethernet, I2P, WiFi, Bluetooth LE, LoRa
- **Status**: Active/Inactive
- **Health**: Healthy, Degraded, or Failed
- **Metrics**: Performance statistics

**Actions:**
- **Start**: Activate an adapter
- **Stop**: Deactivate an adapter
- View detailed metrics and configuration

**Adapter Types:**
- **Ethernet**: LAN/WAN connectivity via UDP multicast
- **I2P**: Anonymous overlay network (highest privacy)
- **WiFi**: Direct WiFi mesh (ad-hoc or infrastructure)
- **Bluetooth LE**: Short-range, low-power mesh
- **LoRa**: Long-range, ultra-low power (IoT)

### 🗺️ NodeMap

Visualize discovered nodes in the mesh:

**Node Information:**
- Node ID
- Last seen timestamp
- Number of adapters
- Connection quality
- Heartbeat count

**Adapter Details per Node:**
- Adapter type
- Active status
- Bandwidth
- Latency
- Privacy level

### ⚙️ Settings

Configure your node:

**Network Configuration:**
- Listen address and port
- Enable/disable adapters
- Set backhaul preferences
- Configure failover behavior

**Adapter Settings:**
- Per-adapter configuration
- Scoring weights for adapter selection
- Privacy vs performance trade-offs

## Real-Time Features

### WebSocket Connection

The UI maintains a persistent WebSocket connection to receive live updates:

**Connection Status:**
- Automatically connects on page load
- Reconnects on disconnection (up to 5 attempts)
- Visible in browser console

**Event Types:**
- `message_sent` - Your message was transmitted
- `message_received` - Incoming message from another node
- `adapter_status` - Adapter started, stopped, or health changed
- `heartbeat_update` - Network statistics updated
- `failover_event` - Automatic adapter failover occurred
- `dht_node_discovered` - New peer found via DHT
- `status_update` - General system notifications

**How it Works:**
1. UI connects to `ws://host/ws`
2. Backend broadcasts events to all connected clients
3. UI updates stores and triggers re-renders
4. No polling needed for event-driven updates

## Data Refresh

The UI uses dual update mechanisms:

**Polling (Default: 5 seconds):**
- Fetches all data on interval
- Ensures consistency
- Continues even if WebSocket fails

**WebSocket (Real-time):**
- Instant event notifications
- Lower latency
- Selective updates

**Combined Benefits:**
- Fast updates for events (WebSocket)
- Reliable sync for all data (polling)
- Graceful degradation if WS unavailable

## Troubleshooting

### No Data Displayed

**Check:**
1. MyriadNode backend is running
2. Backend accessible at `http://localhost:8080`
3. No CORS errors in browser console
4. API endpoints responding (check Network tab)

**Fix:**
- Restart MyriadNode backend
- Clear browser cache and reload
- Check firewall settings

### WebSocket Connection Failed

**Symptoms:**
- Console shows "WebSocket error" or "Failed to create WebSocket"
- Real-time updates not working
- Reconnection attempts visible

**Fix:**
1. Check backend is running
2. Verify WebSocket endpoint: `ws://localhost:8080/ws`
3. Check for proxy/firewall blocking WebSocket
4. Try disabling browser extensions

**Note:** Polling will continue to work even if WebSocket fails

### Messages Not Sending

**Check:**
1. Destination Node ID is valid 64-byte hex (128 characters)
2. At least one adapter is active
3. Network connectivity exists
4. Backend logs for routing errors

**Common Issues:**
- Invalid Node ID format
- No active adapters
- Destination node unreachable
- Message too large (>1024 characters)

### Adapter Won't Start

**Possible Causes:**
- Adapter not supported on this platform
- Missing dependencies (e.g., I2P router not running)
- Port already in use
- Insufficient permissions

**Debug Steps:**
1. Check backend logs for error details
2. Verify adapter dependencies installed
3. Try stopping and restarting the adapter
4. Check adapter configuration in settings

## Performance Tips

### Reduce CPU Usage
- Increase polling interval in settings (5s → 10s)
- Disable unused adapters
- Limit number of browser tabs

### Improve Responsiveness
- Enable WebSocket for instant updates
- Use modern browser (Chrome, Firefox, Edge)
- Reduce background processes

### Save Bandwidth
- Increase polling interval
- Disable adapters you're not using
- Use message priorities wisely

## Security & Privacy

### Privacy Levels

Adapters have different privacy characteristics:

- **I2P**: Highest privacy (anonymous overlay)
- **LoRa**: High privacy (low-power broadcast)
- **Bluetooth LE**: Medium privacy (short range)
- **WiFi**: Medium privacy (encrypted mesh)
- **Ethernet**: Lower privacy (local network)

### Best Practices

1. **Use I2P for sensitive communications**
2. **Enable multiple adapters for redundancy**
3. **Configure backhaul carefully** (affects internet routing)
4. **Monitor adapter health** regularly
5. **Keep Node ID private** (don't share publicly)
6. **Review message history** and delete sensitive content
7. **Use strong WiFi encryption** if using WiFi adapter

### Data Storage

- **All data stays local** - No external telemetry
- **Messages stored in SQLite** (configurable)
- **Node discovered via DHT** are cached locally
- **Settings stored in config file**

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `/` | Focus search (future) |
| `Esc` | Close modal/dialog |
| `Ctrl+R` | Refresh data |
| `Ctrl+,` | Open settings |

## Browser Compatibility

**Fully Supported:**
- Chrome/Chromium 90+
- Firefox 88+
- Edge 90+
- Safari 14+

**WebSocket Required:**
- Modern browsers with WebSocket support
- No IE11 support

## Developer Mode

For debugging and development:

**Browser Console:**
```javascript
// Check WebSocket connection
wsClient.isConnected()

// Get subscriber count
wsClient.subscriber_count

// Manually trigger refresh
refreshData()

// Check stores
$nodeInfo
$adapters
$heartbeatStats
```

**Enable Debug Logging:**
All WebSocket events are logged to console by default.

## FAQ

**Q: How often does the UI poll the backend?**
A: Every 5 seconds by default. This can be configured in the data service.

**Q: Can I run the UI on a different machine?**
A: Yes! Build the UI and configure the API URL in `vite.config.js` to point to your node.

**Q: What happens if the backend crashes?**
A: The UI will show connection errors. Polling and WebSocket will attempt reconnection automatically.

**Q: How do I find other node IDs?**
A: Check the NodeMap page - it shows all discovered nodes and their IDs.

**Q: Can I send messages to myself?**
A: Yes, but it's mainly useful for testing.

**Q: What's the maximum message size?**
A: 1024 characters in the UI. Protocol supports larger payloads but UI enforces this limit.

**Q: Why use both polling and WebSocket?**
A: Redundancy and reliability. WebSocket provides instant updates, polling ensures consistency.

## Support

For issues, questions, or feature requests:
- Check the main MyriadMesh documentation
- Review backend logs for errors
- Open an issue on GitHub

## Version

Current version: 0.9.0

Check `package.json` for exact version and dependencies.
