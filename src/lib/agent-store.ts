import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

// ===================== Types =====================

export type Role = "user" | "assistant" | "system";

export type MessageContent =
  | { type: "text"; text: string }
  | { type: "tool_use"; id: string; name: string; input: any }
  | {
      type: "tool_result";
      tool_use_id: string;
      content: string;
      is_error: boolean;
    }
  | { type: "thinking"; thinking: string };

export interface Usage {
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
}

export interface AgentMessage {
  role: Role;
  content: MessageContent[];
  timestamp: number;
  usage?: Usage;
}

export interface ConversationMeta {
  id: string;
  connection_id: string;
  title: string;
  created_at: number;
  updated_at: number;
  provider: string;
  model: string;
}

export interface Conversation extends ConversationMeta {
  messages: AgentMessage[];
}

export type ApprovalMode = "manual" | "auto_read" | "yolo";

export interface AgentSettings {
  default_provider: "anthropic" | "openai";
  anthropic_model: string;
  openai_model: string;
  max_turns_per_invocation: number;
  default_approval_mode: ApprovalMode;
  reasoning_effort: "low" | "medium" | "high";
  show_thinking: boolean;
}

export interface PendingApproval {
  call_id: string;
  tool_name: string;
  args: any;
  preview: string;
}

// ===================== Event payloads =====================

interface DeltaEvent {
  conversation_id: string;
  content_delta: string;
}
interface ToolCallEvent {
  conversation_id: string;
  call_id: string;
  tool_name: string;
  args: any;
}
interface ToolApprovalEvent extends ToolCallEvent {
  preview: string;
}
interface ToolResultEvent {
  conversation_id: string;
  call_id: string;
  ok: boolean;
  output_preview: string;
}
interface TurnEndEvent {
  conversation_id: string;
  usage: Usage;
}
interface ErrorEvent {
  conversation_id: string;
  error: string;
}

// ===================== Store =====================

interface AgentStore {
  activeConversationId: string | null;
  conversation: Conversation | null;
  streaming: boolean;
  streamingText: string;
  streamingThinking: string;
  pendingApproval: PendingApproval | null;
  lastUsage: Usage | null;
  error: string | null;
  conversations: ConversationMeta[];

  // backend settings (cached; sibling F populates fully)
  settings: AgentSettings | null;

  loadSettings(): Promise<void>;
  loadConversations(connectionId: string): Promise<void>;
  openConversation(id: string): Promise<void>;
  newConversation(connectionId: string): Promise<void>;
  deleteConversation(id: string): Promise<void>;
  renameConversation(id: string, title: string): Promise<void>;
  send(content: string, approvalMode: ApprovalMode): Promise<void>;
  cancel(): Promise<void>;
  approve(callId: string): Promise<void>;
  reject(callId: string, reason: string): Promise<void>;
  clearError(): void;
  subscribeAgentEvents(): () => void;
}

// helper: append a content block to the last assistant message (or push a new one)
function appendToAssistant(
  conv: Conversation,
  block: MessageContent,
): Conversation {
  const msgs = conv.messages.slice();
  const last = msgs[msgs.length - 1];
  if (last && last.role === "assistant") {
    msgs[msgs.length - 1] = { ...last, content: [...last.content, block] };
  } else {
    msgs.push({
      role: "assistant",
      content: [block],
      timestamp: Date.now() / 1000,
    });
  }
  return { ...conv, messages: msgs };
}

export const useAgentStore = create<AgentStore>((set, get) => ({
  activeConversationId: null,
  conversation: null,
  streaming: false,
  streamingText: "",
  streamingThinking: "",
  pendingApproval: null,
  lastUsage: null,
  error: null,
  conversations: [],
  settings: null,

  async loadSettings() {
    try {
      const settings = await invoke<AgentSettings>("agent_get_settings");
      set({ settings });
    } catch (e: any) {
      console.error("agent_get_settings", e);
    }
  },

  async loadConversations(connectionId: string) {
    try {
      const conversations = await invoke<ConversationMeta[]>(
        "agent_list_conversations",
        { connectionId },
      );
      set({ conversations });
    } catch (e: any) {
      console.error("agent_list_conversations", e);
      set({ error: String(e?.message ?? e) });
    }
  },

  async openConversation(id: string) {
    try {
      const conversation = await invoke<Conversation>(
        "agent_load_conversation",
        { conversationId: id },
      );
      set({
        activeConversationId: conversation.id,
        conversation,
        streamingText: "",
        streamingThinking: "",
        pendingApproval: null,
        error: null,
      });
    } catch (e: any) {
      set({ error: String(e?.message ?? e) });
    }
  },

  async newConversation(connectionId: string) {
    const settings = get().settings;
    const provider = settings?.default_provider ?? "anthropic";
    const model =
      provider === "anthropic"
        ? settings?.anthropic_model ?? "claude-sonnet-4-5"
        : settings?.openai_model ?? "gpt-5";
    try {
      const conversation = await invoke<Conversation>("agent_new_conversation", {
        connectionId,
        provider,
        model,
      });
      set({
        activeConversationId: conversation.id,
        conversation,
        streamingText: "",
        streamingThinking: "",
        pendingApproval: null,
        error: null,
      });
      // refresh list
      get().loadConversations(connectionId).catch(() => {});
    } catch (e: any) {
      set({ error: String(e?.message ?? e) });
    }
  },

  async deleteConversation(id: string) {
    try {
      await invoke("agent_delete_conversation", { conversationId: id });
      const conv = get().conversation;
      if (conv && conv.id === id) {
        set({ conversation: null, activeConversationId: null });
      }
      set({
        conversations: get().conversations.filter((c) => c.id !== id),
      });
    } catch (e: any) {
      set({ error: String(e?.message ?? e) });
    }
  },

  async renameConversation(id: string, title: string) {
    try {
      await invoke("agent_rename_conversation", {
        conversationId: id,
        title,
      });
      const conv = get().conversation;
      if (conv && conv.id === id) {
        set({ conversation: { ...conv, title } });
      }
      set({
        conversations: get().conversations.map((c) =>
          c.id === id ? { ...c, title } : c,
        ),
      });
    } catch (e: any) {
      set({ error: String(e?.message ?? e) });
    }
  },

  async send(content: string, approvalMode: ApprovalMode) {
    const conv = get().conversation;
    if (!conv) return;
    // Optimistic user message
    const userMsg: AgentMessage = {
      role: "user",
      content: [{ type: "text", text: content }],
      timestamp: Date.now() / 1000,
    };
    set({
      conversation: { ...conv, messages: [...conv.messages, userMsg] },
      streaming: true,
      streamingText: "",
      streamingThinking: "",
      error: null,
    });
    try {
      await invoke("agent_send_message", {
        conversationId: conv.id,
        content,
        approvalMode,
      });
    } catch (e: any) {
      set({ streaming: false, error: String(e?.message ?? e) });
    }
  },

  async cancel() {
    const conv = get().conversation;
    if (!conv) return;
    try {
      await invoke("agent_cancel", { conversationId: conv.id });
    } catch (e: any) {
      console.error("agent_cancel", e);
    }
    set({ streaming: false });
  },

  async approve(callId: string) {
    try {
      await invoke("agent_approve_tool", { callId });
      const pa = get().pendingApproval;
      if (pa && pa.call_id === callId) set({ pendingApproval: null });
    } catch (e: any) {
      set({ error: String(e?.message ?? e) });
    }
  },

  async reject(callId: string, reason: string) {
    try {
      await invoke("agent_reject_tool", { callId, reason });
      const pa = get().pendingApproval;
      if (pa && pa.call_id === callId) set({ pendingApproval: null });
    } catch (e: any) {
      set({ error: String(e?.message ?? e) });
    }
  },

  clearError() {
    set({ error: null });
  },

  subscribeAgentEvents() {
    const unlisteners: UnlistenFn[] = [];
    let disposed = false;

    const matches = (cid: string) => get().activeConversationId === cid;

    const wire = async () => {
      unlisteners.push(
        await listen<DeltaEvent>("agent-message-delta", (e) => {
          const p = e.payload;
          if (!matches(p.conversation_id)) return;
          set({ streamingText: get().streamingText + p.content_delta });
        }),
      );
      unlisteners.push(
        await listen<DeltaEvent>("agent-thinking-delta", (e) => {
          const p = e.payload;
          if (!matches(p.conversation_id)) return;
          set({
            streamingThinking: get().streamingThinking + p.content_delta,
          });
        }),
      );
      unlisteners.push(
        await listen<ToolCallEvent>("agent-tool-call", (e) => {
          const p = e.payload;
          if (!matches(p.conversation_id)) return;
          let conv = get().conversation;
          if (!conv) return;
          // Flush streamingText into the assistant message first
          if (get().streamingText) {
            conv = appendToAssistant(conv, {
              type: "text",
              text: get().streamingText,
            });
          }
          conv = appendToAssistant(conv, {
            type: "tool_use",
            id: p.call_id,
            name: p.tool_name,
            input: p.args,
          });
          set({ conversation: conv, streamingText: "" });
        }),
      );
      unlisteners.push(
        await listen<ToolApprovalEvent>("agent-tool-approval", (e) => {
          const p = e.payload;
          if (!matches(p.conversation_id)) return;
          set({
            pendingApproval: {
              call_id: p.call_id,
              tool_name: p.tool_name,
              args: p.args,
              preview: p.preview,
            },
          });
        }),
      );
      unlisteners.push(
        await listen<ToolResultEvent>("agent-tool-result", (e) => {
          const p = e.payload;
          if (!matches(p.conversation_id)) return;
          let conv = get().conversation;
          if (!conv) return;
          // tool results live as a user-role message in canonical anthropic format,
          // but for display we append to assistant as a tool_result block.
          conv = appendToAssistant(conv, {
            type: "tool_result",
            tool_use_id: p.call_id,
            content: p.output_preview,
            is_error: !p.ok,
          });
          set({ conversation: conv });
        }),
      );
      unlisteners.push(
        await listen<TurnEndEvent>("agent-turn-end", (e) => {
          const p = e.payload;
          if (!matches(p.conversation_id)) return;
          let conv = get().conversation;
          if (!conv) {
            set({ streaming: false, lastUsage: p.usage });
            return;
          }
          // Flush any remaining streamingText
          if (get().streamingText) {
            conv = appendToAssistant(conv, {
              type: "text",
              text: get().streamingText,
            });
          }
          // Attach usage to the last assistant message
          const msgs = conv.messages.slice();
          for (let i = msgs.length - 1; i >= 0; i--) {
            if (msgs[i].role === "assistant") {
              msgs[i] = { ...msgs[i], usage: p.usage };
              break;
            }
          }
          set({
            conversation: { ...conv, messages: msgs },
            streaming: false,
            streamingText: "",
            streamingThinking: "",
            lastUsage: p.usage,
          });
        }),
      );
      unlisteners.push(
        await listen<ErrorEvent>("agent-error", (e) => {
          const p = e.payload;
          if (!matches(p.conversation_id)) return;
          set({ streaming: false, error: p.error });
        }),
      );
    };

    wire().catch((err) => console.error("agent subscribe", err));

    return () => {
      if (disposed) return;
      disposed = true;
      for (const u of unlisteners) {
        try {
          u();
        } catch {
          /* ignore */
        }
      }
    };
  },
}));
