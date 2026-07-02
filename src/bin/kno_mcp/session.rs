use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rmcp::model::Implementation;

use crate::runner::KnoRunner;

const SESSION_NICKNAME: &str = "mcp-session";

#[derive(Debug, Clone, Default)]
pub struct LeaseRegistry {
    inner: Arc<Mutex<LeaseState>>,
}

#[derive(Debug, Default)]
struct LeaseState {
    leases: HashMap<String, String>,
}

impl LeaseRegistry {
    pub fn get_or_create(
        &self,
        runner: &KnoRunner,
        client: &Implementation,
        timeout_seconds: u64,
    ) -> Result<String, String> {
        let key = client_key(client);
        let state = self.inner.lock().expect("lease map poisoned");
        if let Some(id) = state.leases.get(&key).cloned() {
            return Ok(id);
        }
        drop(state);

        let lease = create_session_lease(runner, client, timeout_seconds)?;
        let mut state = self.inner.lock().expect("lease map poisoned");
        state.leases.insert(key, lease.clone());
        Ok(lease)
    }

    pub fn get(&self, client: &Implementation) -> Option<String> {
        let key = client_key(client);
        self.inner
            .lock()
            .expect("lease map poisoned")
            .leases
            .get(&key)
            .cloned()
    }

    pub fn single_lease(&self) -> Option<String> {
        let state = self.inner.lock().expect("lease map poisoned");
        if state.leases.len() == 1 {
            state.leases.values().next().cloned()
        } else {
            None
        }
    }
}

fn client_key(client: &Implementation) -> String {
    format!(
        "{}\0{}\0{}",
        client.name,
        client.version,
        client.title.as_deref().unwrap_or_default()
    )
}

fn create_session_lease(
    runner: &KnoRunner,
    client: &Implementation,
    timeout_seconds: u64,
) -> Result<String, String> {
    let mut args = vec![
        "create".to_string(),
        "--nickname".to_string(),
        SESSION_NICKNAME.to_string(),
        "--type".to_string(),
        "agent".to_string(),
        "--agent-type".to_string(),
        "api".to_string(),
        "--agent-name".to_string(),
        client.name.clone(),
        "--model".to_string(),
        client.name.clone(),
        "--model-version".to_string(),
        client.version.clone(),
        "--timeout-seconds".to_string(),
        timeout_seconds.to_string(),
    ];
    if let Some(provider) = client.title.as_ref().filter(|title| !title.is_empty()) {
        args.push("--provider".to_string());
        args.push(provider.clone());
    }
    runner
        .run("lease", &args)
        .map_err(|err| err.stderr)
        .and_then(|value| {
            value
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| "lease create output did not include id".to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn get_is_empty_before_initialize() {
        let client = Implementation::new("test-client", "1.2.3");
        assert_eq!(LeaseRegistry::default().get(&client), None);
        assert_eq!(LeaseRegistry::default().single_lease(), None);
    }

    #[test]
    fn creates_and_caches_session_lease() {
        let registry = LeaseRegistry::default();
        let runner = KnoRunner::new(fixture_kno(), PathBuf::from("/tmp/repo"));
        let mut client = Implementation::new("test-client", "1.2.3");
        client.title = Some("test-provider".to_string());

        let first = registry
            .get_or_create(&runner, &client, 30)
            .expect("lease should be created");
        let second = registry
            .get_or_create(&runner, &client, 30)
            .expect("cached lease should be returned");

        assert_eq!(first, "L1");
        assert_eq!(second, "L1");
        assert_eq!(registry.get(&client), Some("L1".to_string()));
    }

    #[test]
    fn caches_distinct_leases_by_client() {
        let registry = LeaseRegistry::default();
        let runner = KnoRunner::new(fixture_kno(), PathBuf::from("/tmp/repo"));
        let first = Implementation::new("test-client", "1.2.3");
        let second = Implementation::new("other-client", "1.2.3");

        assert_eq!(registry.get_or_create(&runner, &first, 30), Ok("L1".into()));
        assert_eq!(
            registry.get_or_create(&runner, &second, 30),
            Ok("L2".into())
        );
        assert_eq!(registry.get(&second), Some("L2".to_string()));
        assert_eq!(registry.get_or_create(&runner, &first, 30), Ok("L1".into()));
        assert_eq!(registry.get(&first), Some("L1".to_string()));
        assert_eq!(registry.get(&second), Some("L2".to_string()));
    }

    #[test]
    fn single_lease_is_only_returned_when_unambiguous() {
        let registry = LeaseRegistry::default();
        let runner = KnoRunner::new(fixture_kno(), PathBuf::from("/tmp/repo"));
        let first = Implementation::new("test-client", "1.2.3");
        let second = Implementation::new("other-client", "1.2.3");

        assert_eq!(registry.single_lease(), None);
        registry
            .get_or_create(&runner, &first, 30)
            .expect("first lease");
        assert_eq!(registry.single_lease(), Some("L1".to_string()));
        registry
            .get_or_create(&runner, &second, 30)
            .expect("second lease");
        assert_eq!(registry.single_lease(), None);
    }

    fn fixture_kno() -> PathBuf {
        let fixture = if cfg!(windows) {
            "tests/fixtures/kno-stub.ps1"
        } else {
            "tests/fixtures/kno-stub.sh"
        };
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fixture)
    }
}
