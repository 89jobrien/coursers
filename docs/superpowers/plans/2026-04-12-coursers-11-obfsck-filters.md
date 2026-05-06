---
status: done
---

# coursers-11: obfsck Filter Generation + Redaction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Gate `crs discover` filter generation behind `--generate-filters`, merge with existing
`.ctx/obfsck-filters.yaml`, and apply generated redaction patterns in `crs filter`.

**Architecture:** Three changes: (1) add `--generate-filters` CLI flag to gate generation, (2)
add merge logic so existing user edits survive reruns, (3) add `apply_redaction()` in
`crs_core::filters` + call it in `cmd_filter` after existing filter rules run.

**Tech Stack:** Rust, clap, serde_json, regex, existing `ObfsckMcp` port + `ProcessObfsckMcpClient`
adapter.

---

## File Map

| File                         | Change                                                                                               |
| ---------------------------- | ---------------------------------------------------------------------------------------------------- |
| `crates/crs/src/main.rs`     | Add `generate_filters: bool` to `Discover` variant; gate call; merge logic in `write_obfsck_filters` |
| `crates/core/src/filters.rs` | Add `ObfsckFilters`, `RedactRule`, `load_obfsck_filters()`, `apply_redaction()`                      |
| `crates/core/src/lib.rs`     | No change needed (filters already pub)                                                               |

---

## Task 1: Add `--generate-filters` flag to `crs discover`

**Files:**

- Modify: `crates/crs/src/main.rs:14-64`

- [ ] **Step 1: Write the failing test**

  In `crates/crs/src/main.rs`, add a test module at the bottom verifying the flag parses:

  ```rust
  #[cfg(test)]
  mod cli_tests {
      use super::*;
      use clap::Parser;

      #[test]
      fn discover_default_no_generate_filters() {
          let cli = Cli::try_parse_from(["crs", "discover"]).unwrap();
          match cli.command {
              Command::Discover { generate_filters, .. } => {
                  assert!(!generate_filters);
              }
              _ => panic!("expected Discover"),
          }
      }

      #[test]
      fn discover_generate_filters_flag() {
          let cli = Cli::try_parse_from(["crs", "discover", "--generate-filters"]).unwrap();
          match cli.command {
              Command::Discover { generate_filters, .. } => {
                  assert!(generate_filters);
              }
              _ => panic!("expected Discover"),
          }
      }
  }
  ```

- [ ] **Step 2: Run test to verify it fails**

  ```bash
  cargo test -p crs cli_tests 2>&1 | tail -20
  ```

  Expected: compile error — `generate_filters` field does not exist in `Discover` variant.

- [ ] **Step 3: Add `generate_filters` field to `Discover` variant**

  In `crates/crs/src/main.rs`, update the `Discover` variant (lines ~21-34):

  ```rust
  /// Discover missed savings from Claude Code session history
  Discover {
      /// Scan all projects (default: current project only)
      #[arg(short, long)]
      all: bool,
      /// Max rows per section
      #[arg(short, long, default_value = "15")]
      limit: usize,
      /// Scan sessions from last N days
      #[arg(short, long, default_value = "30")]
      since: u32,
      /// Output format: text or json
      #[arg(short, long, default_value = "text")]
      format: String,
      /// Generate .ctx/obfsck-filters.yaml from unhandled command examples
      #[arg(long)]
      generate_filters: bool,
  },
  ```

- [ ] **Step 4: Thread `generate_filters` through `main()` and `cmd_discover()`**

  Update the `Command::Discover` arm in `main()`:

  ```rust
  Command::Discover { all, limit, since, format, generate_filters } => {
      cmd_discover(all, limit, since, &format, generate_filters);
  }
  ```

  Update `cmd_discover` signature:

  ```rust
  fn cmd_discover(all: bool, limit: usize, since: u32, format: &str, generate_filters: bool) {
  ```

  Gate the filter generation block with the new flag (locate the existing block around line 474):

  ```rust
  if generate_filters {
      if let Some(client) = crs_lib::obfsck::detect() {
          let examples: Vec<String> = report
              .unhandled
              .iter()
              .map(|f| f.example.clone())
              .collect();
          if !examples.is_empty() {
              let suggestions = client.generate_filters(&examples);
              if !suggestions.is_empty() {
                  write_obfsck_filters(&suggestions, ctx.join("obfsck-filters.yaml"));
              }
          }
      }
  }
  ```

- [ ] **Step 5: Run tests to verify they pass**

  ```bash
  cargo test -p crs cli_tests 2>&1 | tail -10
  ```

  Expected: `2 passed`.

- [ ] **Step 6: Commit**

  ```bash
  git add crates/crs/src/main.rs
  git commit -m "feat(crs): gate obfsck filter generation behind --generate-filters flag"
  ```

---

## Task 2: Merge with existing `.ctx/obfsck-filters.yaml` instead of overwriting

**Files:**

- Modify: `crates/crs/src/main.rs` — `write_obfsck_filters` function (~line 562)

- [ ] **Step 1: Write the failing test**

  Add to `cli_tests` module in `crates/crs/src/main.rs`:

  ```rust
  #[test]
  fn write_obfsck_filters_merges_existing() {
      use std::io::Write as _;
      use tempfile::TempDir;

      let dir = TempDir::new().unwrap();
      let path = dir.path().join("obfsck-filters.yaml");

      // Write an existing file with one pattern
      let existing = "# Generated by crs discover\nfilters:\n  - label: existing\n    pattern: \"existing-pat\"\n";
      std::fs::write(&path, existing).unwrap();

      let new_suggestions = vec![
          crs_core::obfsck::FilterSuggestion {
              label: "new-label".to_string(),
              pattern: "new-pat".to_string(),
          },
          // duplicate of existing — should not double-add
          crs_core::obfsck::FilterSuggestion {
              label: "existing".to_string(),
              pattern: "existing-pat".to_string(),
          },
      ];

      write_obfsck_filters(&new_suggestions, path.clone());

      let content = std::fs::read_to_string(&path).unwrap();
      // existing pattern retained
      assert!(content.contains("existing-pat"), "existing pattern must be retained");
      // new pattern added
      assert!(content.contains("new-pat"), "new pattern must be added");
      // no duplicate label
      assert_eq!(content.matches("existing").count(), 1,
          "duplicate label must not be written twice");
  }
  ```

  Add `tempfile` to dev-dependencies in `crates/crs/Cargo.toml`:

  ```toml
  [dev-dependencies]
  tempfile = "3"
  ```

- [ ] **Step 2: Run test to verify it fails**

  ```bash
  cargo test -p crs write_obfsck_filters_merges 2>&1 | tail -15
  ```

  Expected: FAIL — existing file is overwritten, not merged.

- [ ] **Step 3: Rewrite `write_obfsck_filters` with merge logic**

  Replace the existing `write_obfsck_filters` function in `crates/crs/src/main.rs`:

  ```rust
  /// Write filter suggestions from obfsck-mcp to the given YAML path.
  /// Merges with any existing file — new patterns whose label already appears are skipped.
  fn write_obfsck_filters(
      suggestions: &[crs_core::obfsck::FilterSuggestion],
      path: std::path::PathBuf,
  ) {
      use std::io::Write as _;

      // Load existing labels to avoid duplicates.
      let existing_content = std::fs::read_to_string(&path).unwrap_or_default();
      let existing_labels: std::collections::HashSet<String> = existing_content
          .lines()
          .filter_map(|l| {
              let l = l.trim();
              l.strip_prefix("- label: ").map(|s| s.trim().to_string())
          })
          .collect();

      let new_suggestions: Vec<&crs_core::obfsck::FilterSuggestion> = suggestions
          .iter()
          .filter(|s| !existing_labels.contains(&s.label))
          .collect();

      if new_suggestions.is_empty() && !existing_content.is_empty() {
          // Nothing to add; leave file untouched.
          return;
      }

      let today = chrono::Local::now().format("%Y-%m-%d").to_string();

      // Build merged output: preserve existing lines, append new entries.
      let mut out = if existing_content.trim().is_empty() {
          format!(
              "# Generated by crs discover on {today}\n\
               # Review before committing — patterns are regex-based.\n\
               filters:\n"
          )
      } else {
          // Strip trailing newline; we'll re-add it cleanly.
          existing_content.trim_end().to_string() + "\n"
      };

      for s in &new_suggestions {
          out.push_str(&format!("  - label: {}\n", s.label));
          out.push_str(&format!("    pattern: {:?}\n", s.pattern));
      }

      match std::fs::File::create(&path).and_then(|mut f| f.write_all(out.as_bytes())) {
          Ok(()) => eprintln!("wrote {}", path.display()),
          Err(e) => eprintln!("warn: could not write {}: {e}", path.display()),
      }
  }
  ```

- [ ] **Step 4: Run tests**

  ```bash
  cargo test -p crs 2>&1 | tail -15
  ```

  Expected: all tests pass including the new merge test.

- [ ] **Step 5: Commit**

  ```bash
  git add crates/crs/src/main.rs crates/crs/Cargo.toml
  git commit -m "feat(crs): merge obfsck-filters.yaml instead of overwriting on regeneration"
  ```

---

## Task 3: Add `ObfsckFilters` + `apply_redaction()` to `crs_core::filters`

**Files:**

- Modify: `crates/core/src/filters.rs`

- [ ] **Step 1: Write failing tests**

  Append to the bottom of `crates/core/src/filters.rs`:

  ```rust
  #[cfg(test)]
  mod redaction_tests {
      use super::*;

      fn make_obfsck_filters(patterns: &[(&str, &str)]) -> ObfsckFilters {
          ObfsckFilters {
              filters: patterns
                  .iter()
                  .map(|(label, pattern)| RedactRule {
                      label: label.to_string(),
                      pattern: pattern.to_string(),
                  })
                  .collect(),
          }
      }

      #[test]
      fn apply_redaction_replaces_matching_line() {
          let filters = make_obfsck_filters(&[("api-key", r"sk-[A-Za-z0-9]{10,}")]);
          let output = "some output\nsk-abc1234567890 is a secret\nclean line";
          let result = apply_redaction(output, &filters);
          assert!(result.contains("[REDACTED]"), "matching line must be redacted");
          assert!(result.contains("some output"), "non-matching lines must be preserved");
          assert!(result.contains("clean line"), "non-matching lines must be preserved");
          assert!(!result.contains("sk-abc1234567890"), "secret must not appear in output");
      }

      #[test]
      fn apply_redaction_empty_filters_passthrough() {
          let filters = make_obfsck_filters(&[]);
          let output = "sk-abc1234567890 is a secret";
          let result = apply_redaction(output, &filters);
          assert_eq!(result, output);
      }

      #[test]
      fn apply_redaction_no_match_passthrough() {
          let filters = make_obfsck_filters(&[("api-key", r"sk-[A-Za-z0-9]{10,}")]);
          let output = "no secrets here";
          let result = apply_redaction(output, &filters);
          assert_eq!(result, output);
      }

      #[test]
      fn load_obfsck_filters_missing_file_returns_empty() {
          let filters = ObfsckFilters::load_from(std::path::Path::new("/nonexistent/path.yaml"));
          assert!(filters.filters.is_empty());
      }

      #[test]
      fn load_obfsck_filters_parses_yaml() {
          use std::io::Write as _;
          let mut f = tempfile::NamedTempFile::new().unwrap();
          write!(f, "filters:\n  - label: test\n    pattern: \"secret-[0-9]+\"\n").unwrap();
          let filters = ObfsckFilters::load_from(f.path());
          assert_eq!(filters.filters.len(), 1);
          assert_eq!(filters.filters[0].label, "test");
          assert_eq!(filters.filters[0].pattern, "secret-[0-9]+");
      }
  }
  ```

  Add `tempfile` to dev-dependencies in `crates/core/Cargo.toml`:

  ```toml
  [dev-dependencies]
  tempfile = "3"
  ```

- [ ] **Step 2: Run tests to verify they fail**

  ```bash
  cargo test -p crs-core redaction_tests 2>&1 | tail -15
  ```

  Expected: compile error — `ObfsckFilters`, `RedactRule`, `apply_redaction` not defined.

- [ ] **Step 3: Add structs and functions to `crates/core/src/filters.rs`**

  Append after the existing `load()` function:

  ```rust
  // ---------------------------------------------------------------------------
  // Obfsck redaction filters
  // ---------------------------------------------------------------------------

  /// A single redaction rule: lines matching `pattern` are replaced with `[REDACTED]`.
  #[derive(Debug, Clone, serde::Deserialize)]
  pub struct RedactRule {
      pub label: String,
      pub pattern: String,
  }

  /// Root of `.ctx/obfsck-filters.yaml`.
  #[derive(Debug, Clone, serde::Deserialize, Default)]
  pub struct ObfsckFilters {
      #[serde(default)]
      pub filters: Vec<RedactRule>,
  }

  impl ObfsckFilters {
      /// Load from a specific path. Returns empty on missing or malformed file.
      pub fn load_from(path: &std::path::Path) -> Self {
          let Ok(content) = std::fs::read_to_string(path) else {
              return Self::default();
          };
          serde_yaml::from_str(&content).unwrap_or_default()
      }
  }

  /// Load `.ctx/obfsck-filters.yaml` if it exists, otherwise return empty.
  pub fn load_obfsck_filters() -> ObfsckFilters {
      let path = std::path::Path::new(".ctx/obfsck-filters.yaml");
      ObfsckFilters::load_from(path)
  }

  /// Apply redaction rules to `output`. Lines matching any pattern are replaced
  /// with `[REDACTED]` (the match is replaced inline, preserving rest of line is
  /// intentionally not done — the whole line is replaced for safety).
  pub fn apply_redaction(output: &str, filters: &ObfsckFilters) -> String {
      if filters.filters.is_empty() {
          return output.to_string();
      }

      // Compile patterns once; skip invalid regex.
      let compiled: Vec<(regex::Regex, &str)> = filters
          .filters
          .iter()
          .filter_map(|r| {
              regex::Regex::new(&r.pattern)
                  .ok()
                  .map(|re| (re, r.label.as_str()))
          })
          .collect();

      if compiled.is_empty() {
          return output.to_string();
      }

      output
          .lines()
          .map(|line| {
              let matches = compiled.iter().any(|(re, _)| re.is_match(line));
              if matches { "[REDACTED]" } else { line }
          })
          .collect::<Vec<_>>()
          .join("\n")
  }
  ```

  Check `serde_yaml` is already a dependency in `crates/core/Cargo.toml`:

  ```bash
  grep serde_yaml crates/core/Cargo.toml
  ```

  If absent, add:

  ```toml
  serde_yaml = "0.9"
  ```

- [ ] **Step 4: Run tests**

  ```bash
  cargo test -p crs-core redaction_tests 2>&1 | tail -15
  ```

  Expected: all 5 tests pass.

- [ ] **Step 5: Commit**

  ```bash
  git add crates/core/src/filters.rs crates/core/Cargo.toml
  git commit -m "feat(crs-core): add ObfsckFilters, RedactRule, apply_redaction for output sanitization"
  ```

---

## Task 4: Call `apply_redaction()` in `cmd_filter`

**Files:**

- Modify: `crates/crs/src/main.rs` — `cmd_filter` function (~line 73)

- [ ] **Step 1: Write failing test**

  Add to `cli_tests` in `crates/crs/src/main.rs`:

  ```rust
  #[test]
  fn filter_redacts_output_matching_obfsck_patterns() {
      use crs_core::filters::{ObfsckFilters, RedactRule, apply_redaction};

      let filters = ObfsckFilters {
          filters: vec![RedactRule {
              label: "api-key".to_string(),
              pattern: r"sk-[A-Za-z0-9]{10,}".to_string(),
          }],
      };
      let output = "normal line\nsk-abc1234567890 leaked\nclean";
      let result = apply_redaction(output, &filters);
      assert!(result.contains("[REDACTED]"));
      assert!(!result.contains("sk-abc1234567890"));
      assert!(result.contains("normal line"));
  }
  ```

- [ ] **Step 2: Run test to verify it passes** (it tests `apply_redaction` directly, so it should pass after Task 3)

  ```bash
  cargo test -p crs filter_redacts_output 2>&1 | tail -10
  ```

  Expected: PASS (this test validates the function works, not the integration).

- [ ] **Step 3: Wire `apply_redaction` into `cmd_filter`**

  In `crates/crs/src/main.rs`, update `cmd_filter` to apply redaction after the existing filter
  logic. Replace the function body (lines 73–114):

  ```rust
  fn cmd_filter() {
      let Some(payload) = read_stdin_payload() else {
          return;
      };

      if payload.tool_name.as_deref() != Some("Bash") {
          return;
      }

      let command = match payload.tool_input.as_ref().and_then(|i| i.command.as_deref()) {
          Some(c) if !c.is_empty() => c.to_string(),
          _ => return,
      };

      let output = payload
          .tool_response
          .as_ref()
          .and_then(|r| r.get("output"))
          .and_then(|v| v.as_str())
          .unwrap_or("")
          .to_string();

      let exit_code = payload
          .tool_response
          .as_ref()
          .and_then(|r| r.get("exit_code"))
          .and_then(|v| v.as_i64())
          .unwrap_or(0);

      let config = crs_core::filters::load();
      let fp = FilterPayload { command, output: output.clone(), exit_code };

      // Apply compression rules first.
      let filtered_output = match run_filter(&fp, &config) {
          FilterResult::Passthrough => output.clone(),
          FilterResult::Suppress => {
              emit_message("");
              return;
          }
          FilterResult::Replace(text) => text,
      };

      // Apply obfsck redaction patterns if .ctx/obfsck-filters.yaml exists.
      let obfsck = crs_core::filters::load_obfsck_filters();
      let final_output = crs_core::filters::apply_redaction(&filtered_output, &obfsck);

      // Only emit a hook message if output changed (avoids noise on passthrough).
      if final_output != output {
          emit_message(&final_output);
      }
  }
  ```

- [ ] **Step 4: Build and run full test suite**

  ```bash
  cargo build 2>&1 | tail -5
  cargo test 2>&1 | tail -15
  ```

  Expected: compiles cleanly, all tests pass.

- [ ] **Step 5: Commit**

  ```bash
  git add crates/crs/src/main.rs
  git commit -m "feat(crs): apply obfsck redaction patterns in cmd_filter output pass"
  ```

---

## Task 5: Install `obfsck-mcp` and smoke test end-to-end

**Files:** None (install step)

- [ ] **Step 1: Build and install `obfsck-mcp`**

  ```bash
  cargo install --path /Users/joe/dev/obfsck --bin mcp 2>&1 | tail -5
  ```

  Then verify it's on PATH and responds to JSON-RPC:

  ```bash
  echo '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | obfsck-mcp
  ```

  Expected: JSON response listing `audit` and `generate-filters` tools.

- [ ] **Step 2: Run `crs discover --generate-filters` in the coursers repo**

  ```bash
  cd /Users/joe/dev/coursers
  crs discover --generate-filters 2>&1 | tail -20
  ```

  Expected: runs without error; if `.ctx/` exists, writes `.ctx/obfsck-filters.yaml`.
  Check the file:

  ```bash
  cat .ctx/obfsck-filters.yaml
  ```

- [ ] **Step 3: Run `crs discover --generate-filters` again to verify merge**

  ```bash
  crs discover --generate-filters 2>&1 | tail -5
  ```

  Expected: no duplicate entries in `.ctx/obfsck-filters.yaml`.

- [ ] **Step 4: Smoke test redaction via `crs filter`**

  ```bash
  echo '{"tool_name":"Bash","tool_input":{"command":"echo test"},"tool_response":{"output":"sk-abc1234567890abcdef leaked\nclean line","exit_code":0}}' \
    | crs filter
  ```

  Expected: response contains `[REDACTED]` and `clean line`, does not contain the fake secret.
  (Requires a pattern matching `sk-[A-Za-z0-9]{10,}` in `.ctx/obfsck-filters.yaml`.)

- [ ] **Step 5: Run full test suite one final time**

  ```bash
  cargo test --workspace 2>&1 | tail -10
  ```

  Expected: all tests pass.

- [ ] **Step 6: Final commit**

  ```bash
  git add -A
  git commit -m "docs: add coursers-11 implementation plan"
  ```

---

## Dependency Check

Before Task 3, verify `serde_yaml` is in `crates/core/Cargo.toml`:

```bash
grep serde_yaml /Users/joe/dev/coursers/crates/core/Cargo.toml
```

If absent:

```bash
cd /Users/joe/dev/coursers && cargo add serde_yaml -p crs-core
```
