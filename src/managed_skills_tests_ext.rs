use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use super::{
    doctor_check, fix_doctor_check, install_missing, managed_skills, render_skill, DoctorStatus,
    SkillTool,
};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn unique_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("{label}-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).expect("temp root should be creatable");
    root
}

fn init_git_repo(root: &PathBuf) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("init")
        .output()
        .expect("git init should run");
    assert!(
        output.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn check_ignore(root: &PathBuf, path: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["check-ignore", "--quiet", path])
        .status()
        .expect("git check-ignore should run")
        .success()
}

#[test]
fn managed_skill_inventory_includes_knots_create() {
    let names = managed_skills()
        .iter()
        .map(|skill| skill.deploy_name)
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "knots",
            "knots-e2e",
            "knots-create",
            "knots-plan-orchestrator"
        ]
    );
}

#[test]
fn knots_create_skill_describes_structured_creation_inputs() {
    let skill = managed_skills()
        .iter()
        .copied()
        .find(|skill| skill.deploy_name == "knots-create")
        .expect("knots-create should be managed");
    let rendered = render_skill(skill);

    assert!(rendered.contains("name: knots-create"));
    assert!(rendered.contains("Put the"));
    assert!(rendered.contains("goal, verification steps, and constraints in `-d`"));
    assert!(rendered.contains("Put only numbered"));
    assert!(rendered.contains("acceptance criteria in `--acceptance`"));
    assert!(rendered.contains("Goal:"));
    assert!(rendered.contains("Verification:"));
    assert!(rendered.contains("Constraints:"));
    assert!(rendered.contains("exact commands or"));
    assert!(rendered.contains("UI actions"));
    assert!(rendered.contains("API routes"));
    assert!(rendered.contains("file paths"));
    assert!(rendered.contains("kno new \"<title>\""));
    assert!(rendered.contains("--acceptance"));
}

#[test]
fn knots_plan_orchestrator_skill_describes_plan_execution_protocol() {
    let skill = managed_skills()
        .iter()
        .copied()
        .find(|skill| skill.deploy_name == "knots-plan-orchestrator")
        .expect("knots-plan-orchestrator should be managed");
    let rendered = render_skill(skill);

    assert!(rendered.contains("name: knots-plan-orchestrator"));
    assert!(rendered.contains("# Knots Plan Orchestrator"));
    assert!(rendered.contains("kno show <plan-id> --json"));
    assert!(rendered.contains("execution_plan"));
    assert!(rendered.contains("Waves are sequential"));
    assert!(rendered.contains("Steps within a wave are sequential"));
    assert!(rendered.contains("Knots within a step are concurrent"));
    assert!(rendered.contains("kno show <knot-id> --json"));
    assert!(rendered.contains("kno next <plan-id>"));
    assert!(rendered.contains("kno rollback <plan-id>"));
    assert!(rendered.contains("kno -C <path_to_repo>"));
    assert!(rendered.contains("SHIPPED"));
    assert!(rendered.contains("BLOCKED"));
    assert!(rendered.contains("DEFERRED"));
    assert!(rendered.contains("your own protocol for launching and managing coding"));
}

#[test]
fn opencode_install_bootstraps_agents_gitignore_and_cleans_legacy_locations() {
    let repo = unique_root("managed-skills-opencode-install");
    let home = unique_root("managed-skills-home");
    init_git_repo(&repo);
    fs::create_dir_all(repo.join(".opencode/skills/knots")).expect("legacy opencode project root");
    fs::write(repo.join(".opencode/skills/knots/SKILL.md"), "legacy").expect("legacy project");
    fs::create_dir_all(home.join(".config/opencode/skills/knots")).expect("legacy user root");
    fs::write(
        home.join(".config/opencode/skills/knots/SKILL.md"),
        "legacy",
    )
    .expect("legacy user");

    let output = install_missing(&repo, Some(&home), SkillTool::OpenCode).expect("install");
    assert!(output.contains(".agents/skills/knots/SKILL.md"));
    assert!(repo.join(".agents/skills/knots/SKILL.md").exists());
    assert!(!repo.join(".opencode/skills/knots/SKILL.md").exists());
    assert!(!home.join(".config/opencode/skills/knots/SKILL.md").exists());

    let gitignore = fs::read_to_string(repo.join(".gitignore")).expect("gitignore should exist");
    assert!(gitignore.lines().any(|line| line.trim() == "/.agents/*"));
    assert!(gitignore
        .lines()
        .any(|line| line.trim() == "!/.agents/skills/"));
    assert!(gitignore
        .lines()
        .any(|line| line.trim() == "!/.agents/skills/**"));
    fs::write(repo.join(".agents/private.txt"), "private").expect("private file");
    assert!(check_ignore(&repo, ".agents/private.txt"));
    assert!(!check_ignore(&repo, ".agents/skills/knots/SKILL.md"));
}

#[test]
fn claude_install_bootstraps_claude_gitignore_with_skills_allowlist() {
    let repo = unique_root("managed-skills-claude-gitignore");
    let home = unique_root("managed-skills-home");
    init_git_repo(&repo);
    fs::create_dir_all(repo.join(".claude")).expect("claude root");

    let output = install_missing(&repo, Some(&home), SkillTool::Claude).expect("install");
    assert!(output.contains(".claude/skills/knots/SKILL.md"));

    let gitignore = fs::read_to_string(repo.join(".gitignore")).expect("gitignore should exist");
    assert!(gitignore.lines().any(|line| line.trim() == "/.claude/*"));
    assert!(gitignore
        .lines()
        .any(|line| line.trim() == "!/.claude/skills/"));
    assert!(gitignore
        .lines()
        .any(|line| line.trim() == "!/.claude/skills/**"));
    fs::write(repo.join(".claude/settings.local.json"), "{}").expect("local settings");
    assert!(check_ignore(&repo, ".claude/settings.local.json"));
    assert!(!check_ignore(&repo, ".claude/skills/knots/SKILL.md"));
}

#[test]
fn doctor_skips_codex_and_opencode_when_agents_root_is_absent() {
    let _guard = env_lock().lock().expect("env lock");
    let repo = unique_root("managed-skills-doctor-skip");
    let home = unique_root("managed-skills-home");
    let prior_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home);
    fs::create_dir_all(home.join(".config/opencode/skills/knots")).expect("legacy user root");
    fs::write(
        home.join(".config/opencode/skills/knots/SKILL.md"),
        "legacy",
    )
    .expect("legacy");

    let codex = doctor_check(&repo, Some(&home), SkillTool::Codex);
    assert_eq!(codex.status, DoctorStatus::Pass);
    let opencode = doctor_check(&repo, Some(&home), SkillTool::OpenCode);
    assert_eq!(opencode.status, DoctorStatus::Pass);

    fix_doctor_check(&repo, "skills_opencode");
    assert!(!home.join(".config/opencode/skills/knots/SKILL.md").exists());
    assert!(!repo.join(".agents/skills/knots/SKILL.md").exists());

    match prior_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
}
