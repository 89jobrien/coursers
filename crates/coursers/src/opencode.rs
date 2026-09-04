use std::io::{self, Write};

use coursers_core::hook::chain::{
    HookContext as ChainContext, PostHookOutcome, PreHookOutcome, ToolOutput,
};
use coursers_core::hook_pipeline::{HookContext as PipelineContext, HookEvent};
use miette::IntoDiagnostic;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum OpenCodeDecision {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct OpenCodeHookResponse {
    decision: OpenCodeDecision,
    reason: Option<String>,
    updated_input: Option<Value>,
    replacement_output: Option<String>,
    messages: Vec<String>,
    matched_rules: Vec<String>,
}

impl Default for OpenCodeHookResponse {
    fn default() -> Self {
        Self {
            decision: OpenCodeDecision::Allow,
            reason: None,
            updated_input: None,
            replacement_output: None,
            messages: Vec::new(),
            matched_rules: Vec::new(),
        }
    }
}

pub(crate) fn run_hook(event: HookEvent, raw_json: &str) -> miette::Result<OpenCodeHookResponse> {
    let normalized_json = if raw_json.trim().is_empty()
        && !matches!(event, HookEvent::PreToolUse | HookEvent::PostToolUse)
    {
        "{}"
    } else {
        raw_json
    };
    let mut payload: Value = serde_json::from_str(normalized_json).into_diagnostic()?;
    if !payload.is_object() {
        return Err(miette::miette!(
            "normalized OpenCode request must be a JSON object"
        ));
    }

    let is_tool_event = matches!(event, HookEvent::PreToolUse | HookEvent::PostToolUse);
    let tool_name = payload
        .get("tool_name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .map(str::to_owned);
    let tool_input = payload
        .get("tool_input")
        .filter(|input| input.is_object())
        .cloned();

    if is_tool_event && tool_name.is_none() {
        return Err(miette::miette!(
            "normalized OpenCode tool request requires string field tool_name"
        ));
    }
    if is_tool_event && tool_input.is_none() {
        return Err(miette::miette!(
            "normalized OpenCode tool request requires object field tool_input"
        ));
    }

    let tool_input = tool_input.unwrap_or_else(|| json!({}));
    if is_tool_event
        && tool_name.as_deref() == Some("Bash")
        && tool_input
            .get("command")
            .and_then(Value::as_str)
            .filter(|command| !command.is_empty())
            .is_none()
    {
        return Err(miette::miette!(
            "normalized OpenCode Bash request requires non-empty string field tool_input.command"
        ));
    }

    let tool_response = payload.get("tool_response");
    if event == HookEvent::PostToolUse && !tool_response.is_some_and(Value::is_object) {
        return Err(miette::miette!(
            "normalized OpenCode post-tool request requires object field tool_response"
        ));
    }
    let exit_code = tool_response
        .and_then(|value| value.get("exit_code"))
        .and_then(Value::as_i64);
    let output = tool_response
        .and_then(|value| value.get("output"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    if event == HookEvent::PostToolUse && (exit_code.is_none() || output.is_none()) {
        return Err(miette::miette!(
            "normalized OpenCode post-tool request requires integer exit_code and string output"
        ));
    }

    let mut response = OpenCodeHookResponse::default();
    let mut effective_input = tool_input.clone();

    if tool_name.as_deref() == Some("Bash") {
        let profile = coursers_core::config::ConfigBuilder::new().build();
        let chain = profile.build_hook_chain();
        let context = ChainContext::new("Bash", tool_input.clone());

        match event {
            HookEvent::PreToolUse => match chain.run_pre(&context)? {
                PreHookOutcome::Allow => {}
                PreHookOutcome::Deny(reason) => {
                    response.decision = OpenCodeDecision::Deny;
                    response.reason = Some(reason);
                }
                PreHookOutcome::Rewrite { command, reason } => {
                    effective_input["command"] = Value::String(command);
                    response.updated_input = Some(effective_input.clone());
                    response.messages.push(reason);
                }
            },
            HookEvent::PostToolUse => {
                let output = ToolOutput {
                    text: output.clone().unwrap_or_default(),
                    exit_code: exit_code.unwrap_or(0),
                };
                if let PostHookOutcome::Filter(text) = chain.run_post(&context, &output)? {
                    response.replacement_output = Some(text);
                }
            }
            _ => {}
        }
    }

    if is_tool_event {
        payload["tool_input"] = effective_input.clone();
    }
    let target = effective_input
        .get("command")
        .or_else(|| effective_input.get("file_path"))
        .or_else(|| payload.get("target"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let pipeline_json = serde_json::to_string(&payload).into_diagnostic()?;
    let context = PipelineContext {
        event: Some(event),
        tool_name,
        target,
        exit_code,
        raw_json: Some(pipeline_json),
        output,
    };
    let pipeline = coursers_core::hook_pipeline::run_pipeline(
        &coursers_core::hook_pipeline::load_config(),
        &context,
    );

    if !pipeline.matched_rules.is_empty()
        && let Ok(db) = coursers_core::hook::log::open_db(&coursers_core::hook::log::db_path())
    {
        let entry = coursers_core::hook::log::entry_from_pipeline(
            &context,
            &pipeline,
            pipeline.matched_rules.clone(),
        );
        coursers_core::hook::log::record(&db, &entry);
    }

    if response.decision != OpenCodeDecision::Deny
        && let Some(reason) = pipeline.deny
    {
        response.decision = OpenCodeDecision::Deny;
        response.reason = Some(reason);
    }
    if let Some(command) = pipeline.rewrite {
        effective_input["command"] = Value::String(command);
        response.updated_input = Some(effective_input);
    }
    if let Some(output) = pipeline.replace_output {
        response.replacement_output = Some(output);
    }
    response.messages.extend(pipeline.messages);
    response.matched_rules.extend(pipeline.matched_rules);

    Ok(response)
}

pub(crate) fn write_hook_response(response: &OpenCodeHookResponse) -> miette::Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    serde_json::to_writer(&mut handle, response).into_diagnostic()?;
    writeln!(handle).into_diagnostic()?;
    handle.flush().into_diagnostic()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenCodeValidation {
    plugin_path: Option<std::path::PathBuf>,
    declaration_path: Option<std::path::PathBuf>,
    missing_binaries: Vec<String>,
}

pub(crate) fn validate_installation(
    home: &std::path::Path,
    current_dir: &std::path::Path,
) -> OpenCodeValidation {
    let project_dir = current_dir.join(".opencode/plugins");
    let global_dir = home.join(".config/opencode/plugins");
    let project_plugin = project_dir.join("coursers.ts");
    let project_declaration = project_dir.join("opencode-plugin.d.ts");
    let global_plugin = global_dir.join("coursers.ts");
    let global_declaration = global_dir.join("opencode-plugin.d.ts");
    let install_dir = if project_plugin.exists() || project_declaration.exists() {
        project_dir
    } else if global_plugin.exists() || global_declaration.exists() {
        global_dir
    } else {
        project_dir
    };
    let plugin = install_dir.join("coursers.ts");
    let declaration = install_dir.join("opencode-plugin.d.ts");
    let plugin_path = plugin.is_file().then_some(plugin);
    let declaration_path = declaration.is_file().then_some(declaration);
    let missing_binaries = ["crs", "opencode"]
        .into_iter()
        .filter(|binary| {
            std::process::Command::new("which")
                .arg(binary)
                .output()
                .map(|output| !output.status.success())
                .unwrap_or(true)
        })
        .map(str::to_owned)
        .collect();

    OpenCodeValidation {
        plugin_path,
        declaration_path,
        missing_binaries,
    }
}

pub(crate) fn cmd_validate_hooks() {
    let Some(home) = dirs::home_dir() else {
        eprintln!("crs validate-hooks: cannot resolve home directory");
        std::process::exit(1);
    };
    let current_dir = std::env::current_dir().unwrap_or_else(|error| {
        eprintln!("crs validate-hooks: cannot resolve current directory: {error}");
        std::process::exit(1);
    });
    let validation = validate_installation(&home, &current_dir);

    match &validation.plugin_path {
        Some(path) => println!("OpenCode plugin: {}", path.display()),
        None => println!(
            "  MISSING plugin: .opencode/plugins/coursers.ts or {}",
            home.join(".config/opencode/plugins/coursers.ts").display()
        ),
    }
    match &validation.declaration_path {
        Some(path) => println!("OpenCode declaration: {}", path.display()),
        None => println!("  MISSING declaration: opencode-plugin.d.ts beside coursers.ts"),
    }
    for binary in &validation.missing_binaries {
        println!("  MISSING binary: {binary}");
    }

    if validation.plugin_path.is_some()
        && validation.declaration_path.is_some()
        && validation.missing_binaries.is_empty()
    {
        println!("OpenCode plugin files installed. Binaries available.");
    } else {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opencode_response_serializes_stable_contract() {
        let response = OpenCodeHookResponse {
            decision: OpenCodeDecision::Allow,
            reason: None,
            updated_input: Some(json!({"command": "nu -c ls"})),
            replacement_output: None,
            messages: vec!["rewritten".to_string()],
            matched_rules: vec!["prefer-nu".to_string()],
        };
        assert_eq!(
            serde_json::to_value(response).unwrap(),
            json!({
                "decision": "allow",
                "reason": null,
                "updated_input": {"command": "nu -c ls"},
                "replacement_output": null,
                "messages": ["rewritten"],
                "matched_rules": ["prefer-nu"]
            })
        );
    }
}
