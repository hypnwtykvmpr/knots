use std::ffi::OsString;
use std::process::Command;

pub(crate) const RELEASES_LATEST_URL: &str = "https://github.com/acartine/knots/releases/latest";
const RELEASES_LATEST_API_URL: &str = "https://api.github.com/repos/acartine/knots/releases/latest";

pub(crate) fn fetch_latest_tag(url: &str, timeout_secs: u32) -> Option<String> {
    api_url_for_latest_redirect(url)
        .and_then(|api_url| fetch_latest_tag_from_api(api_url, timeout_secs))
        .or_else(|| fetch_latest_tag_from_redirect(url, timeout_secs))
}

fn api_url_for_latest_redirect(url: &str) -> Option<&'static str> {
    (url == RELEASES_LATEST_URL).then_some(RELEASES_LATEST_API_URL)
}

fn fetch_latest_tag_from_api(url: &str, timeout_secs: u32) -> Option<String> {
    let output = curl_command()
        .args(["--max-time", &timeout_secs.to_string(), "-fsS", url])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let body = String::from_utf8_lossy(&output.stdout);
    parse_tag_name_json(&body)
}

fn fetch_latest_tag_from_redirect(url: &str, timeout_secs: u32) -> Option<String> {
    let output = curl_command()
        .args(["--max-time", &timeout_secs.to_string(), "-fsS", "-I", url])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let headers = String::from_utf8_lossy(&output.stdout);
    parse_location_tag(&headers)
}

fn curl_command() -> Command {
    let program = std::env::var_os("KNOTS_CURL_BIN").unwrap_or_else(|| OsString::from("curl"));
    crate::native_command::command_for_program(program)
}

pub(crate) fn latest_available_version(current: &str, tag: Option<String>) -> Option<String> {
    let tag = tag?;
    let latest = strip_v_prefix(&tag);
    matches!(is_outdated(current, latest), Some(true)).then(|| latest.to_string())
}

pub(crate) fn parse_location_tag(headers: &str) -> Option<String> {
    for line in headers.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("location:") {
            let url = line.split_once(':')?.1.trim();
            let tag = url.rsplit('/').next()?;
            if !tag.is_empty() {
                return Some(tag.to_string());
            }
        }
    }
    None
}

fn parse_tag_name_json(body: &str) -> Option<String> {
    let key_pos = body.find("\"tag_name\"")?;
    let after_key = &body[key_pos + "\"tag_name\"".len()..];
    let value = after_key
        .trim_start()
        .strip_prefix(':')?
        .trim_start()
        .strip_prefix('"')?;
    let end = value.find('"')?;
    let value = &value[..end];
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

pub(crate) fn strip_v_prefix(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

pub(crate) fn is_outdated(current: &str, latest: &str) -> Option<bool> {
    let cur: Vec<u64> = current
        .split('.')
        .map(|s| s.parse().ok())
        .collect::<Option<Vec<_>>>()?;
    let lat: Vec<u64> = latest
        .split('.')
        .map(|s| s.parse().ok())
        .collect::<Option<Vec<_>>>()?;
    if cur.len() != 3 || lat.len() != 3 {
        return None;
    }
    Some(cur < lat)
}

#[cfg(test)]
mod tests {
    use super::{
        api_url_for_latest_redirect, fetch_latest_tag, is_outdated, latest_available_version,
        parse_location_tag, parse_tag_name_json, strip_v_prefix, RELEASES_LATEST_URL,
    };

    #[test]
    fn fetch_latest_tag_returns_none_for_unreachable_url() {
        let result = fetch_latest_tag("http://127.0.0.1:1/nonexistent", 1);
        assert_eq!(result, None);
    }

    #[test]
    fn api_url_is_only_used_for_canonical_latest_redirect() {
        assert!(api_url_for_latest_redirect(RELEASES_LATEST_URL).is_some());
        assert!(api_url_for_latest_redirect("https://example.com/releases/latest").is_none());
    }

    #[test]
    fn latest_available_version_returns_newer_release() {
        assert_eq!(
            latest_available_version("0.1.0", Some("v0.2.0".to_string())),
            Some("0.2.0".to_string())
        );
    }

    #[test]
    fn latest_available_version_skips_equal_or_invalid_versions() {
        assert_eq!(
            latest_available_version("0.2.0", Some("v0.2.0".to_string())),
            None
        );
        assert_eq!(
            latest_available_version("0.2.0", Some("beta-1".to_string())),
            None
        );
        assert_eq!(latest_available_version("0.2.0", None), None);
    }

    #[test]
    fn parse_location_tag_extracts_tag_from_redirect() {
        let headers =
            "HTTP/2 302\r\nlocation: https://github.com/acartine/knots/releases/tag/v1.0.0\r\n";
        assert_eq!(parse_location_tag(headers), Some("v1.0.0".to_string()));
    }

    #[test]
    fn parse_location_tag_handles_mixed_case_header() {
        let headers = "HTTP/2 302\r\nLocation: https://example.com/releases/tag/v0.2.2\r\n";
        assert_eq!(parse_location_tag(headers), Some("v0.2.2".to_string()));
    }

    #[test]
    fn parse_location_tag_returns_none_when_missing() {
        assert_eq!(parse_location_tag("HTTP/2 200\r\n"), None);
        assert_eq!(parse_location_tag(""), None);
    }

    #[test]
    fn parse_tag_name_json_extracts_latest_tag() {
        let body = r#"{
          "url": "https://api.github.com/repos/acartine/knots/releases/1",
          "tag_name": "v1.2.3",
          "draft": false
        }"#;
        assert_eq!(parse_tag_name_json(body), Some("v1.2.3".to_string()));
        assert_eq!(
            parse_tag_name_json(r#"{"tag_name":"v1.2.4"}"#),
            Some("v1.2.4".to_string())
        );
    }

    #[test]
    fn parse_tag_name_json_returns_none_when_missing_or_empty() {
        assert_eq!(parse_tag_name_json("{}"), None);
        assert_eq!(parse_tag_name_json(r#"{ "tag_name": "" }"#), None);
    }

    #[test]
    fn strip_v_prefix_removes_leading_v() {
        assert_eq!(strip_v_prefix("v1.2.3"), "1.2.3");
        assert_eq!(strip_v_prefix("1.2.3"), "1.2.3");
        assert_eq!(strip_v_prefix("v0.0.1"), "0.0.1");
    }

    #[test]
    fn is_outdated_compares_semver_parts() {
        assert_eq!(is_outdated("0.2.2", "0.2.3"), Some(true));
        assert_eq!(is_outdated("0.2.2", "0.3.0"), Some(true));
        assert_eq!(is_outdated("0.2.2", "1.0.0"), Some(true));
        assert_eq!(is_outdated("0.2.2", "0.2.2"), Some(false));
        assert_eq!(is_outdated("0.2.3", "0.2.2"), Some(false));
        assert_eq!(is_outdated("1.0.0", "0.9.9"), Some(false));
    }

    #[test]
    fn is_outdated_returns_none_for_invalid_versions() {
        assert_eq!(is_outdated("abc", "0.2.2"), None);
        assert_eq!(is_outdated("0.2.2", "abc"), None);
        assert_eq!(is_outdated("0.2", "0.2.2"), None);
        assert_eq!(is_outdated("0.2.2.1", "0.2.2"), None);
    }
}
