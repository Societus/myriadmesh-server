<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { page } from '$app/stores';
	import { startPolling, stopPolling } from '$lib/dataService';
	import { error as errorStore } from '$lib/stores';

	let currentPath = '';

	onMount(() => {
		// Start data polling when app loads
		startPolling(5000);
	});

	onDestroy(() => {
		// Stop polling when app unloads
		stopPolling();
	});

	$: currentPath = $page.url.pathname;

	const navItems = [
		{ path: '/', label: 'Dashboard', icon: '📊' },
		{ path: '/adapters', label: 'Adapters', icon: '🔌' },
		{ path: '/messages', label: 'Messages', icon: '✉️' },
		{ path: '/nodemap', label: 'NodeMap', icon: '🗺️' },
		{ path: '/settings', label: 'Settings', icon: '⚙️' }
	];
</script>

<div class="app">
	<nav class="sidebar">
		<div class="logo">
			<h1>🌐 MyriadNode</h1>
		</div>

		<ul class="nav-menu">
			{#each navItems as item}
				<li>
					<a href={item.path} class:active={currentPath === item.path}>
						<span class="icon">{item.icon}</span>
						<span class="label">{item.label}</span>
					</a>
				</li>
			{/each}
		</ul>

		<div class="sidebar-footer">
			<div class="version">v0.1.0</div>
		</div>
	</nav>

	<main class="content">
		{#if $errorStore}
			<div class="error-banner">
				<strong>⚠️ Error:</strong> {$errorStore}
			</div>
		{/if}

		<slot />
	</main>
</div>

<style>
	:global(body) {
		margin: 0;
		padding: 0;
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell,
			sans-serif;
		background: #111827;
		color: #f9fafb;
	}

	:global(*) {
		box-sizing: border-box;
	}

	.app {
		display: flex;
		min-height: 100vh;
	}

	.sidebar {
		width: 250px;
		background: #1f2937;
		border-right: 2px solid #374151;
		display: flex;
		flex-direction: column;
	}

	.logo {
		padding: 1.5rem;
		border-bottom: 2px solid #374151;
	}

	.logo h1 {
		margin: 0;
		font-size: 1.5rem;
		color: #f9fafb;
	}

	.nav-menu {
		flex: 1;
		list-style: none;
		margin: 0;
		padding: 1rem 0;
	}

	.nav-menu li {
		margin: 0;
	}

	.nav-menu a {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.75rem 1.5rem;
		color: #9ca3af;
		text-decoration: none;
		transition: all 0.2s ease;
	}

	.nav-menu a:hover {
		background: #374151;
		color: #f9fafb;
	}

	.nav-menu a.active {
		background: #374151;
		color: #3b82f6;
		border-left: 3px solid #3b82f6;
	}

	.nav-menu .icon {
		font-size: 1.25rem;
	}

	.sidebar-footer {
		padding: 1rem 1.5rem;
		border-top: 2px solid #374151;
		color: #6b7280;
		font-size: 0.875rem;
	}

	.content {
		flex: 1;
		padding: 2rem;
		overflow-y: auto;
	}

	.error-banner {
		background: #dc2626;
		color: #fff;
		padding: 1rem;
		border-radius: 8px;
		margin-bottom: 1.5rem;
	}
</style>
