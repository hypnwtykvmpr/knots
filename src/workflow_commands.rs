use crate::app;
use crate::cli;
use crate::domain::knot_type::KnotType;
use crate::installed_workflows;
use std::io::{self, BufRead, IsTerminal, Write};

fn parse_bool_flag(raw: &str) -> Result<bool, app::AppError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "yes" | "true" | "1" => Ok(true),
        "no" | "false" | "0" => Ok(false),
        other => Err(app::AppError::InvalidArgument(format!(
            "invalid boolean value '{}'; expected yes|true|1|no|false|0",
            other
        ))),
    }
}

trait PromptEnv {
    fn stdin_is_terminal(&self) -> bool;
    fn write_prompt(&mut self, text: &str) -> io::Result<()>;
    fn read_line(&mut self, buf: &mut String) -> io::Result<usize>;
}

#[cfg(not(tarpaulin_include))]
struct StdPromptEnv;

#[cfg(not(tarpaulin_include))]
impl PromptEnv for StdPromptEnv {
    fn stdin_is_terminal(&self) -> bool {
        io::stdin().is_terminal()
    }
    fn write_prompt(&mut self, text: &str) -> io::Result<()> {
        let mut out = io::stdout();
        write!(out, "{text}")?;
        out.flush()
    }
    fn read_line(&mut self, buf: &mut String) -> io::Result<usize> {
        io::stdin().lock().read_line(buf)
    }
}

fn prompt_install_default_with<E: PromptEnv>(
    env: &mut E,
    workflow_id: &str,
) -> Result<bool, app::AppError> {
    if !env.stdin_is_terminal() {
        return Ok(false);
    }
    env.write_prompt(&format!(
        "set '{workflow_id}' as the default workflow? [y/N]: "
    ))
    .map_err(|err| app::AppError::InvalidArgument(err.to_string()))?;
    let mut input = String::new();
    env.read_line(&mut input)
        .map_err(|err| app::AppError::InvalidArgument(err.to_string()))?;
    Ok(matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn prompt_install_default(workflow_id: &str) -> Result<bool, app::AppError> {
    let mut env = StdPromptEnv;
    prompt_install_default_with(&mut env, workflow_id)
}

fn parse_knot_type(raw: Option<&str>) -> Result<KnotType, app::AppError> {
    raw.unwrap_or("work")
        .parse::<KnotType>()
        .map_err(|err| app::AppError::InvalidArgument(err.to_string()))
}

#[cfg(not(tarpaulin_include))]
pub(crate) fn run_workflow_command(
    args: &cli::WorkflowArgs,
    repo_root: &std::path::Path,
) -> Result<(), app::AppError> {
    use cli::WorkflowSubcommands;

    match &args.command {
        WorkflowSubcommands::Install(install_args) => run_workflow_install(install_args, repo_root),
        WorkflowSubcommands::Use(use_args) => run_workflow_use(use_args, repo_root),
        WorkflowSubcommands::Current(current_args) => run_workflow_current(current_args, repo_root),
        WorkflowSubcommands::List(list_args) => run_workflow_list(list_args, repo_root),
        WorkflowSubcommands::Show(show_args) => run_workflow_show(show_args, repo_root),
    }
}

#[cfg(not(tarpaulin_include))]
fn run_workflow_install(
    install_args: &cli::WorkflowInstallArgs,
    repo_root: &std::path::Path,
) -> Result<(), app::AppError> {
    let knot_type = install_args
        .knot_type
        .parse::<KnotType>()
        .map_err(|err| app::AppError::InvalidArgument(err.to_string()))?;
    let workflow_id = installed_workflows::install_bundle(repo_root, &install_args.source)?;
    let config = installed_workflows::register_workflow_for_knot_type(
        repo_root,
        knot_type,
        &workflow_id,
        None,
        false,
    )?;
    let set_default = match install_args.set_default.as_deref() {
        Some(raw) => parse_bool_flag(raw)?,
        None => prompt_install_default(&workflow_id)?,
    };
    let config = if set_default {
        installed_workflows::set_current_workflow_selection_for_knot_type(
            repo_root,
            knot_type,
            &workflow_id,
            None,
            None,
        )?
    } else {
        config
    };
    if set_default {
        let profile = config
            .default_profile_id_for_workflow(&workflow_id)
            .unwrap_or_default();
        println!("installed workflow: {workflow_id} (default profile={profile})");
    } else {
        println!("installed workflow: {workflow_id}");
    }
    Ok(())
}

#[cfg(not(tarpaulin_include))]
fn run_workflow_use(
    use_args: &cli::WorkflowUseArgs,
    repo_root: &std::path::Path,
) -> Result<(), app::AppError> {
    let knot_type = parse_knot_type(use_args.knot_type.as_deref())?;
    let config = installed_workflows::set_current_workflow_selection_for_knot_type(
        repo_root,
        knot_type,
        &use_args.id,
        use_args.version,
        use_args.profile.as_deref(),
    )?;
    let workflow_id = config
        .current_workflow_ref_for_knot_type(knot_type)
        .map(|workflow| workflow.workflow_id)
        .unwrap_or_else(|| use_args.id.clone());
    let version = config
        .current_workflow_ref_for_knot_type(knot_type)
        .and_then(|workflow| workflow.version);
    if let Some(version) = version {
        if let Some(profile) = config.default_profile_id_for_workflow(&workflow_id) {
            let profile = profile.rsplit('/').next().unwrap_or(profile);
            println!("default workflow: {workflow_id} v{version} profile={profile}");
        } else {
            println!("default workflow: {workflow_id} v{version}");
        }
    } else {
        println!("default workflow: {workflow_id}");
    }
    Ok(())
}

#[cfg(not(tarpaulin_include))]
fn run_workflow_current(
    current_args: &cli::WorkflowCurrentArgs,
    repo_root: &std::path::Path,
) -> Result<(), app::AppError> {
    let knot_type = parse_knot_type(current_args.knot_type.as_deref())?;
    let registry = installed_workflows::InstalledWorkflowRegistry::load(repo_root)?;
    let workflow = registry.current_workflow_for_knot_type(knot_type)?;
    if current_args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "knot_type": knot_type.as_str(),
                "id": workflow.id,
                "version": workflow.version,
                "builtin": workflow.builtin,
                "bundle_default_profile": workflow.default_profile,
                "default_profile": registry
                    .default_profile_id_for_knot_type(knot_type)
                    .map(|profile| {
                        profile
                            .rsplit('/')
                            .next()
                            .unwrap_or(profile.as_str())
                            .to_string()
                    }),
            }))
            .expect("json serialization should work")
        );
    } else if let Some(profile) = registry.default_profile_id_for_knot_type(knot_type) {
        let profile = profile
            .rsplit('/')
            .next()
            .unwrap_or(profile.as_str())
            .to_string();
        println!(
            "{} v{} default_profile={profile}",
            workflow.id, workflow.version
        );
    } else {
        println!("{} v{}", workflow.id, workflow.version);
    }
    Ok(())
}

#[cfg(not(tarpaulin_include))]
fn run_workflow_list(
    list_args: &cli::WorkflowListArgs,
    repo_root: &std::path::Path,
) -> Result<(), app::AppError> {
    let knot_type = parse_knot_type(list_args.knot_type.as_deref())?;
    let registry = installed_workflows::InstalledWorkflowRegistry::load(repo_root)?;
    let current_id = registry
        .current_workflow_id_for_knot_type(knot_type)
        .to_string();
    let current_version = registry
        .current_workflow_ref_for_knot_type(knot_type)
        .version;
    let workflows = registry
        .registered_workflows_for_knot_type(knot_type)
        .into_iter()
        .map(|workflow| {
            serde_json::json!({
                "knot_type": knot_type.as_str(),
                "id": workflow.id,
                "version": workflow.version,
                "builtin": workflow.builtin,
                "bundle_default_profile": workflow.default_profile,
                "default_profile": registry
                    .default_profile_id_for_workflow(&workflow.id)
                    .map(|profile| {
                        profile
                            .rsplit('/')
                            .next()
                            .unwrap_or(profile.as_str())
                            .to_string()
                    }),
                "current": workflow.id == current_id
                    && Some(workflow.version) == current_version,
                "profiles":
                    workflow.profiles.keys().cloned().collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    if list_args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&workflows).expect("json serialization should work")
        );
    } else if workflows.is_empty() {
        println!("no workflows installed");
    } else {
        for item in workflows {
            print_workflow_list_item(&item);
        }
    }
    Ok(())
}

#[cfg(not(tarpaulin_include))]
fn print_workflow_list_item(item: &serde_json::Value) {
    let id = item["id"].as_str().unwrap_or_default();
    let version = item["version"].as_u64().unwrap_or_default();
    let suffix = if item["current"].as_bool() == Some(true) {
        " (current)"
    } else {
        ""
    };
    let default_profile = item["default_profile"].as_str().unwrap_or_default();
    if default_profile.is_empty() {
        println!("{id} v{version}{suffix}");
    } else {
        println!("{id} v{version}{suffix} default_profile={default_profile}");
    }
}

#[cfg(not(tarpaulin_include))]
fn run_workflow_show(
    show_args: &cli::WorkflowShowArgs,
    repo_root: &std::path::Path,
) -> Result<(), app::AppError> {
    let registry = installed_workflows::InstalledWorkflowRegistry::load(repo_root)?;
    let workflow = match show_args.version {
        Some(version) => registry.require_workflow_version(&show_args.id, version)?,
        None => registry.require_workflow(&show_args.id)?,
    };
    if show_args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(workflow).expect("json serialization should work")
        );
    } else {
        println!("workflow: {}", workflow.id);
        println!("version: {}", workflow.version);
        if let Some(description) = workflow.display_description() {
            println!("description: {description}");
        }
        if let Some(default_profile) = workflow.default_profile.as_deref() {
            println!("default_profile: {default_profile}");
        }
        println!("builtin: {}", workflow.builtin);
        println!("profiles:");
        for profile in workflow.profiles.keys() {
            println!("  - {profile}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_bool_flag, prompt_install_default_with, PromptEnv};
    use std::io;

    struct MockEnv {
        is_tty: bool,
        input: String,
        prompt_written: Option<String>,
    }

    impl MockEnv {
        fn tty(input: &str) -> Self {
            Self {
                is_tty: true,
                input: input.to_string(),
                prompt_written: None,
            }
        }

        fn no_tty() -> Self {
            Self {
                is_tty: false,
                input: String::new(),
                prompt_written: None,
            }
        }
    }

    impl PromptEnv for MockEnv {
        fn stdin_is_terminal(&self) -> bool {
            self.is_tty
        }
        fn write_prompt(&mut self, text: &str) -> io::Result<()> {
            self.prompt_written = Some(text.to_string());
            Ok(())
        }
        fn read_line(&mut self, buf: &mut String) -> io::Result<usize> {
            buf.push_str(&self.input);
            Ok(self.input.len())
        }
    }

    #[test]
    fn parse_bool_flag_accepts_supported_values() {
        for raw in ["yes", "YES", "true", "1"] {
            assert!(parse_bool_flag(raw).expect("truthy value should parse"));
        }
        for raw in ["no", "NO", "false", "0"] {
            assert!(!parse_bool_flag(raw).expect("falsy value should parse"));
        }
    }

    #[test]
    fn parse_bool_flag_rejects_invalid_values() {
        let err = parse_bool_flag("maybe").expect_err("invalid value should fail");
        assert!(err.to_string().contains("invalid boolean value"));
    }

    #[test]
    fn prompt_install_default_is_disabled_without_tty() {
        let mut env = MockEnv::no_tty();
        let result = prompt_install_default_with(&mut env, "custom_flow")
            .expect("non-interactive prompt should skip");
        assert!(!result);
        assert!(env.prompt_written.is_none(), "no prompt when not a tty");
    }

    #[test]
    fn prompt_install_default_accepts_yes_from_tty() {
        let mut env = MockEnv::tty("yes\n");
        assert!(
            prompt_install_default_with(&mut env, "custom_flow").expect("prompt should succeed")
        );
        assert!(env
            .prompt_written
            .as_deref()
            .unwrap()
            .contains("custom_flow"));
    }

    #[test]
    fn prompt_install_default_accepts_y_short_from_tty() {
        let mut env = MockEnv::tty("y\n");
        assert!(prompt_install_default_with(&mut env, "flow").expect("prompt should succeed"));
    }

    #[test]
    fn prompt_install_default_rejects_non_yes_from_tty() {
        let mut env = MockEnv::tty("no\n");
        assert!(!prompt_install_default_with(&mut env, "flow").expect("prompt should succeed"));
    }

    #[test]
    fn prompt_install_default_rejects_empty_from_tty() {
        let mut env = MockEnv::tty("\n");
        assert!(!prompt_install_default_with(&mut env, "flow").expect("prompt should succeed"));
    }

    #[test]
    fn prompt_install_default_is_case_insensitive() {
        let mut env = MockEnv::tty("YES\n");
        assert!(prompt_install_default_with(&mut env, "flow").expect("prompt should succeed"));
    }
}
