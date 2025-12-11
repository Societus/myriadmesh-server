/**
 * WebSocket client for real-time updates from MyriadNode
 */

export type WsEventType =
	| 'node_info'
	| 'adapter_status'
	| 'message_received'
	| 'message_sent'
	| 'heartbeat_update'
	| 'failover_event'
	| 'dht_node_discovered'
	| 'status_update';

export interface WsEvent {
	type: WsEventType;
	[key: string]: any;
}

export interface NodeInfoEvent extends WsEvent {
	type: 'node_info';
	id: string;
	name: string;
	version: string;
	uptime_secs: number;
}

export interface AdapterStatusEvent extends WsEvent {
	type: 'adapter_status';
	adapter_id: string;
	active: boolean;
	health_status: string;
}

export interface MessageReceivedEvent extends WsEvent {
	type: 'message_received';
	message_id: string;
	source: string;
	content: string;
	timestamp: number;
}

export interface MessageSentEvent extends WsEvent {
	type: 'message_sent';
	message_id: string;
	destination: string;
	timestamp: number;
}

export interface HeartbeatUpdateEvent extends WsEvent {
	type: 'heartbeat_update';
	total_nodes: number;
	nodes_with_location: number;
}

export interface FailoverEventEvent extends WsEvent {
	type: 'failover_event';
	from_adapter: string;
	to_adapter: string;
	reason: string;
	timestamp: number;
}

export interface StatusUpdateEvent extends WsEvent {
	type: 'status_update';
	message: string;
}

type EventHandler = (event: WsEvent) => void;

export class WebSocketClient {
	private ws: WebSocket | null = null;
	private reconnectAttempts = 0;
	private maxReconnectAttempts = 5;
	private reconnectDelay = 1000;
	private handlers = new Map<WsEventType | 'all', Set<EventHandler>>();
	private isConnecting = false;

	constructor(private url: string = `ws://${window.location.host}/ws`) {}

	/**
	 * Connect to the WebSocket server
	 */
	connect(): void {
		if (this.ws?.readyState === WebSocket.OPEN || this.isConnecting) {
			return;
		}

		this.isConnecting = true;

		try {
			this.ws = new WebSocket(this.url);

			this.ws.onopen = () => {
				console.log('WebSocket connected');
				this.isConnecting = false;
				this.reconnectAttempts = 0;
			};

			this.ws.onmessage = (event) => {
				try {
					const data = JSON.parse(event.data) as WsEvent;
					this.handleEvent(data);
				} catch (e) {
					console.error('Failed to parse WebSocket message:', e);
				}
			};

			this.ws.onerror = (error) => {
				console.error('WebSocket error:', error);
				this.isConnecting = false;
			};

			this.ws.onclose = () => {
				console.log('WebSocket closed');
				this.isConnecting = false;
				this.reconnect();
			};
		} catch (error) {
			console.error('Failed to create WebSocket:', error);
			this.isConnecting = false;
			this.reconnect();
		}
	}

	/**
	 * Disconnect from the WebSocket server
	 */
	disconnect(): void {
		if (this.ws) {
			this.ws.close();
			this.ws = null;
		}
	}

	/**
	 * Attempt to reconnect
	 */
	private reconnect(): void {
		if (this.reconnectAttempts >= this.maxReconnectAttempts) {
			console.error('Max reconnect attempts reached');
			return;
		}

		this.reconnectAttempts++;
		const delay = this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1);

		console.log(`Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts})...`);

		setTimeout(() => {
			this.connect();
		}, delay);
	}

	/**
	 * Register an event handler
	 */
	on(eventType: WsEventType | 'all', handler: EventHandler): void {
		if (!this.handlers.has(eventType)) {
			this.handlers.set(eventType, new Set());
		}
		this.handlers.get(eventType)!.add(handler);
	}

	/**
	 * Unregister an event handler
	 */
	off(eventType: WsEventType | 'all', handler: EventHandler): void {
		const handlers = this.handlers.get(eventType);
		if (handlers) {
			handlers.delete(handler);
		}
	}

	/**
	 * Handle incoming event
	 */
	private handleEvent(event: WsEvent): void {
		// Call type-specific handlers
		const typeHandlers = this.handlers.get(event.type);
		if (typeHandlers) {
			typeHandlers.forEach((handler) => handler(event));
		}

		// Call 'all' handlers
		const allHandlers = this.handlers.get('all');
		if (allHandlers) {
			allHandlers.forEach((handler) => handler(event));
		}
	}

	/**
	 * Check if connected
	 */
	isConnected(): boolean {
		return this.ws?.readyState === WebSocket.OPEN;
	}
}

// Create singleton instance
export const wsClient = new WebSocketClient();
