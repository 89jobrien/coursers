//! Facet-based session insights: load, enrich with git context, and aggregate.
//!
//! Each facet JSON lives at `~/.claude/usage-data/facets/<session_id>.json`.
//! Git context is extracted from the first line of the matching session `.jsonl`
//! at `~/.claude/projects/**/<session_id>.jsonl`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ── Facet deserialization ──────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct FacetRecord {
    pub session_id: String,
    pub underlying_goal: Option<String>,
    #[serde(default)]
    pub goal_categories: HashMap<String, u64>,
    pub outcome: Option<String>,
    #[serde(default)]
    pub user_satisfaction_counts: HashMap<String, u64>,
    pub claude_helpfulness: Option<String>,
    pub session_type: Option<String>,
    #[serde(default)]
    pub friction_counts: HashMap<String, u64>,
    pub friction_detail: Option<String>,
    pub primary_success: Option<String>,
    pub brief_summary: Option<String>,
}

// ── Git context from session JSONL ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct GitContext {
    pub repo: String,
    pub cwd: String,
    pub branch: Option<String>,
    pub timestamp: Option<String>,
}

/// Minimal shape needed from a JSONL line to extract git context.
#[derive(Debug, Deserialize)]
struct JsonlFirstLine {
    cwd: Option<String>,
    #[serde(rename = "gitBranch")]
    git_branch: Option<String>,
    timestamp: Option<String>,
}

// ── Enriched record ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EnrichedFacet {
    pub facet: FacetRecord,
    pub git: Option<GitContext>,
}

// ── Aggregated report ──────────────────────────────────────────────────────

#[derive(Debug, Default, Serialize)]
pub struct InsightsReport {
    pub total: usize,
    pub outcomes: HashMap<String, usize>,
    pub helpfulness: HashMap<String, usize>,
    pub session_types: HashMap<String, usize>,
    pub friction: HashMap<String, u64>,
    pub goal_categories: HashMap<String, u64>,
    pub top_repos: Vec<(String, usize)>,
    pub top_branches: Vec<(String, usize)>,
    /// Sessions with git context attached
    pub git_enriched: usize,
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Load all facet JSON files from `facets_dir`.
pub fn load_facets(facets_dir: &Path) -> Vec<FacetRecord> {
    let Ok(entries) = std::fs::read_dir(facets_dir) else {
        return vec![];
    };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().map(|x| x == "json").unwrap_or(false)
                && e.file_type().map(|t| t.is_file()).unwrap_or(false)
        })
        .filter_map(|e| {
            let content = std::fs::read_to_string(e.path()).ok()?;
            serde_json::from_str::<FacetRecord>(&content).ok()
        })
        .collect()
}

/// Enrich facets with git context by scanning the projects directory for
/// matching session `.jsonl` files.
pub fn enrich(facets: Vec<FacetRecord>, projects_root: &Path) -> Vec<EnrichedFacet> {
    // Build session_id → jsonl path index once, then look up per facet.
    let index = build_jsonl_index(projects_root);

    facets
        .into_iter()
        .map(|f| {
            let git = index
                .get(&f.session_id)
                .and_then(|path| read_git_context(path));
            EnrichedFacet { facet: f, git }
        })
        .collect()
}

/// Aggregate enriched facets into a summary report.
pub fn aggregate(facets: &[EnrichedFacet]) -> InsightsReport {
    let mut report = InsightsReport {
        total: facets.len(),
        ..Default::default()
    };

    let mut repos: HashMap<String, usize> = HashMap::new();
    let mut branches: HashMap<String, usize> = HashMap::new();

    for ef in facets {
        let f = &ef.facet;

        if let Some(ref o) = f.outcome {
            *report.outcomes.entry(o.clone()).or_default() += 1;
        }
        if let Some(ref h) = f.claude_helpfulness {
            *report.helpfulness.entry(h.clone()).or_default() += 1;
        }
        if let Some(ref s) = f.session_type {
            *report.session_types.entry(s.clone()).or_default() += 1;
        }
        for (k, v) in &f.friction_counts {
            *report.friction.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &f.goal_categories {
            *report.goal_categories.entry(k.clone()).or_default() += v;
        }

        if let Some(ref git) = ef.git {
            report.git_enriched += 1;
            *repos.entry(git.repo.clone()).or_default() += 1;
            if let Some(ref b) = git.branch {
                *branches.entry(b.clone()).or_default() += 1;
            }
        }
    }

    report.top_repos = sorted_top(repos, 10);
    report.top_branches = sorted_top(branches, 10);

    report
}

// ── Internals ──────────────────────────────────────────────────────────────

/// Walk projects_root and build a map: session_id → path to .jsonl file.
/// Top-level session files only (depth 2: <project-dir>/<session-id>.jsonl).
fn build_jsonl_index(projects_root: &Path) -> HashMap<String, PathBuf> {
    let mut index = HashMap::new();
    let Ok(project_dirs) = std::fs::read_dir(projects_root) else {
        return index;
    };
    for project_entry in project_dirs.filter_map(|e| e.ok()) {
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }
        let Ok(sessions) = std::fs::read_dir(&project_path) else {
            continue;
        };
        for session_entry in sessions.filter_map(|e| e.ok()) {
            let path = session_entry.path();
            if path.extension().map(|x| x == "jsonl").unwrap_or(false)
                && path.is_file()
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            {
                index.insert(stem.to_string(), path);
            }
        }
    }
    index
}

/// Read the first line of a `.jsonl` and extract git context.
fn read_git_context(path: &Path) -> Option<GitContext> {
    let content = std::fs::read_to_string(path).ok()?;
    let first_line = content.lines().next()?;
    let parsed: JsonlFirstLine = serde_json::from_str(first_line).ok()?;

    let cwd = parsed.cwd.unwrap_or_default();
    let repo = repo_name_from_cwd(&cwd);

    Some(GitContext {
        repo,
        cwd,
        branch: parsed.git_branch,
        timestamp: parsed.timestamp,
    })
}

/// Derive a short repo name from a cwd path (last path component).
fn repo_name_from_cwd(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(cwd)
        .to_string()
}

fn sorted_top(map: HashMap<String, usize>, limit: usize) -> Vec<(String, usize)> {
    let mut v: Vec<(String, usize)> = map.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v.truncate(limit);
    v
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_facet(session_id: &str, outcome: &str, helpfulness: &str) -> FacetRecord {
        FacetRecord {
            session_id: session_id.to_string(),
            underlying_goal: None,
            goal_categories: HashMap::new(),
            outcome: Some(outcome.to_string()),
            user_satisfaction_counts: HashMap::new(),
            claude_helpfulness: Some(helpfulness.to_string()),
            session_type: Some("single_task".to_string()),
            friction_counts: HashMap::new(),
            friction_detail: None,
            primary_success: None,
            brief_summary: None,
        }
    }

    #[test]
    fn aggregate_counts_outcomes() {
        let facets = vec![
            EnrichedFacet {
                facet: make_facet("a", "fully_achieved", "very_helpful"),
                git: None,
            },
            EnrichedFacet {
                facet: make_facet("b", "fully_achieved", "moderately_helpful"),
                git: None,
            },
            EnrichedFacet {
                facet: make_facet("c", "partially_achieved", "very_helpful"),
                git: None,
            },
        ];
        let report = aggregate(&facets);
        assert_eq!(report.total, 3);
        assert_eq!(report.outcomes["fully_achieved"], 2);
        assert_eq!(report.outcomes["partially_achieved"], 1);
    }

    #[test]
    fn aggregate_counts_friction() {
        let mut f = make_facet("x", "fully_achieved", "very_helpful");
        f.friction_counts.insert("wrong_approach".to_string(), 3);
        f.friction_counts.insert("buggy_code".to_string(), 1);
        let mut f2 = make_facet("y", "fully_achieved", "very_helpful");
        f2.friction_counts.insert("wrong_approach".to_string(), 2);

        let facets = vec![
            EnrichedFacet {
                facet: f,
                git: None,
            },
            EnrichedFacet {
                facet: f2,
                git: None,
            },
        ];
        let report = aggregate(&facets);
        assert_eq!(report.friction["wrong_approach"], 5);
        assert_eq!(report.friction["buggy_code"], 1);
    }

    #[test]
    fn aggregate_counts_git_enriched() {
        let git = GitContext {
            repo: "minibox".to_string(),
            cwd: "/Users/joe/dev/minibox".to_string(),
            branch: Some("main".to_string()),
            timestamp: None,
        };
        let facets = vec![
            EnrichedFacet {
                facet: make_facet("a", "fully_achieved", "very_helpful"),
                git: Some(git),
            },
            EnrichedFacet {
                facet: make_facet("b", "fully_achieved", "very_helpful"),
                git: None,
            },
        ];
        let report = aggregate(&facets);
        assert_eq!(report.git_enriched, 1);
        assert_eq!(report.top_repos[0], ("minibox".to_string(), 1));
    }

    #[test]
    fn repo_name_from_cwd_extracts_last_component() {
        assert_eq!(repo_name_from_cwd("/Users/joe/dev/minibox"), "minibox");
        assert_eq!(
            repo_name_from_cwd("/Users/joe/dev/orca-strait"),
            "orca-strait"
        );
    }
}
