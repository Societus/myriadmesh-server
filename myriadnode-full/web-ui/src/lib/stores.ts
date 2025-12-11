/**
 * Svelte stores for global application state
 * These stores are reactive and automatically update components
 */

import { writable, derived } from 'svelte/store';
import type { AdapterStatus, NodeInfo, HeartbeatStats, NodeMapEntry, FailoverEvent } from './api';

// Node information
export const nodeInfo = writable<NodeInfo | null>(null);

// Adapter statuses
export const adapters = writable<AdapterStatus[]>([]);

// Heartbeat statistics
export const heartbeatStats = writable<HeartbeatStats | null>(null);

// NodeMap entries
export const nodeMap = writable<NodeMapEntry[]>([]);

// Recent failover events
export const failoverEvents = writable<FailoverEvent[]>([]);

// Loading state
export const isLoading = writable<boolean>(true);

// Error state
export const error = writable<string | null>(null);

// Auto-refresh interval (in milliseconds)
export const refreshInterval = writable<number>(5000);

/**
 * Derived store: Active adapters count
 */
export const activeAdaptersCount = derived(adapters, ($adapters) => {
	return $adapters.filter((a) => a.active).length;
});

/**
 * Derived store: Healthy adapters count
 */
export const healthyAdaptersCount = derived(adapters, ($adapters) => {
	return $adapters.filter((a) => a.health_status === 'Healthy').length;
});

/**
 * Derived store: Primary adapter (first active adapter)
 */
export const primaryAdapter = derived(adapters, ($adapters) => {
	return $adapters.find((a) => a.active) || null;
});

/**
 * Derived store: Privacy-optimized adapters (privacy_level > 0.7)
 */
export const privateAdapters = derived(adapters, ($adapters) => {
	return $adapters.filter((a) => a.metrics.privacy_level > 0.7);
});

/**
 * Derived store: Total known nodes from heartbeat
 */
export const totalKnownNodes = derived(heartbeatStats, ($stats) => {
	return $stats?.total_nodes || 0;
});
