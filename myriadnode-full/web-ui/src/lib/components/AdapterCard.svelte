<script lang="ts">
	import type { AdapterStatus } from '../api';

	export let adapter: AdapterStatus;

	function formatBandwidth(bps: number): string {
		if (bps >= 1_000_000_000) {
			return `${(bps / 1_000_000_000).toFixed(2)} Gbps`;
		} else if (bps >= 1_000_000) {
			return `${(bps / 1_000_000).toFixed(2)} Mbps`;
		} else if (bps >= 1_000) {
			return `${(bps / 1_000).toFixed(2)} Kbps`;
		}
		return `${bps} bps`;
	}

	function getHealthColor(status: string): string {
		switch (status) {
			case 'Healthy':
				return '#10b981';
			case 'Degraded':
				return '#f59e0b';
			case 'Failed':
				return '#ef4444';
			default:
				return '#6b7280';
		}
	}

	function getPrivacyLevel(level: number): string {
		if (level >= 0.8) return 'Very High';
		if (level >= 0.6) return 'High';
		if (level >= 0.4) return 'Medium';
		if (level >= 0.2) return 'Low';
		return 'Very Low';
	}
</script>

<div
	class="adapter-card"
	class:active={adapter.active}
	class:inactive={!adapter.active}
	style="--health-color: {getHealthColor(adapter.health_status)}"
>
	<div class="card-header">
		<div class="title-section">
			<h3>{adapter.adapter_id}</h3>
			<span class="adapter-type">{adapter.adapter_type}</span>
		</div>
		<div class="status-badges">
			{#if adapter.active}
				<span class="badge badge-active">Active</span>
			{:else}
				<span class="badge badge-inactive">Inactive</span>
			{/if}
			{#if adapter.is_backhaul}
				<span class="badge badge-backhaul">Backhaul</span>
			{/if}
		</div>
	</div>

	<div class="health-indicator">
		<div class="health-bar">
			<div class="health-fill" style="background-color: {getHealthColor(adapter.health_status)}"></div>
		</div>
		<span class="health-text" style="color: {getHealthColor(adapter.health_status)}">
			{adapter.health_status}
		</span>
	</div>

	<div class="metrics">
		<div class="metric">
			<span class="metric-label">Latency</span>
			<span class="metric-value">{adapter.metrics.latency_ms.toFixed(1)} ms</span>
		</div>
		<div class="metric">
			<span class="metric-label">Bandwidth</span>
			<span class="metric-value">{formatBandwidth(adapter.metrics.bandwidth_bps)}</span>
		</div>
		<div class="metric">
			<span class="metric-label">Reliability</span>
			<span class="metric-value">{(adapter.metrics.reliability * 100).toFixed(1)}%</span>
		</div>
		<div class="metric">
			<span class="metric-label">Privacy</span>
			<span class="metric-value privacy-{getPrivacyLevel(adapter.metrics.privacy_level).toLowerCase().replace(' ', '-')}">
				{getPrivacyLevel(adapter.metrics.privacy_level)}
			</span>
		</div>
	</div>
</div>

<style>
	.adapter-card {
		background: #1f2937;
		border: 2px solid #374151;
		border-radius: 8px;
		padding: 1rem;
		transition: all 0.2s ease;
	}

	.adapter-card.active {
		border-color: var(--health-color);
		box-shadow: 0 0 15px rgba(16, 185, 129, 0.2);
	}

	.adapter-card.inactive {
		opacity: 0.7;
	}

	.adapter-card:hover {
		transform: translateY(-2px);
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
	}

	.card-header {
		display: flex;
		justify-content: space-between;
		align-items: flex-start;
		margin-bottom: 0.75rem;
	}

	.title-section h3 {
		margin: 0;
		font-size: 1.25rem;
		color: #f9fafb;
		font-weight: 600;
	}

	.adapter-type {
		font-size: 0.875rem;
		color: #9ca3af;
		text-transform: uppercase;
	}

	.status-badges {
		display: flex;
		gap: 0.5rem;
	}

	.badge {
		padding: 0.25rem 0.5rem;
		border-radius: 4px;
		font-size: 0.75rem;
		font-weight: 600;
	}

	.badge-active {
		background: #10b981;
		color: #000;
	}

	.badge-inactive {
		background: #6b7280;
		color: #fff;
	}

	.badge-backhaul {
		background: #f59e0b;
		color: #000;
	}

	.health-indicator {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 1rem;
	}

	.health-bar {
		flex: 1;
		height: 8px;
		background: #374151;
		border-radius: 4px;
		overflow: hidden;
	}

	.health-fill {
		height: 100%;
		width: 100%;
		transition: background-color 0.3s ease;
	}

	.health-text {
		font-size: 0.875rem;
		font-weight: 600;
	}

	.metrics {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 0.75rem;
	}

	.metric {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.metric-label {
		font-size: 0.75rem;
		color: #9ca3af;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.metric-value {
		font-size: 1rem;
		color: #f9fafb;
		font-weight: 600;
	}

	.privacy-very-high {
		color: #10b981;
	}

	.privacy-high {
		color: #34d399;
	}

	.privacy-medium {
		color: #fbbf24;
	}

	.privacy-low {
		color: #f59e0b;
	}

	.privacy-very-low {
		color: #ef4444;
	}
</style>
