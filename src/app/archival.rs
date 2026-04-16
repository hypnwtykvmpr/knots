//! Cold-tier archival sweep.
//!
//! The sweep runs inline on `kno ls` when the hot cache is over the high-water
//! mark. It moves terminal, stale knots from `knot_hot` to `cold_catalog` in
//! batches, pushing the hot count down toward `HOT_TARGET`. Recently terminated
//! knots (age <= 72h) are preserved in hot so users see them in `kno ls`.
//!
//! The per-knot profile is consulted for the terminal-state check so that
//! custom workflows that name their terminal states differently still sweep.

use std::time::Duration as StdDuration;

use rusqlite::{params, Connection};
use time::{Duration, OffsetDateTime};

use crate::db;
use crate::domain::knot_type::parse_knot_type;
use crate::locks::FileLock;
use crate::tiering::ARCHIVE_AGE_HOURS;
use crate::workflow_runtime;

use super::error::AppError;
use super::App;

/// Hot cache size above which the sweep will start moving eligible knots out.
pub const HOT_HIGH_WATER: usize = 110;

/// Post-sweep target for the hot cache. Sweep stops once the count reaches
/// this number (or runs out of eligible candidates).
pub const HOT_TARGET: usize = 100;

/// Summary of a sweep invocation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColdSweepReport {
    pub moved: Vec<ColdSweepMoved>,
}

impl ColdSweepReport {
    pub fn len(&self) -> usize {
        self.moved.len()
    }

    pub fn is_empty(&self) -> bool {
        self.moved.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColdSweepMoved {
    pub id: String,
    pub title: String,
    pub state: String,
    pub updated_at: String,
}

struct Candidate {
    id: String,
    title: String,
    state: String,
    updated_at: String,
    profile_id: String,
    knot_type: Option<String>,
}

impl App {
    /// Run the cold-tier sweep. Returns a report with the moved knots.
    /// When the hot cache has `<= HOT_HIGH_WATER` entries, returns an empty
    /// report without taking any locks.
    pub fn run_cold_sweep(&self) -> Result<ColdSweepReport, AppError> {
        let hot_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM knot_hot", [], |row| row.get(0))?;
        if hot_count <= HOT_HIGH_WATER as i64 {
            return Ok(ColdSweepReport::default());
        }
        let limit = (hot_count as usize).saturating_sub(HOT_TARGET);
        if limit == 0 {
            return Ok(ColdSweepReport::default());
        }
        self.run_cold_sweep_locked(limit)
    }

    fn run_cold_sweep_locked(&self, limit: usize) -> Result<ColdSweepReport, AppError> {
        let _repo_guard =
            FileLock::acquire(&self.repo_lock_path(), StdDuration::from_millis(5_000))?;
        let _cache_guard =
            FileLock::acquire(&self.cache_lock_path(), StdDuration::from_millis(5_000))?;
        let now = OffsetDateTime::now_utc();
        let cutoff = format_rfc3339(now - Duration::hours(ARCHIVE_AGE_HOURS));
        let candidates = select_candidates(&self.conn, &cutoff)?;
        let eligible = self.filter_terminal_candidates(candidates)?;
        let to_move: Vec<Candidate> = eligible.into_iter().take(limit).collect();
        crate::trace::measure("cold_sweep_move", || self.move_candidates(to_move))
    }

    fn filter_terminal_candidates(
        &self,
        candidates: Vec<Candidate>,
    ) -> Result<Vec<Candidate>, AppError> {
        let mut out = Vec::new();
        for cand in candidates {
            let kt = parse_knot_type(cand.knot_type.as_deref());
            let is_terminal = workflow_runtime::is_terminal_state(
                &self.profile_registry,
                &cand.profile_id,
                kt,
                &cand.state,
            )
            .unwrap_or(false);
            if is_terminal {
                out.push(cand);
            }
        }
        Ok(out)
    }

    fn move_candidates(&self, candidates: Vec<Candidate>) -> Result<ColdSweepReport, AppError> {
        let mut moved = Vec::with_capacity(candidates.len());
        for cand in candidates {
            db::upsert_cold_catalog(
                &self.conn,
                &cand.id,
                &cand.title,
                &cand.state,
                &cand.updated_at,
            )?;
            db::delete_knot_hot(&self.conn, &cand.id)?;
            if std::env::var("KNO_TRACE").is_ok() {
                eprintln!(
                    "[archival] moved knot_id={} state={} updated_at={}",
                    cand.id, cand.state, cand.updated_at
                );
            }
            moved.push(ColdSweepMoved {
                id: cand.id,
                title: cand.title,
                state: cand.state,
                updated_at: cand.updated_at,
            });
        }
        Ok(ColdSweepReport { moved })
    }
}

fn format_rfc3339(ts: OffsetDateTime) -> String {
    ts.format(&time::format_description::well_known::Rfc3339)
        .expect("RFC3339 formatting for UTC timestamp should never fail")
}

fn select_candidates(conn: &Connection, cutoff: &str) -> Result<Vec<Candidate>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, title, state, updated_at, profile_id, knot_type \
         FROM knot_hot \
         WHERE updated_at < ?1 \
         ORDER BY updated_at ASC, id ASC",
    )?;
    let mut rows = stmt.query(params![cutoff])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(Candidate {
            id: row.get(0)?,
            title: row.get(1)?,
            state: row.get(2)?,
            updated_at: row.get(3)?,
            profile_id: row.get(4)?,
            knot_type: row.get(5)?,
        });
    }
    Ok(out)
}

#[cfg(test)]
#[path = "archival_tests.rs"]
mod tests;
