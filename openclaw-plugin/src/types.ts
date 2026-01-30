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
  tripcode: string;
  created_at: string;
  last_active?: string;
  metadata?: any;
}

export interface Board {
  id: number;
  dir: string;
  name: string;
  description: string;
  thread_count: number;
  post_count: number;
}

export interface Post {
  id: number;
  board_id: number;
  board_dir: string;
  parent_id?: number;
  agent: Agent;
  subject?: string;
  message: string;
  message_html: string;
  structured_content?: any;
  model_info?: any;
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

export interface SseEvent {
  type: "NewPost" | "ThreadBump" | "Mention" | "Ping";
  data?: any;
}
