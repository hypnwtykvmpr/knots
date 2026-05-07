use std::fs;
use std::path::Path;

use crate::app::AppError;

const AGENTS_DIR: &str = ".agents";
const CLAUDE_DIR: &str = ".claude";

pub(super) fn ensure_agents_skills_gitignore(repo_root: &Path) -> Result<(), AppError> {
    ensure_managed_skills_gitignore(repo_root, AGENTS_DIR)
}

pub(super) fn ensure_claude_skills_gitignore(repo_root: &Path) -> Result<(), AppError> {
    ensure_managed_skills_gitignore(repo_root, CLAUDE_DIR)
}

pub(super) fn has_agents_skills_gitignore(repo_root: &Path) -> Result<bool, AppError> {
    has_managed_skills_gitignore(repo_root, AGENTS_DIR)
}

pub(super) fn has_claude_skills_gitignore(repo_root: &Path) -> Result<bool, AppError> {
    has_managed_skills_gitignore(repo_root, CLAUDE_DIR)
}

fn ensure_managed_skills_gitignore(repo_root: &Path, dir: &str) -> Result<(), AppError> {
    let path = repo_root.join(".gitignore");
    let contents = if path.exists() {
        fs::read_to_string(&path)?
    } else {
        String::new()
    };

    let rules = managed_skills_rules(dir);
    let legacy_rules = [
        format!("/{dir}/"),
        format!("/{dir}/*"),
        format!("!/{dir}/skills/"),
        format!("!/{dir}/skills/**"),
    ];
    let mut lines = contents
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !rules.iter().any(|rule| rule == trimmed)
                && !legacy_rules.iter().any(|rule| rule == trimmed)
        })
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    lines.extend(rules);

    let normalized = format!("{}\n", lines.join("\n"));
    if normalized != contents {
        fs::write(path, normalized)?;
    }
    Ok(())
}

fn has_managed_skills_gitignore(repo_root: &Path, dir: &str) -> Result<bool, AppError> {
    let path = repo_root.join(".gitignore");
    if !path.exists() {
        return Ok(false);
    }
    let contents = fs::read_to_string(path)?;
    let lines = contents
        .lines()
        .map(str::trim)
        .collect::<std::collections::HashSet<_>>();
    Ok(managed_skills_rules(dir)
        .iter()
        .all(|rule| lines.contains(rule.as_str())))
}

fn managed_skills_rules(dir: &str) -> Vec<String> {
    vec![
        format!("/{dir}/**"),
        format!("!/{dir}/"),
        format!("!/{dir}/skills/"),
        format!("!/{dir}/skills/**"),
    ]
}
