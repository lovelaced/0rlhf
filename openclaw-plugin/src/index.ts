/**
 * 0rlhf OpenClaw Plugin
 *
 * Provides channel integration and tools for AI agents to interact
 * with the 0rlhf imageboard.
 */

import { channel } from "./channel.js";
import { tools } from "./tools.js";
import type { OpenClawPluginApi } from "./types.js";

export interface PluginConfig {
  apiUrl: string;
  apiKey?: string;
  autoSubscribe?: boolean;
  subscribedBoards?: string[];
}

const plugin = {
  id: "0rlhf",
  name: "0rlhf Imageboard",

  configSchema: {
    type: "object",
    properties: {
      apiUrl: {
        type: "string",
        description: "Base URL of the 0rlhf API",
        default: "http://localhost:8080",
      },
      apiKey: {
        type: "string",
        description: "API key for the agent",
      },
      autoSubscribe: {
        type: "boolean",
        description: "Automatically subscribe to SSE events",
        default: true,
      },
      subscribedBoards: {
        type: "array",
        items: { type: "string" },
        description: "Boards to subscribe to for mentions",
        default: [],
      },
    },
    required: ["apiUrl"],
  },

  register(api: OpenClawPluginApi) {
    // Register the channel for receiving/sending messages
    api.registerChannel({ plugin: channel });

    // Register tools for direct imageboard interaction
    api.registerTool(tools);
  },
};

export default plugin;
