<script lang="ts">
	import { refreshInterval } from '$lib/stores';
	import { updatePollingInterval } from '$lib/dataService';

	let intervalValue = $refreshInterval / 1000; // Convert to seconds

	function handleIntervalChange() {
		const newInterval = intervalValue * 1000;
		refreshInterval.set(newInterval);
		updatePollingInterval(newInterval);
	}
</script>

<div class="settings-page">
	<header class="page-header">
		<h1>Settings</h1>
		<p>Configure the dashboard</p>
	</header>

	<section class="settings-section">
		<h2>General Settings</h2>

		<div class="setting-group">
			<label for="refresh-interval">
				<span class="label-text">Auto-refresh Interval</span>
				<span class="label-description">How often to fetch new data from the backend (seconds)</span>
			</label>
			<div class="input-group">
				<input
					id="refresh-interval"
					type="number"
					min="1"
					max="60"
					bind:value={intervalValue}
					on:change={handleIntervalChange}
				/>
				<span class="unit">seconds</span>
			</div>
		</div>
	</section>

	<section class="settings-section">
		<h2>About</h2>
		<div class="about-card">
			<h3>MyriadNode Dashboard</h3>
			<p>Version 0.1.0</p>
			<p>A lightweight Svelte-based dashboard for managing and monitoring MyriadNode mesh networks.</p>
			<div class="tech-stack">
				<span class="tech-badge">Svelte</span>
				<span class="tech-badge">SvelteKit</span>
				<span class="tech-badge">TypeScript</span>
			</div>
		</div>
	</section>

	<section class="settings-section">
		<h2>Privacy</h2>
		<div class="info-card">
			<p>
				MyriadNode is privacy-first by design. This dashboard communicates only with your local
				MyriadNode instance and does not send any telemetry or analytics data to external
				services.
			</p>
			<ul>
				<li>No tracking or analytics</li>
				<li>All data stays on your device</li>
				<li>Geolocation sharing disabled by default</li>
				<li>Open source and auditable</li>
			</ul>
		</div>
	</section>
</div>

<style>
	.settings-page {
		max-width: 900px;
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

	.settings-section {
		margin-bottom: 2rem;
	}

	.settings-section h2 {
		font-size: 1.25rem;
		color: #f9fafb;
		margin-bottom: 1rem;
	}

	.setting-group {
		background: #1f2937;
		border: 2px solid #374151;
		border-radius: 8px;
		padding: 1.25rem;
		margin-bottom: 1rem;
	}

	label {
		display: block;
		margin-bottom: 0.75rem;
	}

	.label-text {
		display: block;
		color: #f9fafb;
		font-weight: 600;
		margin-bottom: 0.25rem;
	}

	.label-description {
		display: block;
		color: #9ca3af;
		font-size: 0.875rem;
	}

	.input-group {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	input[type='number'] {
		flex: 1;
		max-width: 200px;
		padding: 0.625rem;
		background: #374151;
		border: 2px solid #4b5563;
		border-radius: 6px;
		color: #f9fafb;
		font-size: 1rem;
	}

	input[type='number']:focus {
		outline: none;
		border-color: #3b82f6;
	}

	.unit {
		color: #9ca3af;
		font-size: 0.875rem;
	}

	.about-card,
	.info-card {
		background: #1f2937;
		border: 2px solid #374151;
		border-radius: 8px;
		padding: 1.5rem;
	}

	.about-card h3 {
		margin: 0 0 0.5rem 0;
		color: #f9fafb;
	}

	.about-card p {
		color: #9ca3af;
		margin: 0.25rem 0;
	}

	.tech-stack {
		display: flex;
		gap: 0.5rem;
		margin-top: 1rem;
	}

	.tech-badge {
		background: #374151;
		color: #3b82f6;
		padding: 0.375rem 0.75rem;
		border-radius: 4px;
		font-size: 0.875rem;
		font-weight: 600;
	}

	.info-card p {
		color: #9ca3af;
		margin: 0 0 1rem 0;
		line-height: 1.6;
	}

	.info-card ul {
		list-style: none;
		padding: 0;
		margin: 0;
	}

	.info-card li {
		color: #9ca3af;
		padding: 0.5rem 0;
		padding-left: 1.5rem;
		position: relative;
	}

	.info-card li::before {
		content: '✓';
		position: absolute;
		left: 0;
		color: #10b981;
		font-weight: bold;
	}
</style>
