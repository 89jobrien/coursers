import { describe, expect, test } from "bun:test"
import {
  createCoursersPlugin,
  type Bridge,
  type BridgeRequest,
  type BridgeResponse,
} from "./coursers"

const allow = (overrides: Partial<BridgeResponse> = {}): BridgeResponse => ({
  decision: "allow",
  reason: null,
  updated_input: null,
  replacement_output: null,
  messages: [],
  matched_rules: [],
  ...overrides,
})

async function harness(
  responses: unknown[],
  log: (input: unknown) => Promise<unknown> = async () => ({}),
) {
  const calls: Array<{ event: string; payload: BridgeRequest }> = []
  const bridge: Bridge = async (event, payload) => {
    calls.push({ event, payload })
    return (responses.shift() ?? allow()) as BridgeResponse
  }
  const plugin = createCoursersPlugin(bridge)
  const hooks = await plugin({
    directory: "/workspace",
    client: { app: { log } },
  } as never)
  return { calls, hooks }
}

describe("OpenCode tool bridge", () => {
  test("normalizes bash input and applies rewritten command", async () => {
    const { calls, hooks } = await harness([
      allow({ updated_input: { command: "eza" } }),
    ])
    const output = { args: { command: "ls" } }

    await hooks["tool.execute.before"]?.(
      { tool: "bash", sessionID: "ses-root", callID: "call-1" },
      output,
    )

    expect(calls).toEqual([
      {
        event: "pre-tool-use",
        payload: {
          tool_name: "Bash",
          tool_input: { command: "ls" },
          session_id: "ses-root",
        },
      },
    ])
    expect(output.args.command).toBe("eza")
  })

  test("throws the policy reason when a tool is denied", async () => {
    const { hooks } = await harness([
      allow({ decision: "deny", reason: "blocked command" }),
    ])

    expect(
      hooks["tool.execute.before"]?.(
        { tool: "bash", sessionID: "ses-root", callID: "call-1" },
        { args: { command: "rm -rf build" } },
      ),
    ).rejects.toThrow("blocked command")
  })

  test("uses metadata exit and replaces post-tool output", async () => {
    const { calls, hooks } = await harness([
      allow({ replacement_output: "filtered" }),
    ])
    const output = { title: "shell", output: "verbose", metadata: { exit: 7 } }

    await hooks["tool.execute.after"]?.(
      {
        tool: "bash",
        sessionID: "ses-root",
        callID: "call-1",
        args: { command: "cargo test" },
      },
      output,
    )

    expect(calls[0]).toEqual({
      event: "post-tool-use",
      payload: {
        tool_name: "Bash",
        tool_input: { command: "cargo test" },
        tool_response: { exit_code: 7, output: "verbose" },
        session_id: "ses-root",
      },
    })
    expect(output.output).toBe("filtered")
  })

  test("sets permission status to deny", async () => {
    const { calls, hooks } = await harness([
      allow({ decision: "deny", reason: "permission denied" }),
    ])
    const output: { status: "ask" | "deny" | "allow" } = { status: "ask" }

    await hooks["permission.ask"]?.(
      { id: "perm-1", sessionID: "ses-root", type: "bash" } as never,
      output,
    )

    expect(calls[0]?.event).toBe("permission-request")
    expect(calls[0]?.payload.session_id).toBe("ses-root")
    expect(output.status).toBe("deny")
  })
})

describe("OpenCode lifecycle bridge", () => {
  const session = (id: string, parentID?: string) => ({
    id,
    parentID,
    directory: "/workspace",
    projectID: "project",
    title: id,
    version: "1",
    time: { created: 1, updated: 1 },
  })

  test("maps root creation and idle to session start and stop", async () => {
    const { calls, hooks } = await harness([])
    await hooks.event?.({
      event: { type: "session.created", properties: { info: session("root") } },
    } as never)
    await hooks.event?.({
      event: { type: "session.idle", properties: { sessionID: "root" } },
    } as never)

    expect(calls.map((call) => call.event)).toEqual(["session-start", "stop"])
  })

  test("maps child creation and idle to subagent start and stop", async () => {
    const { calls, hooks } = await harness([])
    await hooks.event?.({
      event: {
        type: "session.created",
        properties: { info: session("child", "root") },
      },
    } as never)
    await hooks.event?.({
      event: { type: "session.idle", properties: { sessionID: "child" } },
    } as never)

    expect(calls.map((call) => call.event)).toEqual([
      "subagent-start",
      "subagent-stop",
    ])
  })

  test("maps compaction hooks before and after compaction", async () => {
    const { calls, hooks } = await harness([])
    await hooks["experimental.session.compacting"]?.(
      { sessionID: "root" },
      { context: [] },
    )
    await hooks.event?.({
      event: { type: "session.compacted", properties: { sessionID: "root" } },
    } as never)

    expect(calls.map((call) => call.event)).toEqual([
      "pre-compact",
      "post-compact",
    ])
  })

  test("delivers session end once across deletion and disposal", async () => {
    const { calls, hooks } = await harness([])
    await hooks.event?.({
      event: { type: "session.created", properties: { info: session("root-a") } },
    } as never)
    await hooks.event?.({
      event: { type: "session.created", properties: { info: session("root-b") } },
    } as never)
    await hooks.event?.({
      event: { type: "session.deleted", properties: { info: session("root-a") } },
    } as never)
    await hooks.event?.({
      event: {
        type: "server.instance.disposed",
        properties: { directory: "/workspace" },
      },
    } as never)

    expect(
      calls.filter((call) => call.event === "session-end").map((call) => call.payload.session_id),
    ).toEqual(["root-a", "root-b"])
  })
})

describe("OpenCode review safeguards", () => {
  test("swallows application logging failures", async () => {
    const { hooks } = await harness(
      [allow({ messages: ["notice"] })],
      async () => {
        throw new Error("logger unavailable")
      },
    )

    await expect(
      hooks["tool.execute.before"]?.(
        { tool: "bash", sessionID: "root", callID: "call" },
        { args: { command: "ls" } },
      ),
    ).resolves.toBeUndefined()
  })

  test("fails open on a malformed bridge response", async () => {
    const logs: unknown[] = []
    const { hooks } = await harness(
      [
        {
          decision: "allow",
          reason: null,
          updated_input: { command: "unsafe rewrite" },
          replacement_output: null,
          messages: [7],
          matched_rules: [],
        },
      ],
      async (entry) => {
        logs.push(entry)
      },
    )
    const output = { args: { command: "ls" } }

    await hooks["tool.execute.before"]?.(
      { tool: "bash", sessionID: "root", callID: "call" },
      output,
    )

    expect(output.args.command).toBe("ls")
    expect(logs).toHaveLength(1)
  })

  test("throws the policy reason when a prompt is denied", async () => {
    const { hooks } = await harness([
      allow({ decision: "deny", reason: "prompt blocked" }),
    ])

    expect(
      hooks["chat.message"]?.(
        { sessionID: "root" },
        { message: {} as never, parts: [{ text: "secret prompt" }] as never },
      ),
    ).rejects.toThrow("prompt blocked")
  })

  test("defaults missing exit metadata to zero", async () => {
    const { calls, hooks } = await harness([allow()])

    await hooks["tool.execute.after"]?.(
      {
        tool: "bash",
        sessionID: "root",
        callID: "call",
        args: { command: "cargo test" },
      },
      { title: "shell", output: "ok", metadata: {} },
    )

    expect(calls[0]?.payload.tool_response?.exit_code).toBe(0)
  })

  test("uses only minimal permission identity as the target", async () => {
    const { calls, hooks } = await harness([allow()])
    const output: { status: "ask" | "deny" | "allow" } = { status: "ask" }

    await hooks["permission.ask"]?.(
      {
        id: "perm-1",
        type: "bash",
        pattern: "private command",
        sessionID: "root",
        messageID: "message",
        title: "sensitive title",
        metadata: { token: "secret" },
        time: { created: 1 },
      },
      output,
    )

    expect(calls[0]?.payload.target).toBe("bash")
    expect(JSON.stringify(calls[0]?.payload)).not.toContain("secret")
    expect(JSON.stringify(calls[0]?.payload)).not.toContain("private command")
  })

  test("deduplicates repeated lifecycle events", async () => {
    const { calls, hooks } = await harness([])
    const created = {
      event: {
        type: "session.created",
        properties: {
          info: {
            id: "root",
            directory: "/workspace",
            projectID: "project",
            title: "root",
            version: "1",
            time: { created: 1, updated: 1 },
          },
        },
      },
    } as never
    const idle = {
      event: { type: "session.idle", properties: { sessionID: "root" } },
    } as never

    await hooks.event?.(created)
    await hooks.event?.(created)
    await hooks.event?.(idle)
    await hooks.event?.(idle)

    expect(calls.map((call) => call.event)).toEqual(["session-start", "stop"])
  })
})

test("malformed bridge response stays fail-open when error logging fails", async () => {
  const { hooks } = await harness([{}], async () => {
    throw new Error("logger unavailable")
  })
  const output = { args: { command: "ls" } }

  await expect(
    hooks["tool.execute.before"]?.(
      { tool: "bash", sessionID: "root", callID: "call" },
      output,
    ),
  ).resolves.toBeUndefined()
  expect(output.args.command).toBe("ls")
})

describe("production default bridge", () => {
  test("passes encoded request bytes and parses a valid response", async () => {
    const originalSpawn = Bun.spawn
    let stdin: unknown
    try {
      Bun.spawn = ((command: string[], options: { stdin: unknown }) => {
        stdin = options.stdin
        expect(command).toEqual(["crs", "hook", "--target", "opencode", "pre-tool-use"])
        return {
          exited: Promise.resolve(0),
          stdout: new Response(
            JSON.stringify(allow({ updated_input: { command: "eza" } })),
          ).body!,
          stderr: new Response("").body!,
        }
      }) as typeof Bun.spawn
      const hooks = await createCoursersPlugin()({
        directory: "/workspace",
        client: { app: { log: async () => ({}) } },
      } as never)
      const output = { args: { command: "ls" } }

      await hooks["tool.execute.before"]?.(
        { tool: "bash", sessionID: "root", callID: "call" },
        output,
      )

      expect(stdin).toBeInstanceOf(Uint8Array)
      expect(JSON.parse(new TextDecoder().decode(stdin as Uint8Array))).toEqual({
        tool_name: "Bash",
        tool_input: { command: "ls" },
        session_id: "root",
      })
      expect(output.args.command).toBe("eza")
    } finally {
      Bun.spawn = originalSpawn
    }
  })

  test("preserves a valid deny response from a nonzero process", async () => {
    const originalSpawn = Bun.spawn
    const logs: unknown[] = []
    try {
      Bun.spawn = (() => ({
        exited: Promise.resolve(2),
        stdout: new Response(
          JSON.stringify(allow({ decision: "deny", reason: "blocked" })),
        ).body!,
        stderr: new Response("backend exited").body!,
      })) as typeof Bun.spawn
      const hooks = await createCoursersPlugin()({
        directory: "/workspace",
        client: {
          app: {
            log: async (entry: unknown) => {
              logs.push(entry)
              return {}
            },
          },
        },
      } as never)

      await expect(
        hooks["tool.execute.before"]?.(
          { tool: "bash", sessionID: "root", callID: "call" },
          { args: { command: "rm -rf build" } },
        ),
      ).rejects.toThrow("blocked")
      expect(JSON.stringify(logs)).toContain("exit 2")
    } finally {
      Bun.spawn = originalSpawn
    }
  })
})

test("rearms stop delivery only after explicit busy status", async () => {
  const { calls, hooks } = await harness([])
  const session = {
    id: "root",
    directory: "/workspace",
    projectID: "project",
    title: "root",
    version: "1",
    time: { created: 1, updated: 1 },
  }
  const idle = {
    event: { type: "session.idle", properties: { sessionID: "root" } },
  } as never

  await hooks.event?.({
    event: { type: "session.created", properties: { info: session } },
  } as never)
  await hooks.event?.(idle)
  await hooks.event?.({
    event: {
      type: "session.status",
      properties: { sessionID: "root", status: { type: "idle" } },
    },
  } as never)
  await hooks.event?.(idle)
  await hooks.event?.({
    event: {
      type: "session.status",
      properties: { sessionID: "root", status: { type: "busy" } },
    },
  } as never)
  await hooks.event?.(idle)
  await hooks.event?.(idle)

  expect(calls.map((call) => call.event)).toEqual([
    "session-start",
    "stop",
    "stop",
  ])
})
