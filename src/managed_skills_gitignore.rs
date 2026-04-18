use std::fs;
use std::path::Path;

use crate::app::AppError;

const AGENTS_IGNORE_RULE: &str = "/.agents/*";
const AGENTS_SKILLS_RULE: &str = "!/.agents/skills/";
const AGENTS_SKILLS_CONTENT_RULE: &str = "!/.agents/skills/**";
const LEGACY_AGENTS_IGNORE_RULE: &str = "/.agents/";

pub(super) fn ensure_agents_skills_gitignore(repo_root: &Path) -> Result<(), AppError> {
    let path = repo_root.join(".gitignore");
    let contents = if path.exists() {
        fs::read_to_string(&path)?
    } else {
        String::new()
    };

    let mut lines = contents
        .lines()
        .filter(|line| {
            !matches!(
                line.trim(),
                AGENTS_IGNORE_RULE
                    | AGENTS_SKILLS_RULE
                    | AGENTS_SKILLS_CONTENT_RULE
                    | LEGACY_AGENTS_IGNORE_RULE
            )
        })
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    lines.push(AGENTS_IGNORE_RULE.to_string());
    lines.push(AGENTS_SKILLS_RULE.to_string());
    lines.push(AGENTS_SKILLS_CONTENT_RULE.to_string());

    let normalized = format!("{}\n", lines.join("\n"));
    if normalized != contents {
        fs::write(path, normalized)?;
    }
    Ok(())
}
