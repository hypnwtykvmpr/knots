use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::app::AppError;

use super::{command_status_failure, record_git_command};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RemoteBlob {
    object_id: String,
    path: PathBuf,
}

pub(super) fn parse_ls_tree_blobs(raw: &[u8]) -> Result<Vec<RemoteBlob>, AppError> {
    let mut blobs = Vec::new();
    for entry in raw
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let Some(tab) = entry.iter().position(|byte| *byte == b'\t') else {
            return Err(AppError::InvalidArgument(
                "git ls-tree returned an entry without a path separator".to_string(),
            ));
        };
        let metadata = std::str::from_utf8(&entry[..tab]).map_err(|err| {
            AppError::InvalidArgument(format!("git ls-tree returned non-UTF-8 metadata: {err}"))
        })?;
        let path = std::str::from_utf8(&entry[tab + 1..]).map_err(|err| {
            AppError::InvalidArgument(format!("git ls-tree returned a non-UTF-8 path: {err}"))
        })?;
        let path = PathBuf::from(path);
        if !is_knots_json_store_path(&path) {
            continue;
        }
        let mut fields = metadata.split_whitespace();
        let _mode = fields.next();
        let Some(object_type) = fields.next() else {
            return Err(AppError::InvalidArgument(
                "git ls-tree returned an entry without an object type".to_string(),
            ));
        };
        let Some(object_id) = fields.next() else {
            return Err(AppError::InvalidArgument(
                "git ls-tree returned an entry without an object id".to_string(),
            ));
        };
        if object_type != "blob" {
            continue;
        }
        blobs.push(RemoteBlob {
            object_id: object_id.to_string(),
            path,
        });
    }
    Ok(blobs)
}

fn is_knots_json_store_path(path: &Path) -> bool {
    if path.extension().is_none_or(|ext| ext != "json") {
        return false;
    }
    [".knots/index", ".knots/events", ".knots/snapshots"]
        .iter()
        .any(|prefix| path.starts_with(prefix))
}

pub(super) fn cat_file_blobs(
    repo_root: &Path,
    blobs: &[RemoteBlob],
) -> Result<Vec<(PathBuf, Vec<u8>)>, AppError> {
    if blobs.is_empty() {
        return Ok(Vec::new());
    }
    record_git_command();
    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["-c", "core.autocrlf=false", "cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(AppError::Io)?;

    let mut stdin = child.stdin.take().ok_or_else(|| {
        AppError::InvalidArgument("failed to open stdin for git cat-file".to_string())
    })?;
    let object_ids = blobs
        .iter()
        .map(|blob| blob.object_id.clone())
        .collect::<Vec<_>>();
    let writer = std::thread::spawn(move || -> std::io::Result<()> {
        for object_id in object_ids {
            stdin.write_all(object_id.as_bytes())?;
            stdin.write_all(b"\n")?;
        }
        Ok(())
    });

    let stdout = child.stdout.take().ok_or_else(|| {
        AppError::InvalidArgument("failed to open stdout for git cat-file".to_string())
    })?;
    let mut reader = BufReader::new(stdout);
    let mut results = Vec::with_capacity(blobs.len());
    for blob in blobs {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 {
            return Err(AppError::InvalidArgument(format!(
                "git cat-file ended before returning {}",
                blob.object_id
            )));
        }
        let size = parse_cat_file_header(&header, &blob.object_id)?;
        let mut contents = vec![0; size];
        reader.read_exact(&mut contents)?;
        let mut delimiter = [0_u8; 1];
        reader.read_exact(&mut delimiter)?;
        if delimiter[0] != b'\n' {
            return Err(AppError::InvalidArgument(format!(
                "git cat-file returned malformed blob output for {}",
                blob.object_id
            )));
        }
        results.push((blob.path.clone(), contents));
    }

    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_string(&mut stderr)?;
    }
    let status = child.wait().map_err(AppError::Io)?;
    if !status.success() {
        return Err(command_status_failure(
            repo_root,
            &["cat-file", "--batch"],
            status.code(),
            stderr,
        ));
    }
    writer
        .join()
        .map_err(|_| AppError::InvalidArgument("git cat-file writer thread panicked".to_string()))?
        .map_err(AppError::Io)?;

    Ok(results)
}

fn parse_cat_file_header(header: &str, expected_object_id: &str) -> Result<usize, AppError> {
    let mut fields = header.split_whitespace();
    let Some(object_id) = fields.next() else {
        return Err(AppError::InvalidArgument(
            "git cat-file returned an empty header".to_string(),
        ));
    };
    let Some(object_type) = fields.next() else {
        return Err(AppError::InvalidArgument(format!(
            "git cat-file returned an incomplete header for {expected_object_id}"
        )));
    };
    if object_id != expected_object_id || object_type != "blob" {
        return Err(AppError::InvalidArgument(format!(
            "git cat-file returned {object_id} {object_type} for requested blob {expected_object_id}"
        )));
    }
    let Some(size) = fields.next() else {
        return Err(AppError::InvalidArgument(format!(
            "git cat-file returned no size for {expected_object_id}"
        )));
    };
    size.parse::<usize>().map_err(|err| {
        AppError::InvalidArgument(format!(
            "git cat-file returned invalid size for {expected_object_id}: {err}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::{cat_file_blobs, parse_cat_file_header, parse_ls_tree_blobs};

    #[test]
    fn parse_ls_tree_blobs_filters_to_knots_json_store_paths() {
        let raw = concat!(
            "100644 blob 1111111111111111111111111111111111111111\t",
            ".knots/events/2026/06/12/a.json\0",
            "100644 blob 2222222222222222222222222222222222222222\t",
            ".knots/events/2026/06/12/a.txt\0",
            "100644 blob 3333333333333333333333333333333333333333\t",
            ".knots/_worktree/.knots/events/2026/06/12/stale.json\0",
            "040000 tree 4444444444444444444444444444444444444444\t",
            ".knots/events/tree.json\0",
            "040000 tree 5555555555555555555555555555555555555555\t",
            ".knots/events/2026\0"
        );

        let blobs = parse_ls_tree_blobs(raw.as_bytes()).expect("listing should parse");
        assert_eq!(blobs.len(), 1);
        assert_eq!(
            blobs[0].object_id,
            "1111111111111111111111111111111111111111"
        );
        assert_eq!(
            blobs[0].path.to_string_lossy(),
            ".knots/events/2026/06/12/a.json"
        );
    }

    #[test]
    fn parse_ls_tree_blobs_reports_malformed_entries() {
        let no_tab = parse_ls_tree_blobs(b"100644 blob abc");
        assert!(format!("{:?}", no_tab.unwrap_err()).contains("path separator"));

        let bad_metadata = parse_ls_tree_blobs(b"\xff\t.knots/events/a.json\0");
        assert!(format!("{:?}", bad_metadata.unwrap_err()).contains("non-UTF-8 metadata"));

        let bad_path = parse_ls_tree_blobs(b"100644 blob abc\t\xff\0");
        assert!(format!("{:?}", bad_path.unwrap_err()).contains("non-UTF-8 path"));

        let missing_type = parse_ls_tree_blobs(b"100644\t.knots/events/a.json\0");
        assert!(format!("{:?}", missing_type.unwrap_err()).contains("object type"));

        let missing_object = parse_ls_tree_blobs(b"100644 blob\t.knots/events/a.json\0");
        assert!(format!("{:?}", missing_object.unwrap_err()).contains("object id"));
    }

    #[test]
    fn parse_cat_file_header_validates_shape() {
        assert_eq!(
            parse_cat_file_header("abc blob 12\n", "abc").expect("header should parse"),
            12
        );
        assert!(parse_cat_file_header("", "abc").is_err());
        assert!(parse_cat_file_header("abc\n", "abc").is_err());
        assert!(parse_cat_file_header("def blob 12\n", "abc").is_err());
        assert!(parse_cat_file_header("abc tree 12\n", "abc").is_err());
        assert!(parse_cat_file_header("abc blob\n", "abc").is_err());
        assert!(parse_cat_file_header("abc blob nope\n", "abc").is_err());
    }

    #[test]
    fn cat_file_blobs_returns_empty_without_spawning_git() {
        let blobs = cat_file_blobs(std::path::Path::new("."), &[]).expect("empty is ok");
        assert!(blobs.is_empty());
    }
}
