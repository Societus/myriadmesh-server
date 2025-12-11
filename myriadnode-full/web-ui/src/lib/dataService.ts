/**
 * Data service for polling and updating stores
 */

import * as api from './api';
import { nodeInfo, adapters, heartbeatStats, nodeMap, failoverEvents, isLoading, error } from './stores';
import { wsClient } from './websocket';
import type { WsEvent } from './websocket';

let pollingInterval: ReturnType<typeof setInterval> | null = null;

/**
 * Fetch all data and update stores
 */
export async function refreshData(): Promise<void> {
	try {
		isLoading.set(true);
		error.set(null);

		// Fetch all data in parallel
		const [nodeInfoData, adaptersData, heartbeatData, nodeMapData, eventsData] = await Promise.all([
			api.getNodeInfo().catch(() => null),
			api.getAdapters().catch(() => []),
			api.getHeartbeatStats().catch(() => null),
			api.getNodeMap().catch(() => []),
			api.getFailoverEvents().catch(() => [])
		]);

		// Update stores
		nodeInfo.set(nodeInfoData);
		adapters.set(adaptersData);
		heartbeatStats.set(heartbeatData);
		nodeMap.set(nodeMapData);
		failoverEvents.set(eventsData);

		isLoading.set(false);
	} catch (err) {
		console.error('Failed to refresh data:', err);
		error.set(err instanceof Error ? err.message : 'Unknown error');
		isLoading.set(false);
	}
}

/**
 * Start automatic polling and WebSocket connection
 */
export function startPolling(intervalMs: number = 5000): void {
	// Initial fetch
	refreshData();

	// Clear existing interval
	if (pollingInterval) {
		clearInterval(pollingInterval);
	}

	// Start new interval
	pollingInterval = setInterval(() => {
		refreshData();
	}, intervalMs);

	// Connect WebSocket for real-time updates
	wsClient.connect();

	// Handle WebSocket events
	wsClient.on('adapter_status', (event: WsEvent) => {
		console.log('Adapter status update:', event);
		// Refresh adapters when status changes
		api.getAdapters().then(adapters.set).catch(console.error);
	});

	wsClient.on('heartbeat_update', (event: WsEvent) => {
		console.log('Heartbeat update:', event);
		// Refresh heartbeat stats
		api.getHeartbeatStats().then(heartbeatStats.set).catch(console.error);
	});

	wsClient.on('failover_event', (event: WsEvent) => {
		console.log('Failover event:', event);
		// Refresh failover events
		api.getFailoverEvents().then(failoverEvents.set).catch(console.error);
	});

	wsClient.on('status_update', (event: WsEvent) => {
		console.log('Status update:', event);
	});
}

/**
 * Stop automatic polling and WebSocket
 */
export function stopPolling(): void {
	if (pollingInterval) {
		clearInterval(pollingInterval);
		pollingInterval = null;
	}
	wsClient.disconnect();
}

/**
 * Update polling interval
 */
export function updatePollingInterval(intervalMs: number): void {
	startPolling(intervalMs);
}
