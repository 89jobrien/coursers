# Design: Rewrite Before Block

## Goal

Apply deterministic command rewrites before course-correction checks so safe mechanical fixes avoid a rejected tool call without bypassing safety rules.

## Approved Approach

Use the existing rewrite configuration and hook-chain implementation, changing pre-hook composition so rewrites feed subsequent deny checks across supported harnesses.

## Crate Ownership

- **Owner crate**: `coursers-core` — hook composition and rule evaluation are core domain behavior.
- **Affected crate**: `coursers` — OpenCode and CLI adapters translate the final core outcome into harness-specific responses.
- **Affected crate**: `coursers-e2e` — cross-process coverage verifies protocol behavior.

## Public API

No new public traits, types, or functions. `HookChain::run_pre` retains its existing signature and outcome contract.

## Data Flow

1. Source: a harness supplies a Bash command in `HookContext`.
2. Transform: configured rewrite hooks update the effective command and continue evaluation.
3. Policy: rule-block hooks evaluate the rewritten command.
4. Sink: the adapter emits deny when policy rejects the effective command, otherwise emits the final rewrite when one occurred.

## Hexagonal Boundaries

- **Ports**: existing `PreHook`, `RewriteLoader`, `RulesLoader`, and `StateStore` traits.
- **Adapters**: existing filesystem-backed loaders assembled by `ProfileConfig`.

## Out of Scope

- Generating dedicated-tool calls such as Grep, Glob, or Read from shell commands.
- Adding new rewrite syntax or external dependencies.
- Changing post-hook filtering or failure-learning behavior.
- Modifying user-global rule configuration.

## Risk

- [x] Breaking API changes: no.
- [x] New external dependency: no.
- [x] Feature flag required: no new flag; the legacy opt-in chain gate remains unchanged.
- [x] Security: deny rules must evaluate the rewritten command, and a rewritten unsafe command must still be denied.
