use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListArgs {
    pub state: Option<String>,
    pub tag: Option<String>,
    #[serde(rename = "type")]
    pub knot_type: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IdArgs {
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateArgs {
    pub title: String,
    pub description: Option<String>,
    pub acceptance: Option<String>,
    #[serde(rename = "type")]
    pub knot_type: Option<String>,
    pub priority: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateArgs {
    pub id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub acceptance: Option<String>,
    pub priority: Option<i64>,
    pub state: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ClaimArgs {
    pub id: String,
    pub e2e: Option<bool>,
}

pub fn list_argv(args: ListArgs) -> Vec<String> {
    let mut argv = Vec::new();
    push_opt(&mut argv, "--state", args.state);
    push_opt(&mut argv, "--tag", args.tag);
    push_opt(&mut argv, "--type", args.knot_type);
    push_opt(&mut argv, "--limit", args.limit.map(|v| v.to_string()));
    push_opt(&mut argv, "--offset", args.offset.map(|v| v.to_string()));
    argv
}

pub fn create_argv(args: CreateArgs) -> Vec<String> {
    let mut argv = vec![args.title];
    push_opt(&mut argv, "--desc", args.description);
    push_opt(&mut argv, "--acceptance", args.acceptance);
    push_opt(&mut argv, "--type", args.knot_type);
    argv
}

pub fn update_argv(args: UpdateArgs) -> Vec<String> {
    let mut argv = vec![args.id];
    push_opt(&mut argv, "--title", args.title);
    push_opt(&mut argv, "--description", args.description);
    push_opt(&mut argv, "--acceptance", args.acceptance);
    push_opt(
        &mut argv,
        "--priority",
        args.priority.map(|v| v.to_string()),
    );
    push_opt(&mut argv, "--status", args.state);
    argv
}

pub fn claim_argv(args: ClaimArgs, lease_id: Option<String>) -> Vec<String> {
    let mut argv = vec![args.id];
    if let Some(id) = lease_id {
        argv.push("--lease".to_string());
        argv.push(id);
    }
    if args.e2e.unwrap_or(false) {
        argv.push("--e2e".to_string());
    }
    argv
}

pub fn id_argv(args: IdArgs) -> Vec<String> {
    vec![args.id]
}

fn push_opt(argv: &mut Vec<String>, flag: &str, value: Option<String>) {
    if let Some(value) = value {
        argv.push(flag.to_string());
        argv.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_maps_filters_to_cli_flags() {
        let argv = list_argv(ListArgs {
            state: Some("ready".to_string()),
            tag: Some("mcp".to_string()),
            knot_type: Some("work".to_string()),
            limit: Some(10),
            offset: None,
        });
        assert_eq!(
            argv,
            ["--state", "ready", "--tag", "mcp", "--type", "work", "--limit", "10"]
        );
    }

    #[test]
    fn claim_passes_lease_and_e2e_only_when_requested() {
        let argv = claim_argv(
            ClaimArgs {
                id: "k1".to_string(),
                e2e: Some(true),
            },
            Some("L1".to_string()),
        );
        assert_eq!(argv, ["k1", "--lease", "L1", "--e2e"]);
    }

    #[test]
    fn create_update_claim_and_id_map_optional_fields() {
        assert_eq!(
            create_argv(CreateArgs {
                title: "Title".to_string(),
                description: Some("Body".to_string()),
                acceptance: Some("Done".to_string()),
                knot_type: Some("gate".to_string()),
                priority: Some(5),
            }),
            [
                "Title",
                "--desc",
                "Body",
                "--acceptance",
                "Done",
                "--type",
                "gate"
            ]
        );

        assert_eq!(
            update_argv(UpdateArgs {
                id: "k1".to_string(),
                title: Some("New".to_string()),
                description: Some("Desc".to_string()),
                acceptance: Some("Accepted".to_string()),
                priority: Some(9),
                state: Some("ready".to_string()),
            }),
            [
                "k1",
                "--title",
                "New",
                "--description",
                "Desc",
                "--acceptance",
                "Accepted",
                "--priority",
                "9",
                "--status",
                "ready"
            ]
        );

        assert_eq!(
            claim_argv(
                ClaimArgs {
                    id: "k1".to_string(),
                    e2e: None,
                },
                None,
            ),
            ["k1"]
        );
        assert_eq!(
            id_argv(IdArgs {
                id: "k1".to_string()
            }),
            ["k1"]
        );
    }
}
