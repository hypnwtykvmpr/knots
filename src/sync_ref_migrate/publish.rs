use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::app::AppError;

use super::{
    command_failure, git_checked, git_output, record_git_command, remote_ref_exists, RefEndpoint,
};

pub(super) fn publish_target(
    repo_root: &Path,
    target: &RefEndpoint,
    files: &BTreeMap<PathBuf, Vec<u8>>,
) -> Result<Option<String>, AppError> {
    let remote_url = git_checked(repo_root, &["remote", "get-url", &target.remote])?;
    let work_root =
        std::env::temp_dir().join(format!("knots-sync-ref-migrate-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&work_root)?;
    let publish = work_root.join("publish");
    std::fs::create_dir_all(&publish)?;
    git_checked(&publish, &["init"])?;
    git_checked(&publish, &["remote", "add", "origin", &remote_url])?;

    let target_exists = remote_ref_exists(repo_root, target)?;
    let parent = if target_exists {
        git_checked(
            &publish,
            &[
                "fetch",
                "--no-tags",
                "--filter=blob:none",
                "origin",
                &target.refname,
            ],
        )?;
        Some(git_checked(&publish, &["rev-parse", "FETCH_HEAD"])?)
    } else {
        None
    };
    fast_import_knots_commit(&publish, parent.as_deref(), files)?;
    let commit = git_checked(&publish, &["rev-parse", "refs/heads/knots-migrate"])?;
    if target_exists && !target_has_changed(&publish)? {
        let _ = std::fs::remove_dir_all(work_root);
        return Ok(None);
    }

    git_checked(
        &publish,
        &[
            "push",
            "--no-verify",
            "origin",
            &format!("refs/heads/knots-migrate:{}", target.refname),
        ],
    )?;
    let _ = std::fs::remove_dir_all(work_root);
    Ok(Some(commit))
}

fn target_has_changed(repo_root: &Path) -> Result<bool, AppError> {
    let diff = git_output(
        repo_root,
        &[
            "diff",
            "--quiet",
            "FETCH_HEAD",
            "refs/heads/knots-migrate",
            "--",
            ".knots/index",
            ".knots/events",
            ".knots/snapshots",
        ],
    )?;
    match diff.status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => Err(command_failure(repo_root, &["diff", "--quiet"], diff)),
    }
}

fn fast_import_knots_commit(
    repo_root: &Path,
    parent: Option<&str>,
    files: &BTreeMap<PathBuf, Vec<u8>>,
) -> Result<(), AppError> {
    record_git_command();
    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["fast-import", "--quiet"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(AppError::Io)?;

    let mut stdin = child.stdin.take().ok_or_else(|| {
        AppError::InvalidArgument("failed to open stdin for git fast-import".to_string())
    })?;
    write_fast_import_stream(&mut stdin, parent, files)?;
    drop(stdin);

    let output = child.wait_with_output().map_err(AppError::Io)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_failure(
            repo_root,
            &["fast-import", "--quiet"],
            output,
        ))
    }
}

fn write_fast_import_stream<W: Write>(
    writer: &mut W,
    parent: Option<&str>,
    files: &BTreeMap<PathBuf, Vec<u8>>,
) -> Result<(), AppError> {
    writer.write_all(b"commit refs/heads/knots-migrate\n")?;
    writer.write_all(b"author Knots <knots@example.com> 0 +0000\n")?;
    writer.write_all(b"committer Knots <knots@example.com> 0 +0000\n")?;
    write_fast_import_data(writer, b"knots: migrate sync ref\n")?;
    if let Some(parent) = parent {
        writeln!(writer, "from {parent}")?;
        for dir in [".knots/index", ".knots/events", ".knots/snapshots"] {
            writeln!(writer, "D {}", quote_fast_import_path(dir))?;
        }
    }
    for (relative, contents) in files {
        writeln!(writer, "M 100644 inline {}", fast_import_path(relative)?)?;
        write_fast_import_data(writer, contents)?;
    }
    Ok(())
}

fn write_fast_import_data<W: Write>(writer: &mut W, data: &[u8]) -> Result<(), AppError> {
    writeln!(writer, "data {}", data.len())?;
    writer.write_all(data)?;
    writer.write_all(b"\n")?;
    Ok(())
}

fn fast_import_path(path: &Path) -> Result<String, AppError> {
    let path = path.to_str().ok_or_else(|| {
        AppError::InvalidArgument(format!("path is not UTF-8: {}", path.display()))
    })?;
    let path = path.replace('\\', "/");
    if path
        .bytes()
        .any(|byte| matches!(byte, b'\0' | b'\n' | b'\r'))
    {
        return Err(AppError::InvalidArgument(format!(
            "path is not valid for git fast-import: {path:?}"
        )));
    }
    Ok(quote_fast_import_path(&path))
}

fn quote_fast_import_path(path: &str) -> String {
    let mut quoted = String::with_capacity(path.len() + 2);
    quoted.push('"');
    for ch in path.chars() {
        match ch {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\t' => quoted.push_str("\\t"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use super::super::RefEndpoint;
    use super::{
        fast_import_path, publish_target, quote_fast_import_path, write_fast_import_stream,
    };

    #[test]
    fn quote_fast_import_path_escapes_special_characters() {
        assert_eq!(
            quote_fast_import_path(".knots/events/a.json"),
            "\".knots/events/a.json\""
        );
        assert_eq!(quote_fast_import_path("a\tb\"c\\d"), "\"a\\tb\\\"c\\\\d\"");
    }

    #[test]
    fn fast_import_path_validates_unsafe_paths() {
        assert_eq!(
            fast_import_path(Path::new(".knots/index/a.json")).expect("path should quote"),
            "\".knots/index/a.json\""
        );
        assert_eq!(
            fast_import_path(Path::new(".knots\\index\\a.json")).expect("path should normalize"),
            "\".knots/index/a.json\""
        );
        assert!(fast_import_path(Path::new(".knots/index/bad\nname.json")).is_err());
        assert!(fast_import_path(Path::new(".knots/index/bad\rname.json")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn fast_import_path_rejects_non_utf8_paths() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let path = PathBuf::from(OsString::from_vec(vec![0xff]));
        assert!(fast_import_path(&path).is_err());
    }

    #[test]
    fn write_fast_import_stream_renders_root_commit() {
        let mut files = BTreeMap::new();
        files.insert(
            std::path::PathBuf::from(".knots/events/2026/06/12/a.json"),
            b"{\"event_id\":\"a\"}\n".to_vec(),
        );
        let mut out = Vec::new();
        write_fast_import_stream(&mut out, None, &files).expect("stream should write");
        let rendered = String::from_utf8(out).expect("stream should be utf8 around JSON");

        assert!(rendered.contains("commit refs/heads/knots-migrate\n"));
        assert!(rendered.contains("data 24\nknots: migrate sync ref\n\n"));
        assert!(!rendered.contains("from "));
        assert!(rendered.contains("M 100644 inline \".knots/events/2026/06/12/a.json\"\n"));
        assert!(rendered.contains("data 17\n{\"event_id\":\"a\"}\n\n"));
    }

    #[test]
    fn write_fast_import_stream_renders_parent_and_deletes_store_dirs() {
        let files = BTreeMap::new();
        let mut out = Vec::new();
        write_fast_import_stream(&mut out, Some("abc123"), &files).expect("stream should write");
        let rendered = String::from_utf8(out).expect("stream should be utf8");

        assert!(rendered.contains("from abc123\n"));
        assert!(rendered.contains("D \".knots/index\"\n"));
        assert!(rendered.contains("D \".knots/events\"\n"));
        assert!(rendered.contains("D \".knots/snapshots\"\n"));
    }

    #[test]
    fn publish_target_returns_none_when_target_store_is_unchanged() {
        let (root, repo) = setup_repo();
        write_remote_knots_ref(
            &repo,
            "refs/work/knots",
            ".knots/index/2026/06/12/a.json",
            "{\"event_id\":\"a\"}\n",
        );
        let mut files = BTreeMap::new();
        files.insert(
            PathBuf::from(".knots/index/2026/06/12/a.json"),
            b"{\"event_id\":\"a\"}\n".to_vec(),
        );
        let target = RefEndpoint {
            remote: "origin".to_string(),
            refname: "refs/work/knots".to_string(),
        };

        let commit = publish_target(&repo, &target, &files).expect("unchanged publish should run");
        assert_eq!(commit, None);

        let _ = std::fs::remove_dir_all(root);
    }

    fn setup_repo() -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "knots-sync-ref-publish-test-{}",
            uuid::Uuid::now_v7()
        ));
        std::fs::create_dir_all(&root).expect("workspace should be creatable");
        let remote = root.join("origin.git");
        let repo = root.join("repo");
        run_git(&root, &["init", "--bare", path(&remote)]);
        std::fs::create_dir_all(&repo).expect("repo dir should be creatable");
        run_git(&repo, &["init"]);
        run_git(&repo, &["config", "user.email", "knots@example.com"]);
        run_git(&repo, &["config", "user.name", "Knots Test"]);
        std::fs::write(repo.join("README.md"), "# test\n").expect("readme should write");
        run_git(&repo, &["add", "README.md"]);
        run_git(&repo, &["commit", "-m", "init"]);
        run_git(&repo, &["branch", "-M", "main"]);
        run_git(&repo, &["remote", "add", "origin", path(&remote)]);
        run_git(&repo, &["push", "-u", "origin", "HEAD:refs/heads/main"]);
        (root, repo)
    }

    fn write_remote_knots_ref(repo: &Path, refname: &str, file: &str, contents: &str) {
        run_git(repo, &["checkout", "--orphan", "knots-target"]);
        let _ = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["rm", "-r", "--ignore-unmatch", "."])
            .output()
            .expect("git rm should run");
        let target = repo.join(file);
        std::fs::create_dir_all(target.parent().expect("file should have parent"))
            .expect("parent should be creatable");
        std::fs::write(&target, contents).expect("fixture file should write");
        run_git(repo, &["add", "-f", file]);
        run_git(repo, &["commit", "-m", "knots target"]);
        run_git(repo, &["push", "origin", &format!("HEAD:{refname}")]);
        run_git(repo, &["checkout", "main"]);
        run_git(repo, &["branch", "-D", "knots-target"]);
    }

    fn run_git(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .expect("git command should run");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn path(path: &Path) -> &str {
        path.to_str().expect("path should be utf8")
    }
}
