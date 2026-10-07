//! One SQLite database per folder, in its outside folder: its deployments, its
//! missions and every deployment's log. Log entries are only ever added; the
//! database itself refuses to change or delete one.

use std::path::Path;

use legion2_proto::{Entry, EntryKind, LogFilter, Mission, NewEntry, Deployment, Part};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::{entry_filter::entries_matching, mission_status::mission_standing};

pub struct Store {
    database: Connection,
}

pub struct Worktree {
    pub path: String,
    pub branch: String,
    /// The branch the mission started from, and where it stood then.
    pub base: String,
    pub base_commit: String,
    pub is_removed: bool,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS deployments (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    pipeline TEXT NOT NULL,
    started_ms INTEGER NOT NULL,
    closed_ms INTEGER
);
CREATE TABLE IF NOT EXISTS missions (
    number INTEGER PRIMARY KEY,
    deployment TEXT NOT NULL REFERENCES deployments(id),
    title TEXT NOT NULL,
    file TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    deployment TEXT NOT NULL REFERENCES deployments(id),
    at_ms INTEGER NOT NULL,
    mission INTEGER,
    from_position TEXT NOT NULL,
    to_position TEXT,
    kind TEXT NOT NULL,
    text TEXT NOT NULL,
    answers INTEGER
);
CREATE INDEX IF NOT EXISTS entries_by_deployment ON entries(deployment, id);
CREATE TRIGGER IF NOT EXISTS entries_never_change BEFORE UPDATE ON entries
    BEGIN SELECT RAISE(ABORT, 'deployment log entries never change'); END;
CREATE TRIGGER IF NOT EXISTS entries_never_removed BEFORE DELETE ON entries
    BEGIN SELECT RAISE(ABORT, 'deployment log entries are never removed'); END;
-- Which entries have been handed to a session. Not part of the log.
CREATE TABLE IF NOT EXISTS delivered_entries (entry INTEGER PRIMARY KEY);
-- Each mission's own checkout of the repo, on its own branch.
CREATE TABLE IF NOT EXISTS worktrees (
    mission INTEGER PRIMARY KEY REFERENCES missions(number),
    path TEXT NOT NULL,
    branch TEXT NOT NULL,
    base TEXT NOT NULL,
    base_commit TEXT NOT NULL,
    is_removed INTEGER NOT NULL DEFAULT 0
);
-- The parts a mission's step was split into, each in its own checkout.
CREATE TABLE IF NOT EXISTS parts (
    mission INTEGER NOT NULL REFERENCES missions(number),
    number INTEGER NOT NULL,
    brief TEXT NOT NULL,
    path TEXT NOT NULL,
    branch TEXT NOT NULL,
    is_merged INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (mission, number)
);
";

/// A part of a split mission, with where it's worked.
pub struct PartCheckout {
    pub part: Part,
    pub path: String,
    pub branch: String,
}

const ENTRY_COLUMNS: &str = "id, deployment, at_ms, mission, from_position, to_position, kind, text, answers";

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

fn database_error(error: rusqlite::Error) -> String {
    format!("database: {error}")
}

fn entry_from_row(row: &Row) -> rusqlite::Result<Entry> {
    let kind_name: String = row.get(6)?;
    let kind = EntryKind::parse(&kind_name).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, format!("unknown entry kind {kind_name}").into())
    })?;
    Ok(Entry {
        id: row.get(0)?,
        deployment: row.get(1)?,
        at_ms: row.get(2)?,
        mission: row.get(3)?,
        from: row.get(4)?,
        to: row.get(5)?,
        kind,
        text: row.get(7)?,
        answers: row.get(8)?,
    })
}

fn deployment_from_row(row: &Row) -> rusqlite::Result<Deployment> {
    Ok(Deployment {
        id: row.get(0)?,
        name: row.get(1)?,
        folder: String::new(),
        pipeline: row.get(2)?,
        started_ms: row.get(3)?,
        closed_ms: row.get(4)?,
    })
}

fn part_from_row(row: &Row) -> rusqlite::Result<PartCheckout> {
    Ok(PartCheckout {
        part: Part { mission: row.get(0)?, number: row.get(1)?, brief: row.get(2)?, is_merged: row.get::<_, i64>(5)? != 0 },
        path: row.get(3)?,
        branch: row.get(4)?,
    })
}

fn worktree_from_row(row: &Row) -> rusqlite::Result<Worktree> {
    Ok(Worktree {
        path: row.get(0)?,
        branch: row.get(1)?,
        base: row.get(2)?,
        base_commit: row.get(3)?,
        is_removed: row.get::<_, i64>(4)? != 0,
    })
}

impl Store {
    pub fn open(path: &Path) -> Result<Store, String> {
        let database = Connection::open(path).map_err(|error| format!("can't open {}: {error}", path.display()))?;
        database.execute_batch(SCHEMA).map_err(database_error)?;
        Ok(Store { database })
    }

    fn query_all<T>(&self, sql: &str, parameters: impl rusqlite::Params, read_row: fn(&Row) -> rusqlite::Result<T>) -> Result<Vec<T>, String> {
        let mut statement = self.database.prepare(sql).map_err(database_error)?;
        let rows = statement.query_map(parameters, read_row).map_err(database_error)?;
        rows.collect::<Result<Vec<T>, _>>().map_err(database_error)
    }

    fn execute(&self, sql: &str, parameters: impl rusqlite::Params) -> Result<(), String> {
        self.database.execute(sql, parameters).map(|_| ()).map_err(database_error)
    }

    pub fn deployments(&self) -> Result<Vec<Deployment>, String> {
        self.query_all("SELECT id, name, pipeline, started_ms, closed_ms FROM deployments ORDER BY started_ms", [], deployment_from_row)
    }

    pub fn add_deployment(&self, deployment: &Deployment) -> Result<(), String> {
        self.execute(
            "INSERT INTO deployments (id, name, pipeline, started_ms) VALUES (?1, ?2, ?3, ?4)",
            params![deployment.id, deployment.name, deployment.pipeline, deployment.started_ms],
        )
    }

    pub fn close_deployment(&self, deployment_id: &str, closed_ms: i64) -> Result<(), String> {
        self.execute("UPDATE deployments SET closed_ms = ?2 WHERE id = ?1", params![deployment_id, closed_ms])
    }

    pub fn next_mission_number(&self) -> Result<u32, String> {
        self.database
            .query_row("SELECT COALESCE(MAX(number), 0) + 1 FROM missions", [], |row| row.get(0))
            .map_err(database_error)
    }

    pub fn add_mission(&self, number: u32, deployment_id: &str, title: &str, file: &str) -> Result<(), String> {
        self.execute("INSERT INTO missions (number, deployment, title, file) VALUES (?1, ?2, ?3, ?4)", params![number, deployment_id, title, file])
    }

    /// A deployment's missions, each with where it stands from its entries.
    pub fn missions(&self, deployment_id: &str) -> Result<Vec<Mission>, String> {
        let deployment_entries = self.all_entries(deployment_id)?;
        let rows = self.query_all(
            "SELECT number, title, file FROM missions WHERE deployment = ?1 ORDER BY number",
            [deployment_id],
            |row| Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
        )?;
        Ok(rows
            .into_iter()
            .map(|(number, title, file)| {
                let mission_entries: Vec<Entry> = deployment_entries.iter().filter(|entry| entry.mission == Some(number)).cloned().collect();
                let standing = mission_standing(&mission_entries);
                Mission { number, deployment: deployment_id.to_string(), title, file, status: standing.status, holder: standing.holder }
            })
            .collect())
    }

    pub fn mission(&self, deployment_id: &str, number: u32) -> Result<Mission, String> {
        self.missions(deployment_id)?
            .into_iter()
            .find(|mission| mission.number == number)
            .ok_or_else(|| format!("this deployment has no mission {number}"))
    }

    pub fn add_entry(&self, deployment_id: &str, author: &str, entry: &NewEntry) -> Result<Entry, String> {
        self.execute(
            "INSERT INTO entries (deployment, at_ms, mission, from_position, to_position, kind, text, answers)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![deployment_id, now_ms(), entry.mission, author, entry.to, entry.kind.as_str(), entry.text, entry.answers],
        )?;
        let new_id = self.database.last_insert_rowid();
        self.database
            .query_row(&format!("SELECT {ENTRY_COLUMNS} FROM entries WHERE id = ?1"), [new_id], entry_from_row)
            .map_err(database_error)
    }

    pub fn entry(&self, deployment_id: &str, entry_id: i64) -> Result<Option<Entry>, String> {
        self.database
            .query_row(&format!("SELECT {ENTRY_COLUMNS} FROM entries WHERE id = ?1 AND deployment = ?2"), params![entry_id, deployment_id], entry_from_row)
            .optional()
            .map_err(database_error)
    }

    fn all_entries(&self, deployment_id: &str) -> Result<Vec<Entry>, String> {
        self.query_all(&format!("SELECT {ENTRY_COLUMNS} FROM entries WHERE deployment = ?1 ORDER BY id"), [deployment_id], entry_from_row)
    }

    pub fn entries(&self, deployment_id: &str, filter: &LogFilter) -> Result<Vec<Entry>, String> {
        Ok(entries_matching(self.all_entries(deployment_id)?, filter))
    }

    /// Entries addressed to a position that no session has been handed yet.
    pub fn undelivered_entries(&self, deployment_id: &str, position: &str) -> Result<Vec<Entry>, String> {
        let addressed = self.query_all(
            &format!(
                "SELECT {ENTRY_COLUMNS} FROM entries
                 WHERE deployment = ?1 AND to_position = ?2 AND id NOT IN (SELECT entry FROM delivered_entries) ORDER BY id"
            ),
            [deployment_id, position],
            entry_from_row,
        )?;
        Ok(addressed.into_iter().filter(|entry| entry.kind.is_delivered()).collect())
    }

    pub fn mark_delivered(&self, entry_ids: &[i64]) -> Result<(), String> {
        entry_ids
            .iter()
            .try_for_each(|entry_id| self.execute("INSERT OR IGNORE INTO delivered_entries (entry) VALUES (?1)", [entry_id]))
    }

    pub fn worktree(&self, mission: u32) -> Result<Option<Worktree>, String> {
        self.database
            .query_row(
                "SELECT path, branch, base, base_commit, is_removed FROM worktrees WHERE mission = ?1",
                [mission],
                worktree_from_row,
            )
            .optional()
            .map_err(database_error)
    }

    pub fn add_worktree(&self, mission: u32, worktree: &Worktree) -> Result<(), String> {
        self.execute(
            "INSERT INTO worktrees (mission, path, branch, base, base_commit) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![mission, worktree.path, worktree.branch, worktree.base, worktree.base_commit],
        )
    }

    pub fn mark_worktree_removed(&self, mission: u32) -> Result<(), String> {
        self.execute("UPDATE worktrees SET is_removed = 1 WHERE mission = ?1", [mission])
    }

    pub fn add_part(&self, checkout: &PartCheckout) -> Result<(), String> {
        let part = &checkout.part;
        self.execute(
            "INSERT INTO parts (mission, number, brief, path, branch) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![part.mission, part.number, part.brief, checkout.path, checkout.branch],
        )
    }

    /// A mission's parts, by number.
    pub fn parts(&self, mission: u32) -> Result<Vec<PartCheckout>, String> {
        let mut statement = self
            .database
            .prepare("SELECT mission, number, brief, path, branch, is_merged FROM parts WHERE mission = ?1 ORDER BY number")
            .map_err(database_error)?;
        let rows = statement.query_map([mission], part_from_row).map_err(database_error)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(database_error)
    }

    pub fn mark_part_merged(&self, mission: u32, number: u32) -> Result<(), String> {
        self.execute("UPDATE parts SET is_merged = 1 WHERE mission = ?1 AND number = ?2", [mission, number])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_are_kept_by_mission_and_marked_merged() {
        let store = store_with_deployment();
        store.add_mission(3, "r", "Pages", "/m/3.md").unwrap();
        let checkout = |number: u32| PartCheckout {
            part: Part { mission: 3, number, brief: format!("part {number}"), is_merged: false },
            path: format!("/wt/3-part-{number}"),
            branch: format!("mission/3--part-{number}"),
        };
        store.add_part(&checkout(2)).unwrap();
        store.add_part(&checkout(1)).unwrap();
        store.mark_part_merged(3, 2).unwrap();
        let parts: Vec<(u32, bool)> = store.parts(3).unwrap().into_iter().map(|checkout| (checkout.part.number, checkout.part.is_merged)).collect();
        assert_eq!(parts, [(1, false), (2, true)]);
    }

    fn store_with_deployment() -> Store {
        let store = Store::open(Path::new(":memory:")).unwrap();
        let deployment = Deployment { id: "r".into(), name: "feature".into(), folder: String::new(), pipeline: "feature".into(), started_ms: 1, closed_ms: None };
        store.add_deployment(&deployment).unwrap();
        store
    }

    fn note(text: &str) -> NewEntry {
        NewEntry { kind: EntryKind::Note, mission: None, to: None, text: text.into(), answers: None }
    }

    #[test]
    fn entries_come_back_in_order() {
        let store = store_with_deployment();
        store.add_entry("r", "builder", &note("one")).unwrap();
        store.add_entry("r", "builder", &note("two")).unwrap();
        let texts: Vec<String> = store.entries("r", &LogFilter::default()).unwrap().into_iter().map(|entry| entry.text).collect();
        assert_eq!(texts, vec!["one", "two"]);
    }

    #[test]
    fn entries_never_change_or_go() {
        let store = store_with_deployment();
        store.add_entry("r", "builder", &note("one")).unwrap();
        assert!(store.execute("UPDATE entries SET text = 'x'", []).is_err());
        assert!(store.execute("DELETE FROM entries", []).is_err());
    }

    #[test]
    fn delivery_happens_once() {
        let store = store_with_deployment();
        let message = NewEntry { kind: EntryKind::Message, to: Some("reviewer".into()), ..note("look") };
        let added = store.add_entry("r", "builder", &message).unwrap();
        store.add_entry("r", "builder", &NewEntry { to: Some("reviewer".into()), ..note("not delivered") }).unwrap();
        assert_eq!(store.undelivered_entries("r", "reviewer").unwrap().len(), 1);
        store.mark_delivered(&[added.id]).unwrap();
        assert!(store.undelivered_entries("r", "reviewer").unwrap().is_empty());
    }

    #[test]
    fn missions_are_numbered_and_tracked() {
        let store = store_with_deployment();
        assert_eq!(store.next_mission_number().unwrap(), 1);
        store.add_mission(1, "r", "Do it", "/m/0001-do-it.md").unwrap();
        assert_eq!(store.next_mission_number().unwrap(), 2);
        let started = NewEntry { kind: EntryKind::SessionStarted, mission: Some(1), ..note("go") };
        store.add_entry("r", "builder", &started).unwrap();
        let mission = store.mission("r", 1).unwrap();
        assert_eq!(mission.holder.as_deref(), Some("builder"));
        assert!(store.mission("r", 9).is_err());
    }

    #[test]
    fn closing_a_deployment_is_remembered() {
        let store = store_with_deployment();
        store.close_deployment("r", 42).unwrap();
        assert_eq!(store.deployments().unwrap()[0].closed_ms, Some(42));
    }
}
