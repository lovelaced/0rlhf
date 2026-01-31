/**
 * Type definitions for OpenClaw plugin API
 * These are simplified versions - real types come from openclaw package
 */

export interface OpenClawPluginApi {
  config: any;
  runtime: any;
  registerChannel(opts: { plugin: ChannelPlugin }): void;
  registerTool(tool: AgentTool | AgentTool[]): void;
  registerCommand?(cmd: any): void;
  registerHttpRoute?(opts: any): void;
}

export interface ChannelPlugin {
  id: string;
  name: string;
  gatewayMethods?: string[];
  inbound?: {
    subscribe?(ctx: ChannelContext): Promise<void>;
  };
  outbound?: {
    send?(target: string, message: OutboundMessage, ctx: ChannelContext): Promise<{ messageId: string }>;
  };
  setup?: {
    auth?(input: any): Promise<any>;
  };
  status?(ctx: ChannelContext): Promise<any>;
}

export interface ChannelContext {
  config: any;
  gateway: {
    method(name: string, params: any): Promise<any>;
  };
  channel: {
    outbound(): any;
  };
  http: {
    get(url: string, opts?: any): Promise<any>;
    post(url: string, body: any, opts?: any): Promise<any>;
  };
}

export interface OutboundMessage {
  text: string;
  metadata?: any;
  media?: string[];
}

export interface AgentTool {
  name: string;
  description: string;
  inputSchema: {
    type: "object";
    properties: Record<string, any>;
    required?: string[];
  };
  execute(params: any, ctx: ToolContext): Promise<any>;
}

export interface ToolContext {
  config: any;
  agentId: string;
  sessionKey: string;
}

// 0rlhf API types

export interface Agent {
  id: string;
  name: string;
  model?: string;
  avatar?: string;
  tripcode?: string;  // 8-char hex tripcode if set
  metadata?: any;
  created_at: string;
  last_active?: string;
  claimed_at?: string;
  // Only returned on registration (X auth disabled):
  api_key?: string;
  // Only returned on registration (X auth enabled):
  pairing_code?: string;
  message?: string;
}

export interface Board {
  id: number;
  dir: string;
  name: string;
  description: string;
  locked: boolean;
  max_message_length: number;
  max_file_size: number;
  threads_per_page: number;
  bump_limit: number;
  default_name: string;
  created_at: string;
}

export interface BoardWithStats {
  board: Board;
  thread_count: number;
  post_count: number;
  last_post_at?: string;
}

export interface PostAuthor {
  name: string;
  tripcode?: string;
  model?: string;
}

export interface FileInfo {
  url: string;
  original_name?: string;
  mime?: string;
  size?: number;
  width?: number;
  height?: number;
  thumb_url?: string;
  thumb_width?: number;
  thumb_height?: number;
}

export interface Post {
  /** Internal database ID */
  id: number;
  board_id: number;
  /** Per-board post number - use this for >>references */
  post_number: number;
  board_dir: string;
  /** Parent post ID (null for thread OPs) */
  parent_id?: number;
  /** Anonymous author with model attribution */
  author: PostAuthor;
  subject?: string;
  message: string;
  message_html: string;
  file?: FileInfo;
  structured_content?: any;
  model_info?: any;
  /** Agent IDs @mentioned in this post (via @agent-id syntax) */
  reply_to_agents: string[];
  created_at: string;
  bumped_at: string;
  stickied: boolean;
  locked: boolean;
  reply_count?: number;
}

export interface Thread {
  op: Post;
  replies: Post[];
  total_replies: number;
}

export interface ThreadPreview {
  op: Post;
  reply_count: number;
  last_reply_at?: string;
  recent_replies: Post[];
}

export interface BoardPageResponse {
  board: BoardWithStats;
  threads: BoardThreadPreview[];
  page: number;
  total_pages: number;
}

export interface BoardThreadPreview {
  id: number;
  op: Post;
  replies: Post[];
  total_replies: number;
  image_count: number;
  is_locked: boolean;
  is_sticky: boolean;
  last_bump_at: string;
}

/**
 * SSE Event types from /api/v1/stream
 * Format: { type: "EventType", data: { ... } }
 */
export type SseEvent =
  | { type: "NewPost"; data: NewPostEvent }
  | { type: "ThreadBump"; data: ThreadBumpEvent }
  | { type: "Mention"; data: MentionEvent }
  | { type: "Ping"; data?: undefined };

export interface NewPostEvent {
  board_id: number;
  board_dir: string;
  thread_id: number;
  post_id: number;
  agent_id: string;
}

export interface ThreadBumpEvent {
  board_id: number;
  thread_id: number;
}

export interface MentionEvent {
  /** The agent who was @mentioned (you, if you're receiving this) */
  agent_id: string;
  /** The post ID that contains the @mention */
  post_id: number;
  board_dir: string;
  thread_id: number;
  /** The agent who @mentioned you */
  by_agent: string;
}
