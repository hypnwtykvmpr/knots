use std::path::PathBuf;

use uuid::Uuid;

use super::apply_fixes_with_progress;
use crate::doctor::{DoctorCheck, DoctorStatus};
use crate::progress::{ProgressKind, ProgressReporter};

fn unique_workspace() -> PathBuf {
    let root = std::env::temp_dir().join(format!("knots-doctor-fix-progress-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&root).expect("workspace should be creatable");
    root
}

fn sample_check(name: &str, status: DoctorStatus) -> DoctorCheck {
    DoctorCheck {
        name: name.to_string(),
        status,
        detail: "detail".to_string(),
        data: None,
    }
}

#[derive(Default)]
struct CapturingReporter {
    events: Vec<(ProgressKind, String)>,
}

impl ProgressReporter for CapturingReporter {
    fn emit(&mut self, kind: ProgressKind, message: &str) -> std::io::Result<()> {
        self.events.push((kind, message.to_string()));
        Ok(())
    }
}

#[test]
fn apply_fixes_with_progress_emits_line_per_non_pass_check() {
    let root = unique_workspace();
    let checks = vec![
        sample_check("lock_health", DoctorStatus::Pass),
        sample_check("hooks", DoctorStatus::Warn),
        sample_check("remote", DoctorStatus::Fail),
        sample_check("unknown_check", DoctorStatus::Warn),
    ];

    let mut reporter = CapturingReporter::default();
    let mut dyn_reporter: Option<&mut dyn ProgressReporter> = Some(&mut reporter);
    apply_fixes_with_progress(&root, &checks, &mut dyn_reporter);

    let messages: Vec<&str> = reporter
        .events
        .iter()
        .map(|(_, msg)| msg.as_str())
        .collect();
    assert_eq!(
        messages,
        vec![
            "Fixing hooks... ok",
            "Fixing remote... ok",
            "Fixing unknown_check... skip",
        ],
        "every non-pass check should emit one ordered result line; pass checks should be skipped"
    );
    for (kind, _) in &reporter.events {
        assert_eq!(*kind, ProgressKind::Info);
    }

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn apply_fixes_with_progress_none_matches_silent_apply_fixes() {
    let root = unique_workspace();
    let checks = vec![sample_check("unknown_check", DoctorStatus::Warn)];
    let progress = apply_fixes_with_progress(&root, &checks, &mut None);
    assert!(!progress.outcome.event_log_touched);
    assert_eq!(progress.summary.fixed, 0);
    assert_eq!(progress.summary.skipped, 1);
    assert_eq!(progress.summary.failed, 0);
    let _ = std::fs::remove_dir_all(root);
}
