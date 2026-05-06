---
status: done
---

# rx Prefix Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Teach `crs rewrite` to transparently prepend `rx`-learned command prefixes (e.g.
`op plugin run --`) to Bash tool calls, and teach `crs filter` to confirm or discard candidate
prefixes after observing exit codes, writing confirmed mappings back to `~/.config/rx/prefixes.toml`.

**Architecture:** New `crs-core` module `rx_prefix` owns all logic: TOML loading, shell segment
splitting, prefix lookup (two-word keys then single-word), candidate probe recording, and
`prefixes.toml` merge-write. `cmd_rewrite` calls it as step 3; `cmd_filter` calls it post-
redaction for learning. A `PrefixStore` trait keeps file I/O out of unit tests.

**Tech Stack:** Rust, `shell-words 1` (already in crs-core), `toml 0.8` (already in crs-core),
`serde/serde_derive`, existing `crs-core` module conventions.

---

## File Map

| File                           | Action | Responsibility                                         |
| ------------------------------ | ------ | ------------------------------------------------------ |
| `crates/core/src/rx_prefix.rs` | Create | All rx-prefix logic: types, parsing, rewrite, learning |
| `crates/core/src/lib.rs`       | Modify | `pub mod rx_prefix;`                                   |
| `crates/crs/src/main.rs`       | Modify | Wire rx_prefix into `cmd_rewrite` and `cmd_filter`     |

---

## Task 1: Shell segment splitter

**Files:**

- Create: `crates/core/src/rx_prefix.rs`
- Modify: `crates/core/src/lib.rs`

- [ ] **Step 1: Add the module declaration**

  In `crates/core/src/lib.rs`, add after `pub mod tool_swap;`:

  ```rust
  pub mod rx_prefix;
  ```

- [ ] **Step 2: Write the failing tests**

  Create `crates/core/src/rx_prefix.rs` with just the test module (no impl yet):

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn split_simple_pipeline() {
          let segs = split_segments("cargo build | tail -5");
          assert_eq!(segs, vec![
              Segment { text: "cargo build ".to_string(), sep: Some("|".to_string()) },
              Segment { text: " tail -5".to_string(), sep: None },
          ]);
      }

      #[test]
      fn split_and_and() {
          let segs = split_segments("git add -A && git commit -m 'msg'");
          assert_eq!(segs, vec![
              Segment { text: "git add -A ".to_string(), sep: Some("&&".to_string()) },
              Segment { text: " git commit -m 'msg'".to_string(), sep: None },
          ]);
      }

      #[test]
      fn split_semicolon() {
          let segs = split_segments("echo a; echo b");
          assert_eq!(segs, vec![
              Segment { text: "echo a".to_string(), sep: Some(";".to_string()) },
              Segment { text: " echo b".to_string(), sep: None },
          ]);
      }

      #[test]
      fn split_or_or() {
          let segs = split_segments("cargo check || echo failed");
          assert_eq!(segs, vec![
              Segment { text: "cargo check ".to_string(), sep: Some("||".to_string()) },
              Segment { text: " echo failed".to_string(), sep: None },
          ]);
      }

      #[test]
      fn split_no_separator_is_single_segment() {
          let segs = split_segments("cargo test --workspace");
          assert_eq!(segs, vec![
              Segment { text: "cargo test --workspace".to_string(), sep: None },
          ]);
      }

      #[test]
      fn rejoin_preserves_separators() {
          let segs = vec![
              Segment { text: "cargo build ".to_string(), sep: Some("|".to_string()) },
              Segment { text: " tail -5".to_string(), sep: None },
          ];
          assert_eq!(rejoin(&segs), "cargo build | tail -5");
      }

      #[test]
      fn rejoin_and_and() {
          let segs = vec![
              Segment { text: "git add -A ".to_string(), sep: Some("&&".to_string()) },
              Segment { text: " git commit -m 'msg'".to_string(), sep: None },
          ];
          assert_eq!(rejoin(&segs), "git add -A && git commit -m 'msg'");
      }
  }
  ```

- [ ] **Step 3: Run to verify they fail**

  ```bash
  cargo test -p crs-core rx_prefix 2>&1 | tail -10
  ```

  Expected: compile error — `split_segments`, `Segment`, `rejoin` not defined.

- [ ] **Step 4: Implement `Segment`, `split_segments`, `rejoin`**

  Add to `crates/core/src/rx_prefix.rs` above the test module:

  ```rust
  /// One shell segment plus the separator that followed it (if any).
  #[derive(Debug, Clone, PartialEq)]
  pub struct Segment {
      /// The raw text of this segment (may have leading/trailing spaces).
      pub text: String,
      /// The separator token that terminated this segment: `&&`, `||`, `;`, or `|`.
      pub sep: Option<String>,
  }

  /// Split a shell command string into segments on `&&`, `||`, `;`, `|`.
  ///
  /// Preserves surrounding whitespace in each segment so `rejoin` is lossless.
  /// Does NOT handle quotes — splitting is purely textual, which is correct for
  /// the commands we see in practice (no quoted separators).
  pub fn split_segments(cmd: &str) -> Vec<Segment> {
      // Separators in priority order (longest first so `||` beats `|`).
      let seps = ["&&", "||", ";", "|"];
      let mut result = Vec::new();
      let mut remaining = cmd;

      'outer: loop {
          // Find the earliest separator in remaining.
          let mut earliest: Option<(usize, &str)> = None;
          for sep in &seps {
              if let Some(pos) = remaining.find(sep) {
                  if earliest.is_none() || pos < earliest.unwrap().0 {
                      earliest = Some((pos, sep));
                  }
              }
          }
          match earliest {
              None => {
                  result.push(Segment { text: remaining.to_string(), sep: None });
                  break 'outer;
              }
              Some((pos, sep)) => {
                  result.push(Segment {
                      text: remaining[..pos].to_string(),
                      sep: Some(sep.to_string()),
                  });
                  remaining = &remaining[pos + sep.len()..];
              }
          }
      }
      result
  }

  /// Reconstruct the original command string from segments.
  pub fn rejoin(segs: &[Segment]) -> String {
      let mut out = String::new();
      for seg in segs {
          out.push_str(&seg.text);
          if let Some(sep) = &seg.sep {
              out.push_str(sep);
          }
      }
      out
  }
  ```

- [ ] **Step 5: Run tests**

  ```bash
  cargo test -p crs-core rx_prefix 2>&1 | tail -10
  ```

  Expected: all 7 tests pass.

- [ ] **Step 6: Commit**

  ```bash
  git add crates/core/src/rx_prefix.rs crates/core/src/lib.rs
  git commit -m "feat(crs-core): add rx_prefix module with segment splitter/rejoiner"
  ```

---

## Task 2: TOML types + `PrefixStore` trait + file loader

**Files:**

- Modify: `crates/core/src/rx_prefix.rs`

- [ ] **Step 1: Write the failing tests**

  Add to the `tests` module in `rx_prefix.rs`:

  ```rust
  #[test]
  fn parse_prefixes_toml_mappings() {
      let toml = r#"
  learn_on_successful_fallback = true
  candidate_prefixes = [["op", "plugin", "run", "--"]]

  [mappings]
  gh = ["op", "plugin", "run", "--"]
  cargo = ["op", "plugin", "run", "--"]
  "#;
      let config: RxPrefixConfig = toml::from_str(toml).unwrap();
      assert_eq!(config.mappings.get("gh"), Some(&vec!["op".to_string(), "plugin".to_string(), "run".to_string(), "--".to_string()]));
      assert_eq!(config.mappings.get("cargo"), Some(&vec!["op".to_string(), "plugin".to_string(), "run".to_string(), "--".to_string()]));
      assert_eq!(config.candidate_prefixes, vec![vec!["op".to_string(), "plugin".to_string(), "run".to_string(), "--".to_string()]]);
      assert!(config.learn_on_successful_fallback);
  }

  #[test]
  fn parse_prefixes_toml_empty() {
      let config: RxPrefixConfig = toml::from_str("").unwrap();
      assert!(config.mappings.is_empty());
      assert!(config.candidate_prefixes.is_empty());
  }

  #[test]
  fn fake_store_load_returns_injected_config() {
      let config = RxPrefixConfig {
          mappings: std::collections::HashMap::from([
              ("gh".to_string(), vec!["op".to_string(), "plugin".to_string(), "run".to_string(), "--".to_string()]),
          ]),
          candidate_prefixes: vec![],
          learn_on_successful_fallback: true,
      };
      let store = FakePrefixStore { config: config.clone(), written: std::cell::RefCell::new(None) };
      let loaded = store.load();
      assert_eq!(loaded.mappings.get("gh"), config.mappings.get("gh"));
  }
  ```

- [ ] **Step 2: Run to verify they fail**

  ```bash
  cargo test -p crs-core rx_prefix 2>&1 | tail -10
  ```

  Expected: compile error — `RxPrefixConfig`, `FakePrefixStore`, `PrefixStore` not defined.

- [ ] **Step 3: Implement types and trait**

  Add before `split_segments` in `rx_prefix.rs`:

  ```rust
  use std::collections::HashMap;

  /// Mirrors the `~/.config/rx/prefixes.toml` schema.
  #[derive(Debug, Clone, serde::Deserialize, Default)]
  pub struct RxPrefixConfig {
      /// Definite mappings: command (or "cmd sub") → prefix argv.
      #[serde(default)]
      pub mappings: HashMap<String, Vec<String>>,
      /// Candidate prefixes to try when no mapping exists.
      #[serde(default)]
      pub candidate_prefixes: Vec<Vec<String>>,
      /// Whether to persist a successful candidate as a confirmed mapping.
      #[serde(default)]
      pub learn_on_successful_fallback: bool,
  }

  /// Port for reading and writing the rx prefix config.
  pub trait PrefixStore {
      fn load(&self) -> RxPrefixConfig;
      /// Merge-write: add `key → prefix` to existing mappings without overwriting others.
      fn confirm_mapping(&self, key: &str, prefix: &[String]);
  }

  /// File-backed implementation reading `~/.config/rx/prefixes.toml`.
  pub struct FilePrefixStore {
      pub path: std::path::PathBuf,
  }

  impl FilePrefixStore {
      pub fn default_path() -> std::path::PathBuf {
          std::env::var_os("CRS_RX_PREFIXES")
              .map(std::path::PathBuf::from)
              .unwrap_or_else(|| {
                  let base = std::env::var_os("XDG_CONFIG_HOME")
                      .map(std::path::PathBuf::from)
                      .unwrap_or_else(|| {
                          std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                              .join(".config")
                      });
                  base.join("rx").join("prefixes.toml")
              })
      }
  }

  impl PrefixStore for FilePrefixStore {
      fn load(&self) -> RxPrefixConfig {
          let Ok(content) = std::fs::read_to_string(&self.path) else {
              return RxPrefixConfig::default();
          };
          toml::from_str(&content).unwrap_or_default()
      }

      fn confirm_mapping(&self, key: &str, prefix: &[String]) {
          let mut config = self.load();
          config.mappings.insert(key.to_string(), prefix.to_vec());
          if let Ok(serialized) = toml::to_string_pretty(&config) {
              let _ = std::fs::write(&self.path, serialized);
          }
      }
  }

  /// Test double — injected config, records what was written.
  #[cfg(test)]
  pub struct FakePrefixStore {
      pub config: RxPrefixConfig,
      pub written: std::cell::RefCell<Option<(String, Vec<String>)>>,
  }

  #[cfg(test)]
  impl PrefixStore for FakePrefixStore {
      fn load(&self) -> RxPrefixConfig { self.config.clone() }
      fn confirm_mapping(&self, key: &str, prefix: &[String]) {
          *self.written.borrow_mut() = Some((key.to_string(), prefix.to_vec()));
      }
  }
  ```

  Also add `use serde::Deserialize;` at the top of the file (or rely on the path `serde::Deserialize`
  on the derive — the derive path is fine as-is with the workspace serde dep).

- [ ] **Step 4: Run tests**

  ```bash
  cargo test -p crs-core rx_prefix 2>&1 | tail -10
  ```

  Expected: all tests pass.

- [ ] **Step 5: Commit**

  ```bash
  git add crates/core/src/rx_prefix.rs
  git commit -m "feat(crs-core): RxPrefixConfig, PrefixStore trait, FilePrefixStore"
  ```

---

## Task 3: Prefix lookup — two-word key, single-word key, candidate fallback

**Files:**

- Modify: `crates/core/src/rx_prefix.rs`

- [ ] **Step 1: Write the failing tests**

  Add to the `tests` module:

  ```rust
  fn make_store(mappings: &[(&str, &[&str])], candidates: &[&[&str]]) -> FakePrefixStore {
      FakePrefixStore {
          config: RxPrefixConfig {
              mappings: mappings.iter().map(|(k, v)| {
                  (k.to_string(), v.iter().map(|s| s.to_string()).collect())
              }).collect(),
              candidate_prefixes: candidates.iter().map(|c| {
                  c.iter().map(|s| s.to_string()).collect()
              }).collect(),
              learn_on_successful_fallback: true,
          },
          written: std::cell::RefCell::new(None),
      }
  }

  #[test]
  fn lookup_single_word_key_matches() {
      let store = make_store(&[("gh", &["op", "plugin", "run", "--"])], &[]);
      let result = lookup_prefix("gh issue list", &store.load());
      assert_eq!(result, Some(PrefixMatch::Confirmed {
          key: "gh".to_string(),
          prefix: vec!["op".to_string(), "plugin".to_string(), "run".to_string(), "--".to_string()],
      }));
  }

  #[test]
  fn lookup_two_word_key_wins_over_single() {
      let store = make_store(&[
          ("cargo", &["op", "plugin", "run", "--"]),
          ("cargo test", &["dotenvx", "run", "--"]),
      ], &[]);
      let result = lookup_prefix("cargo test --workspace", &store.load());
      assert_eq!(result, Some(PrefixMatch::Confirmed {
          key: "cargo test".to_string(),
          prefix: vec!["dotenvx".to_string(), "run".to_string(), "--".to_string()],
      }));
  }

  #[test]
  fn lookup_no_match_no_candidates_returns_none() {
      let store = make_store(&[], &[]);
      let result = lookup_prefix("echo hello", &store.load());
      assert_eq!(result, None);
  }

  #[test]
  fn lookup_no_match_with_candidate_returns_candidate() {
      let store = make_store(&[], &[&["op", "plugin", "run", "--"]]);
      let result = lookup_prefix("gh issue list", &store.load());
      assert_eq!(result, Some(PrefixMatch::Candidate {
          key: "gh".to_string(),
          prefix: vec!["op".to_string(), "plugin".to_string(), "run".to_string(), "--".to_string()],
      }));
  }

  #[test]
  fn lookup_skips_command_with_subshell() {
      let store = make_store(&[("gh", &["op", "plugin", "run", "--"])], &[]);
      let result = lookup_prefix("$(gh issue list)", &store.load());
      assert_eq!(result, None);
  }
  ```

- [ ] **Step 2: Run to verify they fail**

  ```bash
  cargo test -p crs-core rx_prefix 2>&1 | tail -10
  ```

  Expected: compile error — `lookup_prefix`, `PrefixMatch` not defined.

- [ ] **Step 3: Implement `PrefixMatch` and `lookup_prefix`**

  Add after the `FakePrefixStore` impl in `rx_prefix.rs`:

  ```rust
  /// Result of a prefix lookup for a single segment's base command.
  #[derive(Debug, Clone, PartialEq)]
  pub enum PrefixMatch {
      /// A definite mapping exists in `mappings`.
      Confirmed { key: String, prefix: Vec<String> },
      /// No mapping; a candidate prefix is being tried speculatively.
      Candidate { key: String, prefix: Vec<String> },
  }

  /// Look up the prefix for the leading command word(s) of `segment`.
  ///
  /// Returns `None` if:
  /// - `segment` contains `$(` or a backtick (subshell — unsafe to rewrite blindly)
  /// - no mapping or candidate applies
  ///
  /// Two-word key check happens before single-word.
  pub fn lookup_prefix(segment: &str, config: &RxPrefixConfig) -> Option<PrefixMatch> {
      let trimmed = segment.trim();
      if trimmed.contains("$(") || trimmed.contains('`') {
          return None;
      }

      // Tokenize to extract base words.
      let tokens = shell_words::split(trimmed).ok()?;
      let first = tokens.first()?.as_str();
      let second = tokens.get(1).map(|s| s.as_str());

      // Two-word key check.
      if let Some(second) = second {
          let two_word = format!("{first} {second}");
          if let Some(prefix) = config.mappings.get(&two_word) {
              return Some(PrefixMatch::Confirmed { key: two_word, prefix: prefix.clone() });
          }
      }

      // Single-word key check.
      if let Some(prefix) = config.mappings.get(first) {
          return Some(PrefixMatch::Confirmed { key: first.to_string(), prefix: prefix.clone() });
      }

      // Candidate fallback — use the first candidate prefix.
      if let Some(candidate) = config.candidate_prefixes.first() {
          return Some(PrefixMatch::Candidate {
              key: first.to_string(),
              prefix: candidate.clone(),
          });
      }

      None
  }
  ```

- [ ] **Step 4: Run tests**

  ```bash
  cargo test -p crs-core rx_prefix 2>&1 | tail -10
  ```

  Expected: all new tests pass, no regressions.

- [ ] **Step 5: Commit**

  ```bash
  git add crates/core/src/rx_prefix.rs
  git commit -m "feat(crs-core): prefix lookup with two-word keys and candidate fallback"
  ```

---

## Task 4: Full command rewriter + candidate probe recorder

**Files:**

- Modify: `crates/core/src/rx_prefix.rs`

- [ ] **Step 1: Write the failing tests**

  Add to the `tests` module:

  ```rust
  #[test]
  fn rewrite_simple_command_with_confirmed_prefix() {
      let store = make_store(&[("gh", &["op", "plugin", "run", "--"])], &[]);
      let result = rewrite_command("gh issue list", &store.load());
      assert_eq!(result.rewritten, "op plugin run -- gh issue list");
      assert!(result.probes.is_empty());
  }

  #[test]
  fn rewrite_pipeline_rewrites_each_segment() {
      let store = make_store(&[("gh", &["op", "plugin", "run", "--"])], &[]);
      let result = rewrite_command("gh issue list | tail -5", &store.load());
      assert_eq!(result.rewritten, "op plugin run -- gh issue list | tail -5");
      assert!(result.probes.is_empty());
  }

  #[test]
  fn rewrite_compound_rewrites_each_segment_independently() {
      let store = make_store(&[("gh", &["op", "plugin", "run", "--"])], &[]);
      let result = rewrite_command("gh issue list && gh pr list", &store.load());
      assert_eq!(result.rewritten, "op plugin run -- gh issue list && op plugin run -- gh pr list");
  }

  #[test]
  fn rewrite_candidate_records_probe() {
      let store = make_store(&[], &[&["op", "plugin", "run", "--"]]);
      let result = rewrite_command("gh issue list", &store.load());
      assert_eq!(result.rewritten, "op plugin run -- gh issue list");
      assert_eq!(result.probes.len(), 1);
      assert_eq!(result.probes[0].key, "gh");
      assert_eq!(result.probes[0].prefix, vec!["op", "plugin", "run", "--"]);
      assert_eq!(result.probes[0].original_command, "gh issue list");
  }

  #[test]
  fn rewrite_no_match_returns_unchanged() {
      let store = make_store(&[], &[]);
      let result = rewrite_command("echo hello", &store.load());
      assert_eq!(result.rewritten, "echo hello");
      assert!(result.probes.is_empty());
  }

  #[test]
  fn rewrite_unchanged_when_already_prefixed() {
      // If the command already starts with the prefix program, don't double-wrap.
      let store = make_store(&[("gh", &["op", "plugin", "run", "--"])], &[]);
      let result = rewrite_command("op plugin run -- gh issue list", &store.load());
      // "op" has no mapping, so first segment is unchanged.
      assert_eq!(result.rewritten, "op plugin run -- gh issue list");
  }
  ```

- [ ] **Step 2: Run to verify they fail**

  ```bash
  cargo test -p crs-core rx_prefix 2>&1 | tail -10
  ```

  Expected: compile error — `rewrite_command`, `RewriteResult`, `ProbeEntry` not defined.

- [ ] **Step 3: Implement `rewrite_command`**

  Add after `lookup_prefix` in `rx_prefix.rs`:

  ```rust
  /// A pending candidate probe: we applied a speculative prefix and need post-hook
  /// learning to confirm or discard it.
  #[derive(Debug, Clone, PartialEq)]
  pub struct ProbeEntry {
      /// The command key (first word or two-word key) that was matched.
      pub key: String,
      /// The candidate prefix that was applied.
      pub prefix: Vec<String>,
      /// The original command before prefix was prepended (for matching in post-hook).
      pub original_command: String,
  }

  /// Result of rewriting a full command string.
  #[derive(Debug, Clone)]
  pub struct RewriteResult {
      /// The rewritten command (may be identical to input if nothing matched).
      pub rewritten: String,
      /// Candidate probes that need post-hook learning.
      pub probes: Vec<ProbeEntry>,
  }

  /// Rewrite `cmd` by prepending learned rx prefixes to each shell segment.
  ///
  /// Segments containing `$(` or backticks are passed through unchanged.
  /// Returns the rewritten command and any speculative candidate probes recorded.
  pub fn rewrite_command(cmd: &str, config: &RxPrefixConfig) -> RewriteResult {
      let mut segs = split_segments(cmd);
      let mut probes = Vec::new();

      for seg in &mut segs {
          let trimmed = seg.text.trim();
          if trimmed.is_empty() {
              continue;
          }
          let Some(m) = lookup_prefix(trimmed, config) else {
              continue;
          };
          let (key, prefix, is_candidate) = match m {
              PrefixMatch::Confirmed { key, prefix } => (key, prefix, false),
              PrefixMatch::Candidate { key, prefix } => (key, prefix, true),
          };
          // Build the prefixed segment text, preserving leading whitespace.
          let leading = &seg.text[..seg.text.len() - seg.text.trim_start().len()];
          let prefix_str = prefix.join(" ");
          seg.text = format!("{leading}{prefix_str} {trimmed}");

          if is_candidate {
              probes.push(ProbeEntry {
                  key,
                  prefix,
                  original_command: cmd.to_string(),
              });
          }
      }

      RewriteResult { rewritten: rejoin(&segs), probes }
  }
  ```

- [ ] **Step 4: Run tests**

  ```bash
  cargo test -p crs-core rx_prefix 2>&1 | tail -15
  ```

  Expected: all tests pass.

- [ ] **Step 5: Commit**

  ```bash
  git add crates/core/src/rx_prefix.rs
  git commit -m "feat(crs-core): rewrite_command — segment-level prefix injection with probe recording"
  ```

---

## Task 5: Candidate probe TOML persistence

**Files:**

- Modify: `crates/core/src/rx_prefix.rs`

The probe store persists `ProbeEntry` records to `.ctx/rx-candidates.toml` so that
`cmd_filter` can match them against the just-executed command's exit code.

- [ ] **Step 1: Write the failing tests**

  Add to the `tests` module:

  ```rust
  #[test]
  fn probe_store_round_trips_entries() {
      use tempfile::TempDir;
      let dir = TempDir::new().unwrap();
      let path = dir.path().join("rx-candidates.toml");
      let store = FileProbeStore { path: path.clone() };

      let entries = vec![
          ProbeEntry {
              key: "gh".to_string(),
              prefix: vec!["op".to_string(), "plugin".to_string(), "run".to_string(), "--".to_string()],
              original_command: "gh issue list".to_string(),
          },
      ];
      store.write(&entries);
      let loaded = store.load();
      assert_eq!(loaded.len(), 1);
      assert_eq!(loaded[0].key, "gh");
      assert_eq!(loaded[0].original_command, "gh issue list");
  }

  #[test]
  fn probe_store_load_missing_file_returns_empty() {
      let store = FileProbeStore { path: std::path::PathBuf::from("/nonexistent/rx-candidates.toml") };
      assert!(store.load().is_empty());
  }

  #[test]
  fn probe_store_remove_matching_removes_only_that_entry() {
      use tempfile::TempDir;
      let dir = TempDir::new().unwrap();
      let path = dir.path().join("rx-candidates.toml");
      let store = FileProbeStore { path: path.clone() };

      let entries = vec![
          ProbeEntry { key: "gh".to_string(), prefix: vec!["op".to_string()], original_command: "gh issue list".to_string() },
          ProbeEntry { key: "cargo".to_string(), prefix: vec!["op".to_string()], original_command: "cargo build".to_string() },
      ];
      store.write(&entries);
      store.remove_matching("gh issue list");
      let remaining = store.load();
      assert_eq!(remaining.len(), 1);
      assert_eq!(remaining[0].key, "cargo");
  }
  ```

  Add `tempfile = "3"` to `[dev-dependencies]` in `crates/core/Cargo.toml` if not already present:

  ```toml
  [dev-dependencies]
  tempfile = "3"
  ```

- [ ] **Step 2: Run to verify they fail**

  ```bash
  cargo test -p crs-core rx_prefix 2>&1 | tail -10
  ```

  Expected: compile error — `FileProbeStore` not defined.

- [ ] **Step 3: Implement `FileProbeStore`**

  Add a TOML wrapper and the store after `rewrite_command` in `rx_prefix.rs`:

  ```rust
  /// TOML-serializable wrapper for a list of probe entries.
  #[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
  struct ProbeFile {
      #[serde(default)]
      probes: Vec<ProbeEntryToml>,
  }

  #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
  struct ProbeEntryToml {
      key: String,
      prefix: Vec<String>,
      original_command: String,
  }

  impl From<&ProbeEntry> for ProbeEntryToml {
      fn from(e: &ProbeEntry) -> Self {
          Self { key: e.key.clone(), prefix: e.prefix.clone(), original_command: e.original_command.clone() }
      }
  }

  impl From<ProbeEntryToml> for ProbeEntry {
      fn from(t: ProbeEntryToml) -> Self {
          Self { key: t.key, prefix: t.prefix, original_command: t.original_command }
      }
  }

  /// File-backed probe store at `.ctx/rx-candidates.toml`.
  pub struct FileProbeStore {
      pub path: std::path::PathBuf,
  }

  impl FileProbeStore {
      pub fn default_path() -> std::path::PathBuf {
          std::path::Path::new(".ctx").join("rx-candidates.toml")
      }

      pub fn load(&self) -> Vec<ProbeEntry> {
          let Ok(content) = std::fs::read_to_string(&self.path) else {
              return Vec::new();
          };
          toml::from_str::<ProbeFile>(&content)
              .unwrap_or_default()
              .probes
              .into_iter()
              .map(ProbeEntry::from)
              .collect()
      }

      pub fn write(&self, entries: &[ProbeEntry]) {
          let file = ProbeFile {
              probes: entries.iter().map(ProbeEntryToml::from).collect(),
          };
          if let Ok(serialized) = toml::to_string_pretty(&file) {
              let _ = std::fs::write(&self.path, serialized);
          }
      }

      /// Remove all probe entries whose `original_command` matches `cmd`.
      pub fn remove_matching(&self, cmd: &str) {
          let mut entries = self.load();
          entries.retain(|e| e.original_command != cmd);
          self.write(&entries);
      }
  }
  ```

- [ ] **Step 4: Run tests**

  ```bash
  cargo test -p crs-core rx_prefix 2>&1 | tail -10
  ```

  Expected: all tests pass.

- [ ] **Step 5: Commit**

  ```bash
  git add crates/core/src/rx_prefix.rs crates/core/Cargo.toml
  git commit -m "feat(crs-core): FileProbeStore — persist/load/remove candidate probes in .ctx/rx-candidates.toml"
  ```

---

## Task 6: Wire into `cmd_rewrite`

**Files:**

- Modify: `crates/crs/src/main.rs`

- [ ] **Step 1: Write the failing integration test**

  Add to the `cli_tests` module in `crates/crs/src/main.rs`:

  ```rust
  #[test]
  fn rewrite_applies_rx_prefix_when_prefixes_toml_present() {
      use crs_core::rx_prefix::{RxPrefixConfig, rewrite_command};
      use std::collections::HashMap;

      let config = RxPrefixConfig {
          mappings: HashMap::from([
              ("gh".to_string(), vec!["op".to_string(), "plugin".to_string(), "run".to_string(), "--".to_string()]),
          ]),
          candidate_prefixes: vec![],
          learn_on_successful_fallback: false,
      };
      let result = rewrite_command("gh issue list", &config);
      assert_eq!(result.rewritten, "op plugin run -- gh issue list");
      assert!(result.probes.is_empty());
  }
  ```

- [ ] **Step 2: Run to verify it passes** (tests `rewrite_command` directly — it will pass after Task 4)

  ```bash
  cargo test -p crs rewrite_applies_rx_prefix 2>&1 | tail -10
  ```

  Expected: PASS.

- [ ] **Step 3: Wire into `cmd_rewrite`**

  In `crates/crs/src/main.rs`, update `cmd_rewrite` to add step 3 after the existing regex rewrite.
  Replace the `None =>` branch of `run_rewrite`:

  ```rust
  fn cmd_rewrite() {
      let Some(payload) = read_stdin_payload() else {
          std::process::exit(1);
      };

      if payload.tool_name.as_deref() != Some("Bash") {
          std::process::exit(1);
      }

      let command = match payload.tool_input.as_ref().and_then(|i| i.command.as_deref()) {
          Some(c) if !c.is_empty() => c,
          _ => std::process::exit(1),
      };

      // 1. Try AST tool swap first
      let filters_cfg = crs_core::filters::load();
      let swap = crs_core::tool_swap::apply(command, &filters_cfg.tool_swap);
      if let crs_core::tool_swap::ToolAction::SwapTool { tool_name, tool_input } = swap {
          emit_tool_swap(&tool_name, tool_input);
          return;
      }

      // 2. Regex rewrite rules from crs-filters.toml
      let config = load_rewrite_config();
      if let Some(rewritten) = run_rewrite(command, &config) {
          emit_rewrite(&rewritten);
          return;
      }

      // 3. rx prefix injection
      let rx_config = crs_core::rx_prefix::FilePrefixStore {
          path: crs_core::rx_prefix::FilePrefixStore::default_path(),
      }.load();
      let result = crs_core::rx_prefix::rewrite_command(command, &rx_config);
      if result.rewritten != command {
          // Persist any candidate probes for post-hook learning.
          if !result.probes.is_empty() {
              let probe_store = crs_core::rx_prefix::FileProbeStore {
                  path: crs_core::rx_prefix::FileProbeStore::default_path(),
              };
              let mut existing = probe_store.load();
              existing.extend(result.probes.clone());
              probe_store.write(&existing);
          }
          emit_rewrite(&result.rewritten);
          return;
      }

      // No rewrite matched.
      std::process::exit(1);
  }
  ```

  Note: `run_rewrite` already returns `Option<String>` — the existing `match` needed changing to
  early-return style. The complete replacement above is the full function body.

- [ ] **Step 4: Build and test**

  ```bash
  cargo build -p crs 2>&1 | tail -5
  cargo test -p crs 2>&1 | tail -10
  ```

  Expected: compiles, all tests pass.

- [ ] **Step 5: Commit**

  ```bash
  git add crates/crs/src/main.rs
  git commit -m "feat(crs): wire rx prefix injection into cmd_rewrite as step 3"
  ```

---

## Task 7: Post-hook learning in `cmd_filter`

**Files:**

- Modify: `crates/crs/src/main.rs`

When a candidate-prefixed command succeeds (exit_code == 0), confirm the mapping into
`~/.config/rx/prefixes.toml`. When it fails, discard the probe.

- [ ] **Step 1: Write the failing test**

  Add to `cli_tests` in `crates/crs/src/main.rs`:

  ```rust
  #[test]
  fn rx_learning_confirms_mapping_on_success() {
      use crs_core::rx_prefix::{ProbeEntry, FileProbeStore, FilePrefixStore, RxPrefixConfig};
      use tempfile::TempDir;

      let dir = TempDir::new().unwrap();
      let probe_path = dir.path().join("rx-candidates.toml");
      let prefixes_path = dir.path().join("prefixes.toml");

      let probe_store = FileProbeStore { path: probe_path.clone() };
      probe_store.write(&[ProbeEntry {
          key: "gh".to_string(),
          prefix: vec!["op".to_string(), "plugin".to_string(), "run".to_string(), "--".to_string()],
          original_command: "gh issue list".to_string(),
      }]);

      let prefix_store = FilePrefixStore { path: prefixes_path.clone() };

      // Simulate a successful exit.
      apply_rx_learning("gh issue list", 0, &probe_store, &prefix_store);

      let config = prefix_store.load();
      assert_eq!(
          config.mappings.get("gh"),
          Some(&vec!["op".to_string(), "plugin".to_string(), "run".to_string(), "--".to_string()])
      );
      // Probe should have been removed.
      assert!(probe_store.load().is_empty());
  }

  #[test]
  fn rx_learning_removes_probe_on_failure() {
      use crs_core::rx_prefix::{ProbeEntry, FileProbeStore, FilePrefixStore};
      use tempfile::TempDir;

      let dir = TempDir::new().unwrap();
      let probe_path = dir.path().join("rx-candidates.toml");
      let prefixes_path = dir.path().join("prefixes.toml");

      let probe_store = FileProbeStore { path: probe_path.clone() };
      probe_store.write(&[ProbeEntry {
          key: "gh".to_string(),
          prefix: vec!["op".to_string()],
          original_command: "gh issue list".to_string(),
      }]);

      let prefix_store = FilePrefixStore { path: prefixes_path.clone() };

      apply_rx_learning("gh issue list", 1, &probe_store, &prefix_store);

      // Probe removed, mapping NOT written.
      assert!(probe_store.load().is_empty());
      assert!(prefix_store.load().mappings.is_empty());
  }
  ```

- [ ] **Step 2: Run to verify they fail**

  ```bash
  cargo test -p crs rx_learning 2>&1 | tail -10
  ```

  Expected: compile error — `apply_rx_learning` not defined.

- [ ] **Step 3: Implement `apply_rx_learning` and wire into `cmd_filter`**

  Add a free function in `crates/crs/src/main.rs` (before `cmd_filter`):

  ```rust
  fn apply_rx_learning(
      command: &str,
      exit_code: i64,
      probe_store: &crs_core::rx_prefix::FileProbeStore,
      prefix_store: &crs_core::rx_prefix::FilePrefixStore,
  ) {
      let probes = probe_store.load();
      let matching: Vec<_> = probes.iter()
          .filter(|p| p.original_command == command)
          .collect();
      if matching.is_empty() {
          return;
      }
      if exit_code == 0 {
          for probe in &matching {
              prefix_store.confirm_mapping(&probe.key, &probe.prefix);
          }
      }
      // Always remove probes for this command regardless of success/failure.
      probe_store.remove_matching(command);
  }
  ```

  Then add a call to `apply_rx_learning` at the end of `cmd_filter`, after the redaction pass,
  before the `if final_output != output` check:

  ```rust
  // Post-hook rx learning: confirm or discard candidate prefix probes.
  let probe_store = crs_core::rx_prefix::FileProbeStore {
      path: crs_core::rx_prefix::FileProbeStore::default_path(),
  };
  let prefix_store = crs_core::rx_prefix::FilePrefixStore {
      path: crs_core::rx_prefix::FilePrefixStore::default_path(),
  };
  apply_rx_learning(&command, exit_code, &probe_store, &prefix_store);
  ```

- [ ] **Step 4: Build and run full test suite**

  ```bash
  cargo build 2>&1 | tail -5
  cargo test --workspace 2>&1 | tail -15
  ```

  Expected: compiles cleanly, all tests pass.

- [ ] **Step 5: Commit**

  ```bash
  git add crates/crs/src/main.rs
  git commit -m "feat(crs): post-hook rx learning — confirm candidate mappings on success, discard on failure"
  ```

---

## Dependency Check

Before starting Task 1, verify `tempfile` is in `[dev-dependencies]` of `crates/core/Cargo.toml`:

```bash
grep tempfile crates/core/Cargo.toml
```

If absent, add:

```toml
[dev-dependencies]
tempfile = "3"
```

`shell-words` and `toml` are already workspace deps used by `crs-core` — no changes needed there.
`serde` with `derive` feature is already a workspace dep.

---

## Self-Review

**Spec coverage:**

| Requirement                                                   | Task                                     |
| ------------------------------------------------------------- | ---------------------------------------- | --------- | --- | ------ |
| Load `~/.config/rx/prefixes.toml`, `CRS_RX_PREFIXES` override | Task 2 (`FilePrefixStore::default_path`) |
| Split on `&&`, `                                              |                                          | `, `;`, ` | `   | Task 1 |
| Two-word key lookup before single-word                        | Task 3                                   |
| Subshell `$(` / backtick skip                                 | Task 3 (`lookup_prefix`)                 |
| Candidate prefix fallback + probe recording                   | Tasks 3 + 4                              |
| Rejoin with preserved separators                              | Task 1                                   |
| Wire into `cmd_rewrite` as step 3                             | Task 6                                   |
| Post-hook: confirm mapping on success                         | Task 7                                   |
| Post-hook: remove probe on failure                            | Task 7                                   |
| `PrefixStore` trait for testability                           | Task 2                                   |

All requirements covered. No placeholders found. Type names are consistent across all tasks
(`ProbeEntry`, `FileProbeStore`, `FilePrefixStore`, `RxPrefixConfig`, `rewrite_command`,
`apply_rx_learning`).
