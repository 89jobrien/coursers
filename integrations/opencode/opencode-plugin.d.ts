/**
 * Compile-time compatibility subset of @opencode-ai/plugin 1.2.26.
 *
 * This intentionally declares only the PluginInput and hook surfaces consumed by
 * coursers.ts. Field names and callback shapes mirror the installed 1.2.26 plugin
 * and SDK declarations; OpenCode supplies the full runtime objects.
 */
declare module "@opencode-ai/plugin" {
  type SessionInfo = {
    id: string
    projectID: string
    directory: string
    parentID?: string
    title: string
    version: string
    time: {
      created: number
      updated: number
      compacting?: number
    }
  }

  type OpenCodeEvent =
    | { type: "session.created"; properties: { info: SessionInfo } }
    | {
        type: "session.status"
        properties: {
          sessionID: string
          status:
            | { type: "idle" }
            | { type: "busy" }
            | { type: "retry"; attempt: number; message: string; next: number }
        }
      }
    | { type: "session.idle"; properties: { sessionID: string } }
    | { type: "session.compacted"; properties: { sessionID: string } }
    | { type: "session.deleted"; properties: { info: SessionInfo } }
    | { type: "server.instance.disposed"; properties: { directory: string } }

  type Permission = {
    id: string
    type: string
    pattern?: string | string[]
    sessionID: string
    messageID: string
    callID?: string
    title: string
    metadata: Record<string, unknown>
    time: { created: number }
  }

  type Part = { text?: string; [key: string]: unknown }

  type Hooks = {
    event?: (input: { event: OpenCodeEvent }) => Promise<void>
    "tool.execute.before"?: (
      input: { tool: string; sessionID: string; callID: string },
      output: { args: Record<string, unknown> },
    ) => Promise<void>
    "tool.execute.after"?: (
      input: {
        tool: string
        sessionID: string
        callID: string
        args: Record<string, unknown>
      },
      output: {
        title: string
        output: string
        metadata: Record<string, unknown>
      },
    ) => Promise<void>
    "chat.message"?: (
      input: {
        sessionID: string
        agent?: string
        model?: { providerID: string; modelID: string }
        messageID?: string
        variant?: string
      },
      output: { message: unknown; parts: Part[] },
    ) => Promise<void>
    "permission.ask"?: (
      input: Permission,
      output: { status: "ask" | "deny" | "allow" },
    ) => Promise<void>
    "experimental.session.compacting"?: (
      input: { sessionID: string },
      output: { context: string[]; prompt?: string },
    ) => Promise<void>
  }

  export type PluginInput = {
    client: {
      app: {
        log(input: {
          body: {
            service: string
            level: "debug" | "info" | "error" | "warn"
            message: string
            extra?: Record<string, unknown>
          }
        }): Promise<unknown>
      }
    }
    project: unknown
    directory: string
    worktree: string
    serverUrl: URL
    $: unknown
  }

  export type Plugin = (input: PluginInput) => Promise<Hooks>
}

declare const Bun: {
  spawn(
    command: string[],
    options: {
      cwd: string
      stdin: Uint8Array
      stdout: "pipe"
      stderr: "pipe"
    },
  ): {
    exited: Promise<number>
    stdout: ReadableStream<Uint8Array>
    stderr: ReadableStream<Uint8Array>
  }
}
