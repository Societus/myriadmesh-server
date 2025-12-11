<script lang="ts">
	import { nodeMap } from '$lib/stores';

	function formatTimestamp(timestamp: number): string {
		const date = new Date(timestamp * 1000);
		const now = Date.now();
		const diff = Math.floor((now - date.getTime()) / 1000);

		if (diff < 60) return `${diff}s ago`;
		if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
		if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
		return `${Math.floor(diff / 86400)}d ago`;
	}

	function getPrivacyColor(level: number): string {
		if (level >= 0.8) return '#10b981';
		if (level >= 0.6) return '#34d399';
		if (level >= 0.4) return '#fbbf24';
		if (level >= 0.2) return '#f59e0b';
		return '#ef4444';
	}

	function formatBandwidth(bps: number): string {
		if (bps >= 1_000_000_000) return `${(bps / 1_000_000_000).toFixed(1)} Gbps`;
		if (bps >= 1_000_000) return `${(bps / 1_000_000).toFixed(1)} Mbps`;
		if (bps >= 1_000) return `${(bps / 1_000).toFixed(1)} Kbps`;
		return `${bps} bps`;
	}
</script>

<div class="nodemap-page">
	<header class="page-header">
		<h1>NodeMap</h1>
		<p>Discovered nodes in the mesh network</p>
	</header>

	<div class="stats-bar">
		<div class="stat">
			<span class="stat-label">Total Nodes:</span>
			<span class="stat-value">{$nodeMap.length}</span>
		</div>
		<div class="stat">
			<span class="stat-label">Active Now:</span>
			<span class="stat-value">
				{$nodeMap.filter((n) => Date.now() / 1000 - n.last_seen < 300).length}
			</span>
		</div>
	</div>

	{#if $nodeMap.length === 0}
		<div class="empty-state">
			<p>No nodes discovered yet</p>
			<small>Nodes will appear here once heartbeats are received</small>
		</div>
	{:else}
		<div class="nodes-grid">
			{#each $nodeMap as node (node.node_id)}
				<div class="node-card">
					<div class="node-header">
						<div class="node-id">
							<span class="label">Node ID:</span>
							<code>{node.node_id.slice(0, 16)}...</code>
						</div>
						<div class="last-seen">{formatTimestamp(node.last_seen)}</div>
					</div>

					<div class="node-stats">
						<div class="stat-item">
							<span class="label">Heartbeats:</span>
							<span class="value">{node.heartbeat_count}</span>
						</div>
						<div class="stat-item">
							<span class="label">Adapters:</span>
							<span class="value">{node.adapters.length}</span>
						</div>
					</div>

					{#if node.adapters.length > 0}
						<div class="adapters-list">
							<h4>Adapters</h4>
							{#each node.adapters as adapter}
								<div class="adapter-item" class:inactive={!adapter.active}>
									<div class="adapter-info">
										<span class="adapter-name">{adapter.adapter_type}</span>
										{#if !adapter.active}
											<span class="inactive-badge">Inactive</span>
										{/if}
									</div>
									<div class="adapter-metrics">
										<span class="metric">
											{adapter.latency_ms}ms
										</span>
										<span class="metric">
											{formatBandwidth(adapter.bandwidth_bps)}
										</span>
										<span
											class="metric privacy"
											style="color: {getPrivacyColor(adapter.privacy_level)}"
										>
											Privacy: {(adapter.privacy_level * 100).toFixed(0)}%
										</span>
									</div>
								</div>
							{/each}
						</div>
					{/if}
				</div>
			{/each}
		</div>
	{/if}
</div>

<style>
	.nodemap-page {
		max-width: 1400px;
		margin: 0 auto;
	}

	.page-header {
		margin-bottom: 2rem;
	}

	.page-header h1 {
		margin: 0 0 0.5rem 0;
		font-size: 2rem;
		color: #f9fafb;
	}

	.page-header p {
		margin: 0;
		color: #9ca3af;
	}

	.stats-bar {
		display: flex;
		gap: 2rem;
		padding: 1.25rem;
		background: #1f2937;
		border: 2px solid #374151;
		border-radius: 8px;
		margin-bottom: 2rem;
	}

	.stats-bar .stat {
		display: flex;
		gap: 0.5rem;
		align-items: center;
	}

	.stats-bar .stat-label {
		color: #9ca3af;
		font-size: 0.875rem;
	}

	.stats-bar .stat-value {
		color: #3b82f6;
		font-size: 1.5rem;
		font-weight: 700;
	}

	.empty-state {
		background: #1f2937;
		border: 2px dashed #374151;
		border-radius: 8px;
		padding: 4rem;
		text-align: center;
	}

	.empty-state p {
		color: #9ca3af;
		font-size: 1.125rem;
		margin: 0 0 0.5rem 0;
	}

	.empty-state small {
		color: #6b7280;
	}

	.nodes-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(450px, 1fr));
		gap: 1.5rem;
	}

	.node-card {
		background: #1f2937;
		border: 2px solid #374151;
		border-radius: 8px;
		padding: 1.25rem;
		transition: all 0.2s ease;
	}

	.node-card:hover {
		border-color: #3b82f6;
		box-shadow: 0 0 20px rgba(59, 130, 246, 0.15);
	}

	.node-header {
		display: flex;
		justify-content: space-between;
		align-items: flex-start;
		margin-bottom: 1rem;
		padding-bottom: 0.75rem;
		border-bottom: 1px solid #374151;
	}

	.node-id {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.node-id .label {
		font-size: 0.75rem;
		color: #6b7280;
		text-transform: uppercase;
	}

	.node-id code {
		font-family: 'Courier New', monospace;
		font-size: 0.875rem;
		color: #3b82f6;
		background: #374151;
		padding: 0.25rem 0.5rem;
		border-radius: 4px;
	}

	.last-seen {
		font-size: 0.75rem;
		color: #9ca3af;
	}

	.node-stats {
		display: flex;
		gap: 1.5rem;
		margin-bottom: 1rem;
	}

	.stat-item {
		display: flex;
		gap: 0.5rem;
	}

	.stat-item .label {
		color: #9ca3af;
		font-size: 0.875rem;
	}

	.stat-item .value {
		color: #f9fafb;
		font-weight: 600;
		font-size: 0.875rem;
	}

	.adapters-list h4 {
		margin: 0 0 0.75rem 0;
		font-size: 0.875rem;
		color: #9ca3af;
		text-transform: uppercase;
	}

	.adapter-item {
		background: #374151;
		border-radius: 6px;
		padding: 0.75rem;
		margin-bottom: 0.5rem;
	}

	.adapter-item.inactive {
		opacity: 0.6;
	}

	.adapter-info {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 0.5rem;
	}

	.adapter-name {
		color: #f9fafb;
		font-weight: 600;
		text-transform: capitalize;
	}

	.inactive-badge {
		font-size: 0.75rem;
		color: #6b7280;
		background: #1f2937;
		padding: 0.125rem 0.5rem;
		border-radius: 4px;
	}

	.adapter-metrics {
		display: flex;
		gap: 1rem;
		font-size: 0.75rem;
	}

	.metric {
		color: #9ca3af;
	}

	.metric.privacy {
		font-weight: 600;
	}
</style>
