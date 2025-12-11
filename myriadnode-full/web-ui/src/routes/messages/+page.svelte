<script lang="ts">
	import { onMount } from 'svelte';
	import { nodeInfo, adapters } from '$lib/stores';
	import { sendMessage as sendMessageAPI } from '$lib/api';

	interface Message {
		id: string;
		destination: string;
		content: string;
		adapter: string;
		timestamp: number;
		status: 'pending' | 'sent' | 'failed';
	}

	let messages: Message[] = [];
	let newMessage = {
		destination: '',
		content: '',
		adapter: 'auto'
	};

	let sending = false;
	let error: string | null = null;
	let success: string | null = null;

	function addMessage(msg: Message) {
		messages = [msg, ...messages];
	}

	async function sendMessage() {
		if (!newMessage.destination || !newMessage.content) {
			error = 'Please enter both destination and message';
			return;
		}

		sending = true;
		error = null;
		success = null;

		const message: Message = {
			id: Math.random().toString(36).substring(7),
			destination: newMessage.destination,
			content: newMessage.content,
			adapter: newMessage.adapter,
			timestamp: Date.now(),
			status: 'pending'
		};

		addMessage(message);

		try {
			// Call the API to send the message
			const result = await sendMessageAPI({
				destination: newMessage.destination,
				payload: newMessage.content,
				priority: 128 // Normal priority
			});

			// Update message with actual ID from server
			message.id = result.message_id;
			message.status = 'sent';
			messages = [...messages];

			success = 'Message sent successfully!';

			// Clear form
			newMessage = {
				destination: '',
				content: '',
				adapter: 'auto'
			};

			// Clear success message after 3 seconds
			setTimeout(() => {
				success = null;
			}, 3000);

		} catch (err) {
			message.status = 'failed';
			messages = [...messages];
			error = err instanceof Error ? err.message : 'Failed to send message';
		} finally {
			sending = false;
		}
	}

	function formatTimestamp(timestamp: number): string {
		const date = new Date(timestamp);
		return date.toLocaleString();
	}

	function getStatusColor(status: string): string {
		switch (status) {
			case 'sent':
				return '#10b981';
			case 'pending':
				return '#f59e0b';
			case 'failed':
				return '#ef4444';
			default:
				return '#6b7280';
		}
	}

	function deleteMessage(id: string) {
		messages = messages.filter(m => m.id !== id);
	}
</script>

<div class="messages-page">
	<header class="page-header">
		<h1>Messages</h1>
		<p class="subtitle">Send messages to other nodes in the mesh network</p>
	</header>

	<!-- Composer -->
	<section class="composer-section">
		<h2>Compose Message</h2>

		{#if error}
			<div class="alert alert-error">
				⚠️ {error}
			</div>
		{/if}

		{#if success}
			<div class="alert alert-success">
				✅ {success}
			</div>
		{/if}

		<form on:submit|preventDefault={sendMessage} class="composer-form">
			<div class="form-group">
				<label for="destination">Destination Node ID</label>
				<input
					id="destination"
					type="text"
					bind:value={newMessage.destination}
					placeholder="Enter 64-character hex NodeID"
					maxlength="128"
					disabled={sending}
					class="form-input"
				/>
				<small class="form-help">The NodeID of the destination node (64 bytes hex-encoded)</small>
			</div>

			<div class="form-group">
				<label for="adapter">Adapter</label>
				<select id="adapter" bind:value={newMessage.adapter} disabled={sending} class="form-select">
					<option value="auto">Auto (Best Available)</option>
					{#each $adapters.filter(a => a.active) as adapter}
						<option value={adapter.adapter_id}>
							{adapter.adapter_id} ({adapter.adapter_type})
						</option>
					{/each}
				</select>
				<small class="form-help">Choose a specific adapter or let the system select automatically</small>
			</div>

			<div class="form-group">
				<label for="content">Message Content</label>
				<textarea
					id="content"
					bind:value={newMessage.content}
					placeholder="Enter your message here..."
					rows="5"
					disabled={sending}
					maxlength="1024"
					class="form-textarea"
				/>
				<small class="form-help">{newMessage.content.length} / 1024 characters</small>
			</div>

			<div class="form-actions">
				<button type="submit" disabled={sending} class="btn btn-primary">
					{#if sending}
						<span class="spinner-small"></span>
						Sending...
					{:else}
						Send Message
					{/if}
				</button>
			</div>
		</form>
	</section>

	<!-- Message History -->
	<section class="history-section">
		<h2>Recent Messages ({messages.length})</h2>

		{#if messages.length === 0}
			<div class="empty-state">
				<p>No messages yet. Send your first message above!</p>
			</div>
		{:else}
			<div class="messages-list">
				{#each messages as message (message.id)}
					<div class="message-card" style="--status-color: {getStatusColor(message.status)}">
						<div class="message-header">
							<div class="message-info">
								<span class="message-status" style="background-color: {getStatusColor(message.status)}">
									{message.status}
								</span>
								<span class="message-time">{formatTimestamp(message.timestamp)}</span>
							</div>
							<button on:click={() => deleteMessage(message.id)} class="btn-delete" aria-label="Delete message">
								🗑️
							</button>
						</div>

						<div class="message-body">
							<div class="message-field">
								<span class="field-label">To:</span>
								<span class="field-value">{message.destination.substring(0, 16)}...</span>
							</div>
							<div class="message-field">
								<span class="field-label">Via:</span>
								<span class="field-value">{message.adapter}</span>
							</div>
							<div class="message-content">
								{message.content}
							</div>
						</div>
					</div>
				{/each}
			</div>
		{/if}
	</section>
</div>

<style>
	.messages-page {
		max-width: 1200px;
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

	.subtitle {
		color: #9ca3af;
		margin: 0;
	}

	.composer-section,
	.history-section {
		background: #1f2937;
		border: 2px solid #374151;
		border-radius: 8px;
		padding: 1.5rem;
		margin-bottom: 2rem;
	}

	.composer-section h2,
	.history-section h2 {
		margin: 0 0 1.5rem 0;
		font-size: 1.25rem;
		color: #f9fafb;
	}

	.alert {
		padding: 1rem;
		border-radius: 6px;
		margin-bottom: 1rem;
	}

	.alert-error {
		background: rgba(239, 68, 68, 0.1);
		border: 2px solid #ef4444;
		color: #fca5a5;
	}

	.alert-success {
		background: rgba(16, 185, 129, 0.1);
		border: 2px solid #10b981;
		color: #6ee7b7;
	}

	.composer-form {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
	}

	.form-group {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.form-group label {
		color: #f9fafb;
		font-weight: 600;
		font-size: 0.875rem;
	}

	.form-input,
	.form-select,
	.form-textarea {
		background: #374151;
		border: 2px solid #4b5563;
		border-radius: 6px;
		padding: 0.75rem;
		color: #f9fafb;
		font-size: 1rem;
		font-family: 'Monaco', 'Courier New', monospace;
		transition: border-color 0.2s;
	}

	.form-input:focus,
	.form-select:focus,
	.form-textarea:focus {
		outline: none;
		border-color: #3b82f6;
	}

	.form-input:disabled,
	.form-select:disabled,
	.form-textarea:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.form-textarea {
		resize: vertical;
		min-height: 120px;
	}

	.form-help {
		color: #9ca3af;
		font-size: 0.75rem;
	}

	.form-actions {
		display: flex;
		justify-content: flex-end;
	}

	.btn {
		padding: 0.75rem 1.5rem;
		border-radius: 6px;
		border: none;
		font-size: 1rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s;
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.btn-primary {
		background: #3b82f6;
		color: #fff;
	}

	.btn-primary:hover:not(:disabled) {
		background: #2563eb;
	}

	.btn-primary:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.spinner-small {
		width: 16px;
		height: 16px;
		border: 2px solid rgba(255, 255, 255, 0.3);
		border-top-color: #fff;
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}

	.messages-list {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.message-card {
		background: #374151;
		border: 2px solid #4b5563;
		border-left: 4px solid var(--status-color);
		border-radius: 6px;
		padding: 1rem;
		transition: all 0.2s;
	}

	.message-card:hover {
		border-color: var(--status-color);
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
	}

	.message-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 0.75rem;
		padding-bottom: 0.75rem;
		border-bottom: 1px solid #4b5563;
	}

	.message-info {
		display: flex;
		gap: 1rem;
		align-items: center;
	}

	.message-status {
		padding: 0.25rem 0.75rem;
		border-radius: 4px;
		font-size: 0.75rem;
		font-weight: 600;
		color: #000;
		text-transform: uppercase;
	}

	.message-time {
		color: #9ca3af;
		font-size: 0.875rem;
	}

	.btn-delete {
		background: none;
		border: none;
		font-size: 1.25rem;
		cursor: pointer;
		padding: 0.25rem;
		opacity: 0.6;
		transition: opacity 0.2s;
	}

	.btn-delete:hover {
		opacity: 1;
	}

	.message-body {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.message-field {
		display: flex;
		gap: 0.5rem;
		font-size: 0.875rem;
	}

	.field-label {
		color: #9ca3af;
		font-weight: 600;
	}

	.field-value {
		color: #f9fafb;
		font-family: 'Monaco', 'Courier New', monospace;
	}

	.message-content {
		margin-top: 0.5rem;
		padding: 0.75rem;
		background: #1f2937;
		border-radius: 4px;
		color: #f9fafb;
		white-space: pre-wrap;
		word-break: break-word;
	}

	.empty-state {
		background: #374151;
		border: 2px dashed #4b5563;
		border-radius: 8px;
		padding: 3rem;
		text-align: center;
		color: #6b7280;
	}
</style>
