<script lang="ts">
	import { onMount } from 'svelte';
	import AdapterCard from '$lib/components/AdapterCard.svelte';
	import StatCard from '$lib/components/StatCard.svelte';
	import {
		nodeInfo,
		adapters,
		heartbeatStats,
		activeAdaptersCount,
		healthyAdaptersCount,
		primaryAdapter,
		totalKnownNodes,
		isLoading
	} from '$lib/stores';

	function formatUptime(seconds: number): string {
		const days = Math.floor(seconds / 86400);
		const hours = Math.floor((seconds % 86400) / 3600);
		const minutes = Math.floor((seconds % 3600) / 60);

		if (days > 0) {
			return `${days}d ${hours}h ${minutes}m`;
		} else if (hours > 0) {
			return `${hours}h ${minutes}m`;
		} else {
			return `${minutes}m`;
		}
	}
</script>

<div class="dashboard">
	<header class="page-header">
		<h1>Dashboard</h1>
		{#if $nodeInfo}
			<div class="node-info">
				<span class="node-name">{$nodeInfo.name}</span>
				<span class="uptime">Uptime: {formatUptime($nodeInfo.uptime_secs)}</span>
			</div>
		{/if}
	</header>

	{#if $isLoading && !$nodeInfo}
		<div class="loading">
			<div class="spinner"></div>
			<p>Loading dashboard...</p>
		</div>
	{:else}
		<!-- Statistics Overview -->
		<section class="stats-grid">
			<StatCard title="Active Adapters" value={$activeAdaptersCount} color="#10b981" />
			<StatCard title="Healthy Adapters" value={$healthyAdaptersCount} color="#3b82f6" />
			<StatCard title="Known Nodes" value={$totalKnownNodes} color="#8b5cf6" />
			<StatCard
				title="Primary Adapter"
				value={$primaryAdapter?.adapter_id || 'None'}
				subtitle={$primaryAdapter ? $primaryAdapter.adapter_type : ''}
				color="#f59e0b"
			/>
		</section>

		<!-- Adapters Overview -->
		<section class="adapters-section">
			<h2>Network Adapters</h2>
			{#if $adapters.length === 0}
				<div class="empty-state">
					<p>No adapters available</p>
				</div>
			{:else}
				<div class="adapters-grid">
					{#each $adapters as adapter (adapter.adapter_id)}
						<AdapterCard {adapter} />
					{/each}
				</div>
			{/if}
		</section>

		<!-- Heartbeat Stats -->
		{#if $heartbeatStats}
			<section class="heartbeat-section">
				<h2>Heartbeat Statistics</h2>
				<div class="heartbeat-info">
					<div class="stat-item">
						<span class="stat-label">Total Nodes:</span>
						<span class="stat-value">{$heartbeatStats.total_nodes}</span>
					</div>
					<div class="stat-item">
						<span class="stat-label">Nodes with Location:</span>
						<span class="stat-value">{$heartbeatStats.nodes_with_location}</span>
					</div>
				</div>

				{#if Object.keys($heartbeatStats.adapter_counts).length > 0}
					<div class="adapter-distribution">
						<h3>Adapter Distribution</h3>
						<div class="distribution-grid">
							{#each Object.entries($heartbeatStats.adapter_counts) as [type, count]}
								<div class="distribution-item">
									<span class="type">{type}</span>
									<span class="count">{count} nodes</span>
								</div>
							{/each}
						</div>
					</div>
				{/if}
			</section>
		{/if}
	{/if}
</div>

<style>
	.dashboard {
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

	.node-info {
		display: flex;
		gap: 1rem;
		align-items: center;
		color: #9ca3af;
		font-size: 0.875rem;
	}

	.node-name {
		color: #3b82f6;
		font-weight: 600;
	}

	.loading {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		padding: 4rem;
		color: #9ca3af;
	}

	.spinner {
		width: 40px;
		height: 40px;
		border: 4px solid #374151;
		border-top-color: #3b82f6;
		border-radius: 50%;
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}

	.stats-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
		gap: 1.5rem;
		margin-bottom: 2rem;
	}

	.adapters-section,
	.heartbeat-section {
		margin-top: 2rem;
	}

	.adapters-section h2,
	.heartbeat-section h2 {
		font-size: 1.5rem;
		color: #f9fafb;
		margin-bottom: 1rem;
	}

	.adapters-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(350px, 1fr));
		gap: 1.5rem;
	}

	.empty-state {
		background: #1f2937;
		border: 2px dashed #374151;
		border-radius: 8px;
		padding: 3rem;
		text-align: center;
		color: #6b7280;
	}

	.heartbeat-info {
		background: #1f2937;
		border: 2px solid #374151;
		border-radius: 8px;
		padding: 1.5rem;
		display: flex;
		gap: 2rem;
		margin-bottom: 1rem;
	}

	.stat-item {
		display: flex;
		gap: 0.5rem;
	}

	.stat-label {
		color: #9ca3af;
	}

	.stat-value {
		color: #f9fafb;
		font-weight: 600;
	}

	.adapter-distribution {
		background: #1f2937;
		border: 2px solid #374151;
		border-radius: 8px;
		padding: 1.5rem;
	}

	.adapter-distribution h3 {
		margin: 0 0 1rem 0;
		font-size: 1rem;
		color: #9ca3af;
	}

	.distribution-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
		gap: 1rem;
	}

	.distribution-item {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 0.75rem;
		background: #374151;
		border-radius: 4px;
	}

	.distribution-item .type {
		color: #f9fafb;
		font-weight: 600;
		text-transform: capitalize;
	}

	.distribution-item .count {
		color: #3b82f6;
		font-size: 0.875rem;
	}
</style>
