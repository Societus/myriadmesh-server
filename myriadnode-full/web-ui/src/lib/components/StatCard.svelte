<script lang="ts">
	export let title: string;
	export let value: string | number;
	export let subtitle: string = '';
	export let color: string = '#3b82f6';
</script>

<div class="stat-card" style="--accent-color: {color}">
	<div class="stat-header">
		<h4>{title}</h4>
	</div>
	<div class="stat-value">{value}</div>
	{#if subtitle}
		<div class="stat-subtitle">{subtitle}</div>
	{/if}
</div>

<style>
	.stat-card {
		background: #1f2937;
		border: 2px solid #374151;
		border-radius: 8px;
		padding: 1.25rem;
		transition: all 0.2s ease;
	}

	.stat-card:hover {
		border-color: var(--accent-color);
		box-shadow: 0 0 20px rgba(59, 130, 246, 0.15);
	}

	.stat-header h4 {
		margin: 0;
		font-size: 0.875rem;
		color: #9ca3af;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		font-weight: 600;
	}

	.stat-value {
		font-size: 2.5rem;
		font-weight: 700;
		color: var(--accent-color);
		margin: 0.5rem 0;
		line-height: 1;
	}

	.stat-subtitle {
		font-size: 0.875rem;
		color: #6b7280;
		margin-top: 0.5rem;
	}
</style>
