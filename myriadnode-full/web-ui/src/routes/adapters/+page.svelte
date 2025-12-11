<script lang="ts">
	import { adapters, failoverEvents } from '$lib/stores';
	import { startAdapter, stopAdapter, forceFailover } from '$lib/api';
	import AdapterCard from '$lib/components/AdapterCard.svelte';

	let selectedAdapter: string | null = null;
	let isActing = false;

	async function handleStartAdapter(adapterId: string) {
		isActing = true;
		try {
			await startAdapter(adapterId);
		} catch (err) {
			console.error('Failed to start adapter:', err);
			alert('Failed to start adapter: ' + err);
		} finally {
			isActing = false;
		}
	}

	async function handleStopAdapter(adapterId: string) {
		isActing = true;
		try {
			await stopAdapter(adapterId);
		} catch (err) {
			console.error('Failed to stop adapter:', err);
			alert('Failed to stop adapter: ' + err);
		} finally {
			isActing = false;
		}
	}

	async function handleForceFailover(adapterId: string) {
		if (!confirm(`Force failover to ${adapterId}?`)) {
			return;
		}

		isActing = true;
		try {
			await forceFailover(adapterId);
		} catch (err) {
			console.error('Failed to force failover:', err);
			alert('Failed to force failover: ' + err);
		} finally {
			isActing = false;
		}
	}

	function formatTimestamp(timestamp: number): string {
		return new Date(timestamp * 1000).toLocaleString();
	}
</script>

<div class="adapters-page">
	<header class="page-header">
		<h1>Network Adapters</h1>
		<p>Manage and monitor all network adapters</p>
	</header>

	<!-- Adapters Grid -->
	<section class="adapters-section">
		<div class="adapters-grid">
			{#each $adapters as adapter (adapter.adapter_id)}
				<div class="adapter-wrapper">
					<AdapterCard {adapter} />
					<div class="adapter-actions">
						{#if adapter.active}
							<button
								class="btn btn-secondary"
								disabled={isActing}
								on:click={() => handleStopAdapter(adapter.adapter_id)}
							>
								Stop
							</button>
						{:else}
							<button
								class="btn btn-primary"
								disabled={isActing}
								on:click={() => handleStartAdapter(adapter.adapter_id)}
							>
								Start
							</button>
						{/if}
						<button
							class="btn btn-accent"
							disabled={isActing || !adapter.active}
							on:click={() => handleForceFailover(adapter.adapter_id)}
						>
							Force Failover
						</button>
					</div>
				</div>
			{/each}
		</div>
	</section>

	<!-- Failover Events -->
	<section class="events-section">
		<h2>Recent Failover Events</h2>
		{#if $failoverEvents.length === 0}
			<div class="empty-state">
				<p>No failover events recorded</p>
			</div>
		{:else}
			<div class="events-list">
				{#each $failoverEvents as event}
					<div class="event-item" class:critical={event.event_type === 'AdapterDown'}>
						<div class="event-header">
							<span class="event-type">{event.event_type}</span>
							<span class="event-time">{formatTimestamp(event.timestamp)}</span>
						</div>
						<div class="event-details">{event.details}</div>
					</div>
				{/each}
			</div>
		{/if}
	</section>
</div>

<style>
	.adapters-page {
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

	.adapters-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(380px, 1fr));
		gap: 1.5rem;
		margin-bottom: 2rem;
	}

	.adapter-wrapper {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.adapter-actions {
		display: flex;
		gap: 0.5rem;
	}

	.btn {
		flex: 1;
		padding: 0.625rem 1rem;
		border: none;
		border-radius: 6px;
		font-size: 0.875rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s ease;
	}

	.btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.btn-primary {
		background: #10b981;
		color: #000;
	}

	.btn-primary:hover:not(:disabled) {
		background: #059669;
	}

	.btn-secondary {
		background: #6b7280;
		color: #fff;
	}

	.btn-secondary:hover:not(:disabled) {
		background: #4b5563;
	}

	.btn-accent {
		background: #3b82f6;
		color: #fff;
	}

	.btn-accent:hover:not(:disabled) {
		background: #2563eb;
	}

	.events-section {
		margin-top: 3rem;
	}

	.events-section h2 {
		font-size: 1.5rem;
		color: #f9fafb;
		margin-bottom: 1rem;
	}

	.events-list {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.event-item {
		background: #1f2937;
		border: 2px solid #374151;
		border-radius: 8px;
		padding: 1rem;
		transition: border-color 0.2s ease;
	}

	.event-item.critical {
		border-color: #ef4444;
	}

	.event-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 0.5rem;
	}

	.event-type {
		font-weight: 600;
		color: #3b82f6;
		font-size: 0.875rem;
	}

	.event-item.critical .event-type {
		color: #ef4444;
	}

	.event-time {
		font-size: 0.75rem;
		color: #6b7280;
	}

	.event-details {
		color: #9ca3af;
		font-size: 0.875rem;
	}

	.empty-state {
		background: #1f2937;
		border: 2px dashed #374151;
		border-radius: 8px;
		padding: 3rem;
		text-align: center;
		color: #6b7280;
	}
</style>
