use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rmcp::model::Implementation;

use crate::runner::KnoRunner;

const SESSION_NICKNAME: &str = "mcp-session";

/// Tracks one lease per MCP session. The registry is shared across the
/// per-session server instances so shutdown can terminate every lease, but
/// entries are keyed by session, never by client identity: two concurrent
/// sessions of the same client app must not share a lease.
#[derive(Debug, Clone, Default)]
pub struct LeaseRegistry {
    inner: Arc<Mutex<LeaseState>>,
}

#[derive(Debug, Default)]
struct LeaseState {
    leases: HashMap<String, String>,
}

impl LeaseRegistry {
    /// Ensure the session holds a usable lease: revalidate a cached lease by
    /// extending it, and recreate it when it has expired or vanished. A
    /// cached lease id is never trusted without revalidation — an idle
    /// session would otherwise permanently lose the ability to claim.
    pub fn ensure_active(
        &self,
        runner: &KnoRunner,
        session_key: &str,
        client: &Implementation,
        timeout_seconds: u64,
    ) -> Result<String, String> {
        let cached = self.get(session_key);
        if let Some(id) = &cached {
            if extend_lease(runner, id, timeout_seconds).is_ok() {
                return Ok(id.clone());
            }
        }

        let lease = create_session_lease(runner, client, timeout_seconds)?;
        let mut state = self.inner.lock().expect("lease map poisoned");
        let current = state.leases.get(session_key).cloned();
        if let Some(existing) = current {
            // The map can hold the stale id we failed to extend (replace it)
            // or a fresh id a concurrent call installed (keep theirs and
            // release ours instead of orphaning it).
            if Some(&existing) != cached.as_ref() && existing != lease {
                drop(state);
                let _ = terminate_lease(runner, &lease);
                return Ok(existing);
            }
        }
        state.leases.insert(session_key.to_string(), lease.clone());
        Ok(lease)
    }

    pub fn get(&self, session_key: &str) -> Option<String> {
        self.inner
            .lock()
            .expect("lease map poisoned")
            .leases
            .get(session_key)
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

    /// Remove and return every tracked lease id.
    pub fn drain_all(&self) -> Vec<String> {
        let mut state = self.inner.lock().expect("lease map poisoned");
        state.leases.drain().map(|(_, id)| id).collect()
    }

    /// Terminate every tracked session lease so sync is not deferred for the
    /// remainder of the lease timeout after the server goes away.
    pub fn terminate_all(&self, runner: &KnoRunner) {
        for id in self.drain_all() {
            if let Err(err) = terminate_lease(runner, &id) {
                eprintln!("kno-mcp failed to terminate lease {id}: {err}");
            }
        }
    }

    /// Remove a single session's lease id from the registry.
    pub fn remove(&self, session_key: &str) -> Option<String> {
        self.inner
            .lock()
            .expect("lease map poisoned")
            .leases
            .remove(session_key)
    }

    #[cfg(test)]
    fn insert(&self, session_key: &str, lease_id: &str) {
        self.inner
            .lock()
            .expect("lease map poisoned")
            .leases
            .insert(session_key.to_string(), lease_id.to_string());
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.inner.lock().expect("lease map poisoned").leases.len()
    }
}

/// Terminates a session's lease when the per-session server instance goes
/// away (rmcp closes the MCP session or the transport ends). Every clone of
/// a session's server shares one guard through an `Arc`, so termination
/// happens exactly once, after in-flight tool calls finish. Clients that
/// vanish without closing their session still fall back to lease expiry —
/// the protocol cannot distinguish "gone" from "between requests".
#[derive(Debug)]
pub struct SessionLeaseGuard {
    registry: LeaseRegistry,
    runner: KnoRunner,
    session_key: String,
}

impl SessionLeaseGuard {
    pub fn new(registry: LeaseRegistry, runner: KnoRunner, session_key: String) -> Self {
        Self {
            registry,
            runner,
            session_key,
        }
    }
}

impl Drop for SessionLeaseGuard {
    fn drop(&mut self) {
        let Some(lease) = self.registry.remove(&self.session_key) else {
            return;
        };
        let runner = self.runner.clone();
        // Drop may run on an async worker; keep the blocking subprocess off
        // it and never stall session teardown.
        std::thread::spawn(move || {
            if let Err(err) = terminate_lease(&runner, &lease) {
                eprintln!("kno-mcp failed to terminate lease {lease}: {err}");
            }
        });
    }
}

fn extend_lease(runner: &KnoRunner, lease_id: &str, timeout_seconds: u64) -> Result<(), String> {
    let args = vec![
        "extend".to_string(),
        "--lease-id".to_string(),
        lease_id.to_string(),
        "--timeout-seconds".to_string(),
        timeout_seconds.to_string(),
    ];
    runner
        .run("lease", &args)
        .map(|_| ())
        .map_err(|err| err.stderr)
}

fn terminate_lease(runner: &KnoRunner, lease_id: &str) -> Result<(), String> {
    let args = vec!["terminate".to_string(), lease_id.to_string()];
    runner.run_raw("lease", &args).map_err(|err| err.stderr)
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
        assert_eq!(LeaseRegistry::default().get("s1"), None);
        assert_eq!(LeaseRegistry::default().single_lease(), None);
    }

    #[test]
    fn creates_then_revalidates_cached_session_lease() {
        let registry = LeaseRegistry::default();
        let runner = KnoRunner::new(fixture_kno(), PathBuf::from("/tmp/repo"));
        let mut client = Implementation::new("test-client", "1.2.3");
        client.title = Some("test-provider".to_string());

        let first = registry
            .ensure_active(&runner, "s1", &client, 30)
            .expect("lease should be created");
        let second = registry
            .ensure_active(&runner, "s1", &client, 30)
            .expect("cached lease should be revalidated");

        assert_eq!(first, "L1");
        assert_eq!(second, "L1");
        assert_eq!(registry.get("s1"), Some("L1".to_string()));
    }

    #[test]
    fn recreates_lease_when_extend_fails() {
        let registry = LeaseRegistry::default();
        let runner = KnoRunner::new(fixture_kno(), PathBuf::from("/tmp/repo"));
        let client = Implementation::new("test-client", "1.2.3");

        // Simulate an idle session whose lease expired: the stub rejects
        // extends for L-expired, forcing recreation.
        registry.insert("s1", "L-expired");
        let refreshed = registry
            .ensure_active(&runner, "s1", &client, 30)
            .expect("expired lease should be recreated");

        assert_eq!(refreshed, "L1");
        assert_eq!(registry.get("s1"), Some("L1".to_string()));
    }

    #[test]
    fn sessions_are_keyed_independently_even_for_identical_clients() {
        let registry = LeaseRegistry::default();
        let runner = KnoRunner::new(fixture_kno(), PathBuf::from("/tmp/repo"));
        let client = Implementation::new("test-client", "1.2.3");

        registry
            .ensure_active(&runner, "s1", &client, 30)
            .expect("first session lease");
        registry
            .ensure_active(&runner, "s2", &client, 30)
            .expect("second session lease");

        assert_eq!(registry.len(), 2, "one lease per session, not per client");
    }

    #[test]
    fn single_lease_is_only_returned_when_unambiguous() {
        let registry = LeaseRegistry::default();
        let runner = KnoRunner::new(fixture_kno(), PathBuf::from("/tmp/repo"));
        let first = Implementation::new("test-client", "1.2.3");
        let second = Implementation::new("other-client", "1.2.3");

        assert_eq!(registry.single_lease(), None);
        registry
            .ensure_active(&runner, "s1", &first, 30)
            .expect("first lease");
        assert_eq!(registry.single_lease(), Some("L1".to_string()));
        registry
            .ensure_active(&runner, "s2", &second, 30)
            .expect("second lease");
        assert_eq!(registry.single_lease(), None);
    }

    #[test]
    fn guard_drop_releases_the_session_lease_exactly_once() {
        let registry = LeaseRegistry::default();
        let runner = KnoRunner::new(fixture_kno(), PathBuf::from("/tmp/repo"));
        registry.insert("s1", "L1");

        let guard = SessionLeaseGuard::new(registry.clone(), runner.clone(), "s1".to_string());
        drop(guard);
        assert_eq!(registry.len(), 0, "drop should release the session lease");

        // A guard for an already-released session is a no-op.
        let guard = SessionLeaseGuard::new(registry.clone(), runner, "s1".to_string());
        drop(guard);
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn terminate_all_drains_the_registry() {
        let registry = LeaseRegistry::default();
        let runner = KnoRunner::new(fixture_kno(), PathBuf::from("/tmp/repo"));

        registry.insert("s1", "L1");
        registry.insert("s2", "L2");
        registry.terminate_all(&runner);

        assert_eq!(registry.len(), 0);
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
