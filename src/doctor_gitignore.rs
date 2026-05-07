use std::path::Path;

use crate::doctor::{DoctorCheck, DoctorStatus};
use crate::project::DistributionMode;

const KNOTS_RULES: &[&str] = &[
    "/.knots",
    "/.knots/",
    "/.knots/*",
    ".knots",
    ".knots/",
    ".knots/*",
];

pub(crate) fn check_gitignore(repo_root: &Path, distribution: DistributionMode) -> DoctorCheck {
    if distribution != DistributionMode::Git {
        return DoctorCheck::simple(
            "gitignore",
            DoctorStatus::Pass,
            "local-only mode; .knots gitignore check skipped",
        );
    }

    match gitignore_has_knots_rule(repo_root) {
        Ok(true) => DoctorCheck::simple(
            "gitignore",
            DoctorStatus::Pass,
            ".gitignore excludes repo-local .knots store",
        ),
        Ok(false) => DoctorCheck::simple(
            "gitignore",
            DoctorStatus::Warn,
            ".gitignore is missing /.knots/",
        ),
        Err(err) => DoctorCheck::simple(
            "gitignore",
            DoctorStatus::Warn,
            format!("could not inspect .gitignore: {err}"),
        ),
    }
}

fn gitignore_has_knots_rule(repo_root: &Path) -> Result<bool, std::io::Error> {
    let path = repo_root.join(".gitignore");
    if !path.exists() {
        return Ok(false);
    }
    let contents = std::fs::read_to_string(path)?;
    Ok(contains_knots_ignore(&contents))
}

fn contains_knots_ignore(contents: &str) -> bool {
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .any(|line| KNOTS_RULES.contains(&line))
}
