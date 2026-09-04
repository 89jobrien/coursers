/// <reference path="./opencode-plugin.d.ts" />

import type { Plugin, PluginInput } from "@opencode-ai/plugin"

export type BridgeRequest = {
  tool_name?: string
  tool_input?: Record<string, unknown>
  tool_response?: { exit_code: number; output: string }
  session_id?: string
  target?: string
  [key: string]: unknown
}

export type BridgeResponse = {
  decision: "allow" | "deny"
  reason: string | null
  updated_input: Record<string, unknown> | null
  replacement_output: string | null
  messages: string[]
  matched_rules: string[]
}

export type Bridge = (
  event: string,
  payload: BridgeRequest,
) => Promise<BridgeResponse>

type SessionState = {
  parents: Map<string, string | undefined>
  ended: Set<string>
  stopped: Set<string>
}

type Log = (level: "info" | "error", message: string) => Promise<void>

const allowResponse = (): BridgeResponse => ({
  decision: "allow",
  reason: null,
  updated_input: null,
  replacement_output: null,
  messages: [],
  matched_rules: [],
})

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string")
}

function isBridgeResponse(value: unknown): value is BridgeResponse {
  if (!isRecord(value)) return false
  return (
    (value.decision === "allow" || value.decision === "deny") &&
    (value.reason === null || typeof value.reason === "string") &&
    (value.updated_input === null || isRecord(value.updated_input)) &&
    (value.replacement_output === null ||
      typeof value.replacement_output === "string") &&
    isStringArray(value.messages) &&
    isStringArray(value.matched_rules)
  )
}

function defaultBridge(directory: string, log: Log): Bridge {
  return async (event, payload) => {
    try {
      const process = Bun.spawn(
        ["crs", "hook", "--target", "opencode", event],
        {
          cwd: directory,
          stdin: new TextEncoder().encode(JSON.stringify(payload)),
          stdout: "pipe",
          stderr: "pipe",
        },
      )
      const [exitCode, stdout, stderr] = await Promise.all([
        process.exited,
        new Response(process.stdout).text(),
        new Response(process.stderr).text(),
      ])
      let parsed: unknown
      try {
        parsed = JSON.parse(stdout)
      } catch {
        parsed = undefined
      }
      if (exitCode !== 0) {
        await log(
          "error",
          `OpenCode bridge exit ${exitCode} for ${event}: ${stderr.trim()}`,
        )
      }
      if (isBridgeResponse(parsed)) return parsed
      if (exitCode === 0) {
        await log("error", `OpenCode bridge returned an invalid response for ${event}`)
      }
      return allowResponse()
    } catch (error) {
      await log("error", `OpenCode bridge failed for ${event}: ${String(error)}`)
      return allowResponse()
    }
  }
}

function normalizeToolName(tool: string): string {
  const names: Record<string, string> = {
    bash: "Bash",
    edit: "Edit",
    write: "Write",
  }
  return names[tool.toLowerCase()] ?? tool
}

function normalizeToolInput(input: Record<string, unknown>): Record<string, unknown> {
  const normalized = { ...input }
  if ("filePath" in normalized) {
    normalized.file_path = normalized.filePath
    delete normalized.filePath
  }
  return normalized
}

function promptTarget(output: { parts: unknown[] }): string {
  return output.parts
    .map((part) => {
      if (part && typeof part === "object" && "text" in part) {
        const text = (part as { text?: unknown }).text
        return typeof text === "string" ? text : ""
      }
      return ""
    })
    .filter(Boolean)
    .join("\n")
}

export function createCoursersPlugin(injectedBridge?: Bridge): Plugin {
  return async ({ client, directory }: PluginInput) => {
    const log: Log = async (level, message) => {
      try {
        await client.app.log({
          body: { service: "coursers", level, message },
        })
      } catch {
        // Application logging is advisory and must never reject a hook.
      }
    }
    const bridge = injectedBridge ?? defaultBridge(directory, log)
    const callBridge = async (event: string, payload: BridgeRequest) => {
      try {
        const response: unknown = await bridge(event, payload)
        if (!isBridgeResponse(response)) {
          await log("error", `OpenCode bridge returned an invalid response for ${event}`)
          return allowResponse()
        }
        for (const message of response.messages) await log("info", message)
        return response
      } catch (error) {
        await log("error", `OpenCode bridge failed for ${event}: ${String(error)}`)
        return allowResponse()
      }
    }
    const sessions: SessionState = {
      parents: new Map(),
      ended: new Set(),
      stopped: new Set(),
    }
    const deliverSessionEnd = async (
      sessionID: string,
      properties: Record<string, unknown>,
    ) => {
      if (sessions.ended.has(sessionID)) return
      sessions.ended.add(sessionID)
      await callBridge("session-end", {
        ...properties,
        session_id: sessionID,
        target: sessionID,
      })
    }

    return {
      event: async ({ event }) => {
        switch (event.type) {
          case "session.created": {
            const { info } = event.properties
            if (sessions.parents.has(info.id)) return
            sessions.parents.set(info.id, info.parentID)
            sessions.stopped.delete(info.id)
            await callBridge(info.parentID ? "subagent-start" : "session-start", {
              ...event.properties,
              session_id: info.id,
              target: info.directory,
            })
            break
          }
          case "session.idle": {
            const { sessionID } = event.properties
            if (!sessions.parents.has(sessionID) || sessions.stopped.has(sessionID)) return
            sessions.stopped.add(sessionID)
            await callBridge(
              sessions.parents.get(sessionID) ? "subagent-stop" : "stop",
              {
                ...event.properties,
                session_id: sessionID,
                target: sessionID,
              },
            )
            break
          }
          case "session.status":
            if (
              event.properties.status.type === "busy" &&
              sessions.parents.has(event.properties.sessionID)
            ) {
              sessions.stopped.delete(event.properties.sessionID)
            }
            break
          case "session.compacted":
            await callBridge("post-compact", {
              ...event.properties,
              session_id: event.properties.sessionID,
              target: event.properties.sessionID,
            })
            break
          case "session.deleted": {
            const { info } = event.properties
            const parentID = sessions.parents.get(info.id) ?? info.parentID
            if (!parentID) await deliverSessionEnd(info.id, event.properties)
            sessions.parents.delete(info.id)
            sessions.stopped.delete(info.id)
            break
          }
          case "server.instance.disposed": {
            const roots = [...sessions.parents.entries()]
              .filter(([, parentID]) => !parentID)
              .map(([sessionID]) => sessionID)
            await Promise.all(
              roots.map((sessionID) => deliverSessionEnd(sessionID, event.properties)),
            )
            break
          }
        }
      },
      "tool.execute.before": async (input, output) => {
        const toolInput = normalizeToolInput(output.args)
        const response = await callBridge("pre-tool-use", {
          tool_name: normalizeToolName(input.tool),
          tool_input: toolInput,
          session_id: input.sessionID,
        })
        if (response.decision === "deny") {
          throw new Error(response.reason ?? "Denied by Coursers policy")
        }
        if (response.updated_input) Object.assign(output.args, response.updated_input)
      },
      "tool.execute.after": async (input, output) => {
        const exit = output.metadata?.exit
        const response = await callBridge("post-tool-use", {
          tool_name: normalizeToolName(input.tool),
          tool_input: normalizeToolInput(input.args),
          tool_response: {
            exit_code: typeof exit === "number" ? exit : 0,
            output: output.output,
          },
          session_id: input.sessionID,
        })
        if (response.replacement_output !== null) {
          output.output = response.replacement_output
        }
      },
      "chat.message": async (input, output) => {
        const response = await callBridge("user-prompt-submit", {
          session_id: input.sessionID,
          target: promptTarget(output),
        })
        if (response.decision === "deny") {
          throw new Error(response.reason ?? "Denied by Coursers policy")
        }
      },
      "permission.ask": async (input, output) => {
        const response = await callBridge("permission-request", {
          session_id: input.sessionID,
          target: input.type,
        })
        if (response.decision === "deny") output.status = "deny"
      },
      "experimental.session.compacting": async (input) => {
        await callBridge("pre-compact", { session_id: input.sessionID })
      },
    }
  }
}

export const CoursersPlugin: Plugin = createCoursersPlugin()
