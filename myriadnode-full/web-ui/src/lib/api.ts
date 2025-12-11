/**
 * API client for MyriadNode REST API
 * Handles all communication with the Rust backend
 */

const API_BASE = '/api';

export interface NodeInfo {
	id: string;
	name: string;
	version: string;
	uptime_secs: number;
}

export interface AdapterStatus {
	adapter_id: string;
	adapter_type: string;
	active: boolean;
	is_backhaul: boolean;
	health_status: 'Healthy' | 'Degraded' | 'Failed';
	metrics: {
		latency_ms: number;
		bandwidth_bps: number;
		reliability: number;
		power_consumption: number;
		privacy_level: number;
	};
}

export interface HeartbeatStats {
	total_nodes: number;
	nodes_with_location: number;
	adapter_counts: Record<string, number>;
}

export interface NodeMapEntry {
	node_id: string;
	last_seen: number;
	adapters: Array<{
		adapter_id: string;
		adapter_type: string;
		active: boolean;
		bandwidth_bps: number;
		latency_ms: number;
		privacy_level: number;
	}>;
	heartbeat_count: number;
}

export interface FailoverEvent {
	timestamp: number;
	event_type: 'AdapterSwitch' | 'ThresholdViolation' | 'AdapterDown' | 'AdapterRecovered';
	details: string;
}

export interface NetworkConfig {
	scoring_mode: string;
	failover_enabled: boolean;
	heartbeat_enabled: boolean;
	privacy_mode: boolean;
}

/**
 * Fetch wrapper with error handling
 */
async function fetchAPI<T>(endpoint: string): Promise<T> {
	const response = await fetch(`${API_BASE}${endpoint}`);

	if (!response.ok) {
		throw new Error(`API error: ${response.status} ${response.statusText}`);
	}

	return response.json();
}

/**
 * POST wrapper with error handling
 */
async function postAPI<T>(endpoint: string, data: unknown): Promise<T> {
	const response = await fetch(`${API_BASE}${endpoint}`, {
		method: 'POST',
		headers: {
			'Content-Type': 'application/json'
		},
		body: JSON.stringify(data)
	});

	if (!response.ok) {
		throw new Error(`API error: ${response.status} ${response.statusText}`);
	}

	return response.json();
}

/**
 * Get node information
 */
export async function getNodeInfo(): Promise<NodeInfo> {
	return fetchAPI<NodeInfo>('/node/info');
}

/**
 * Get all adapter statuses
 */
export async function getAdapters(): Promise<AdapterStatus[]> {
	return fetchAPI<AdapterStatus[]>('/adapters');
}

/**
 * Get specific adapter status
 */
export async function getAdapter(adapterId: string): Promise<AdapterStatus> {
	return fetchAPI<AdapterStatus>(`/adapters/${adapterId}`);
}

/**
 * Start an adapter
 */
export async function startAdapter(adapterId: string): Promise<void> {
	await postAPI(`/adapters/${adapterId}/start`, {});
}

/**
 * Stop an adapter
 */
export async function stopAdapter(adapterId: string): Promise<void> {
	await postAPI(`/adapters/${adapterId}/stop`, {});
}

/**
 * Get heartbeat statistics
 */
export async function getHeartbeatStats(): Promise<HeartbeatStats> {
	return fetchAPI<HeartbeatStats>('/heartbeat/stats');
}

/**
 * Get NodeMap entries
 */
export async function getNodeMap(): Promise<NodeMapEntry[]> {
	return fetchAPI<NodeMapEntry[]>('/heartbeat/nodes');
}

/**
 * Get recent failover events
 */
export async function getFailoverEvents(): Promise<FailoverEvent[]> {
	return fetchAPI<FailoverEvent[]>('/failover/events');
}

/**
 * Get current network configuration
 */
export async function getNetworkConfig(): Promise<NetworkConfig> {
	return fetchAPI<NetworkConfig>('/config/network');
}

/**
 * Update network configuration
 */
export async function updateNetworkConfig(config: Partial<NetworkConfig>): Promise<void> {
	await postAPI('/config/network', config);
}

/**
 * Force a failover to a specific adapter
 */
export async function forceFailover(adapterId: string): Promise<void> {
	await postAPI('/failover/force', { adapter_id: adapterId });
}

// === Message API ===

export interface SendMessageRequest {
	destination: string;
	payload: string;
	priority?: number;
}

export interface SendMessageResponse {
	message_id: string;
	status: string;
}

export async function sendMessage(request: SendMessageRequest): Promise<SendMessageResponse> {
	return postAPI<SendMessageResponse>('/messages/send', request);
}
