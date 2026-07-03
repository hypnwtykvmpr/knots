use super::bundle_toml::{parse_bundle_toml, render_json_bundle_from_toml};
use super::tests_helpers::SAMPLE_BUNDLE;

fn assert_invalid_toml_bundle(raw: &str, needle: &str) {
    let err = parse_bundle_toml(raw).expect_err("bundle should be invalid");
    assert!(
        err.to_string().contains(needle),
        "expected {needle:?} in {err}"
    );
}

#[test]
fn toml_requires_non_empty_workflow_name() {
    let invalid = SAMPLE_BUNDLE.replace("name = \"custom_flow\"", "name = \"   \"");
    assert_invalid_toml_bundle(&invalid, "workflow.name is required");
}

#[test]
fn toml_reports_missing_action_state() {
    let invalid = SAMPLE_BUNDLE.replacen("action = \"work\"", "action = \"missing_work\"", 1);
    assert_invalid_toml_bundle(&invalid, "unknown action state 'missing_work'");
}

#[test]
fn toml_reports_missing_and_invalid_action_executors() {
    let missing = SAMPLE_BUNDLE.replacen("executor = \"agent\"\n", "", 1);
    assert_invalid_toml_bundle(&missing, "action 'work' is missing executor");

    let invalid = SAMPLE_BUNDLE.replacen("executor = \"agent\"", "executor = \"robot\"", 1);
    assert_invalid_toml_bundle(&invalid, "invalid executor 'robot'");
}

#[test]
fn toml_json_renderer_reports_invalid_toml() {
    let err = render_json_bundle_from_toml("not = [valid").expect_err("bad toml should fail");
    assert!(err.to_string().contains("invalid"));
}

#[test]
fn toml_json_renderer_preserves_outputs_and_prompt_params() {
    let with_params = SAMPLE_BUNDLE.replace(
        "[prompts.work]\n",
        "[profiles.autopilot.outputs.work]\n\
         artifact_type = \"branch\"\n\
         access_hint = \"git log\"\n\n\
         [prompts.work]\n",
    );
    let with_params = with_params.replace(
        "[prompts.work.success]\n",
        "[prompts.work.params.audience]\n\
         type = \"choice\"\n\
         values = [\"agent\", \"human\"]\n\
         required = true\n\
         default = \"agent\"\n\
         description = \"Who should receive the output\"\n\n\
         [prompts.work.success]\n",
    );

    let rendered = render_json_bundle_from_toml(&with_params).expect("json should render");
    let json: serde_json::Value =
        serde_json::from_str(&rendered).expect("rendered bundle should be json");
    let profile = json["profiles"]
        .as_array()
        .expect("profiles should be an array")
        .iter()
        .find(|profile| profile["id"] == "autopilot")
        .expect("autopilot profile should render");
    let outputs = &profile["outputs"]["work"];
    assert_eq!(outputs["artifact_type"], "branch");
    assert_eq!(outputs["access_hint"], "git log");
    let prompt = json["prompts"]
        .as_array()
        .expect("prompts should be an array")
        .iter()
        .find(|prompt| prompt["name"] == "work")
        .expect("work prompt should render");
    let params = prompt["params"]
        .as_array()
        .expect("prompt params should be an array");
    assert_eq!(params[0]["name"], "audience");
    assert_eq!(params[0]["type"], "choice");
    assert_eq!(params[0]["values"][0], "agent");
    assert_eq!(params[0]["required"], true);
    assert_eq!(params[0]["default"], "agent");
    assert_eq!(params[0]["description"], "Who should receive the output");
}
