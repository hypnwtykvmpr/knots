use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::app::AppError;

use super::git::changed_tracked_paths;
use super::gitignore::{ensure_agents_skills_gitignore, ensure_claude_skills_gitignore};
use super::output::{
    display_path, format_changed_paths, format_commit_notice, format_existing_skills, skill_paths,
};
use super::state::inspect_location;
use super::{managed_skills, ManagedSkill, SkillLocation, SkillTool};

pub(super) fn install_missing(
    repo_root: &Path,
    home: Option<&Path>,
    tool: SkillTool,
) -> Result<String, AppError> {
    let destination = preferred_location(repo_root, home, tool)?;
    let missing = inspect_location(&destination).missing;
    if missing.is_empty() {
        finalize_install(repo_root, home, tool)?;
        return Ok(format_existing_skills(tool, &destination));
    }
    let changed = write_skills(&destination, &missing)?;
    finalize_install(repo_root, home, tool)?;
    Ok(format_changed_paths(tool, "installed", &changed))
}

pub(super) fn uninstall_managed(
    repo_root: &Path,
    home: Option<&Path>,
    tool: SkillTool,
) -> Result<String, AppError> {
    let locations = tool.locations(repo_root, home);
    let mut removed_paths = Vec::new();
    for location in locations {
        let installed = installed_skills(&location);
        if installed.is_empty() {
            continue;
        }
        removed_paths.extend(remove_skills(&location, &installed)?);
    }
    if removed_paths.is_empty() {
        return Err(AppError::InvalidArgument(format!(
            "{} has no installed managed skills to uninstall",
            tool.display_name()
        )));
    }
    Ok(format_changed_paths(tool, "removed", &removed_paths))
}

pub(super) fn update_managed(
    repo_root: &Path,
    home: Option<&Path>,
    interactive: bool,
    tool: SkillTool,
) -> Result<String, AppError> {
    let destination = preferred_location(repo_root, home, tool)?;
    let missing = inspect_location(&destination).missing;
    if !missing.is_empty() {
        if !interactive {
            return Err(AppError::InvalidArgument(format!(
                "{} missing managed skills at {}; run `kno skills install {}` or rerun \
                 interactively",
                tool.display_name(),
                display_path(&destination.skills_root),
                tool.slug()
            )));
        }
        let mut stderr = io::stderr();
        let mut stdin = io::stdin().lock();
        if !prompt_install_missing(&mut stderr, &mut stdin, tool, &destination, &missing)? {
            return Err(AppError::InvalidArgument(
                "managed skill update cancelled; no changes written".to_string(),
            ));
        }
        write_skills(&destination, &missing)?;
    }

    let installed = installed_skills(&destination);
    let updated_paths = write_skills(&destination, &installed)?;
    finalize_install(repo_root, home, tool)?;
    let mut output = format_changed_paths(tool, "updated", &updated_paths);
    let changed_paths = changed_tracked_paths(repo_root, &updated_paths);
    if !changed_paths.is_empty() {
        output.push_str("\n\n");
        output.push_str(&format_commit_notice(repo_root, &changed_paths));
    }
    Ok(output)
}

pub(super) fn prompt_install_missing<W: Write, R: BufRead>(
    writer: &mut W,
    reader: &mut R,
    tool: SkillTool,
    destination: &SkillLocation,
    missing: &[ManagedSkill],
) -> Result<bool, AppError> {
    writeln!(
        writer,
        "{} is missing managed skills at {}:",
        tool.display_name(),
        display_path(&destination.skills_root)
    )?;
    for path in skill_paths(destination, missing) {
        writeln!(writer, "  {}", display_path(&path))?;
    }
    write!(writer, "install missing skills before update? [y/N]: ")?;
    writer.flush()?;

    let mut input = String::new();
    reader.read_line(&mut input)?;
    Ok(matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

pub(super) fn preferred_location(
    repo_root: &Path,
    home: Option<&Path>,
    tool: SkillTool,
) -> Result<SkillLocation, AppError> {
    super::doctor_location(repo_root, home, tool).ok_or_else(|| {
        AppError::InvalidArgument(format!(
            "{} root not detected; create {} first",
            tool.display_name(),
            super::expected_root_hint(tool)
        ))
    })
}

#[cfg(test)]
pub(super) fn installed_locations(
    repo_root: &Path,
    home: Option<&Path>,
    tool: SkillTool,
) -> Vec<SkillLocation> {
    tool.locations(repo_root, home)
        .into_iter()
        .filter(|location| !installed_skills(location).is_empty())
        .collect()
}

pub(super) fn installed_skills(location: &SkillLocation) -> Vec<ManagedSkill> {
    managed_skills()
        .iter()
        .copied()
        .filter(|skill| skill_path(location, *skill).exists())
        .collect()
}

pub(super) fn write_skills(
    location: &SkillLocation,
    skills: &[ManagedSkill],
) -> Result<Vec<PathBuf>, AppError> {
    if skills.is_empty() {
        return Ok(Vec::new());
    }
    fs::create_dir_all(&location.skills_root)?;
    let mut written = Vec::with_capacity(skills.len());
    for skill in skills {
        let path = skill_path(location, *skill);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, render_skill(*skill))?;
        written.push(path);
    }
    Ok(written)
}

fn finalize_install(
    repo_root: &Path,
    home: Option<&Path>,
    tool: SkillTool,
) -> Result<(), AppError> {
    match tool {
        SkillTool::Codex | SkillTool::OpenCode => ensure_agents_skills_gitignore(repo_root)?,
        SkillTool::Claude => ensure_claude_skills_gitignore(repo_root)?,
    }
    cleanup_legacy_locations(repo_root, home, tool)?;
    Ok(())
}

pub(super) fn remove_skills(
    location: &SkillLocation,
    skills: &[ManagedSkill],
) -> Result<Vec<PathBuf>, AppError> {
    let mut removed = Vec::new();
    for skill in skills {
        let path = skill_path(location, *skill);
        if path.exists() {
            fs::remove_file(&path)?;
            removed.push(path.clone());
        }
        if let Some(dir) = path.parent() {
            remove_dir_if_empty(dir)?;
        }
    }
    remove_dir_if_empty(&location.skills_root)?;
    Ok(removed)
}

pub(super) fn cleanup_legacy_locations(
    repo_root: &Path,
    home: Option<&Path>,
    tool: SkillTool,
) -> Result<Vec<PathBuf>, AppError> {
    let mut removed = Vec::new();
    for location in tool.legacy_cleanup_locations(repo_root, home) {
        let installed = installed_skills(&location);
        if installed.is_empty() {
            continue;
        }
        removed.extend(remove_skills(&location, &installed)?);
    }
    Ok(removed)
}

pub(super) fn remove_dir_if_empty(path: &Path) -> Result<(), AppError> {
    let Ok(mut entries) = fs::read_dir(path) else {
        return Ok(());
    };
    if entries.next().is_none() {
        fs::remove_dir(path)?;
    }
    Ok(())
}

pub(super) fn render_skill(skill: ManagedSkill) -> String {
    skill.contents.to_string()
}

pub(super) fn skill_path(location: &SkillLocation, skill: ManagedSkill) -> PathBuf {
    location
        .skills_root
        .join(skill.deploy_name)
        .join("SKILL.md")
}
