use std::fs;
use std::io::Write;
use std::path::Path;

pub const MRU_LIMIT: usize = 8;

/// Reads an MRU file, most recent first. A missing or unreadable file is an
/// empty list, never an error — a jump with no history is a normal no-op.
///
/// Blank lines are skipped. `record` cannot produce one, so a blank line can
/// only arrive by hand-editing, and dropping it here keeps every consumer free
/// of blank-id guards.
pub fn read(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// Moves `id` to the front of the MRU file, deduplicating and capping the list.
///
/// A blank id is ignored so that a missing `HERDR_TAB_ID` cannot truncate the
/// history. Comparison is whole-value and literal, which is what the shell
/// version needed `grep -vxF`'s `-x` and `-F` flags to achieve.
pub fn record(path: &Path, id: &str) -> std::io::Result<()> {
    if id.is_empty() {
        return Ok(());
    }
    let mut entries = read(path);
    entries.retain(|entry| entry != id);
    entries.insert(0, id.to_string());
    entries.truncate(MRU_LIMIT);
    write_atomic(path, &entries)
}

/// The first MRU entry worth jumping to, or `None`.
///
/// Entries are filtered against the *live* current id rather than assuming the
/// head of the list is accurate, so a missed focus event cannot make the jump
/// land on the tab the user is already looking at.
///
/// Filtering against `live` is required rather than defensive: `herdr tab focus`
/// on a deleted tab exits 0, so a try-and-check-status fallback would silently
/// do nothing instead of moving on to the next candidate.
pub fn pick_target(path: &Path, current: &str, live: &[String]) -> Option<String> {
    read(path)
        .into_iter()
        .find(|id| id != current && live.iter().any(|candidate| candidate == id))
}

/// Replaces `path` with `entries` atomically, so a concurrent reader never
/// observes a half-written list. The temp file is created in the destination
/// directory so the rename stays on one filesystem.
fn write_atomic(path: &Path, entries: &[String]) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no parent")
    })?;
    fs::create_dir_all(dir)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    for entry in entries {
        writeln!(tmp, "{entry}")?;
    }
    tmp.persist(path).map_err(|err| err.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // Creating a tempdir consults TMPDIR, so this briefly takes the shared
    // ENV_LOCK too: record_creates_its_temp_file_in_the_destination_directory
    // mutates TMPDIR process-wide for its duration, and every other test's
    // sandbox() call would otherwise race that mutation (observed directly:
    // without this, concurrent sandbox() calls failed with ENOENT against the
    // deliberately-broken TMPDIR). Callers that already hold ENV_LOCK
    // themselves must call sandbox() before acquiring it, since this Mutex is
    // not reentrant.
    fn sandbox() -> tempfile::TempDir {
        let _env = crate::context::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        tempfile::tempdir().expect("create sandbox")
    }

    #[test]
    fn mru_limit_is_8() {
        assert_eq!(MRU_LIMIT, 8);
    }

    #[test]
    fn read_on_a_missing_file_is_empty() {
        let dir = sandbox();
        assert_eq!(read(&dir.path().join("nope")), Vec::<String>::new());
    }

    #[test]
    fn read_returns_file_contents_in_order() {
        let dir = sandbox();
        let path = dir.path().join("f");
        fs::write(&path, "a\nb\n").unwrap();
        assert_eq!(read(&path), vec!["a", "b"]);
    }

    // record() can never write a blank line, so one can only arrive by hand.
    // Dropping it here keeps record() and pick_target() free of blank guards.
    #[test]
    fn read_skips_blank_lines() {
        let dir = sandbox();
        let path = dir.path().join("f");
        fs::write(&path, "a\n\nb\n").unwrap();
        assert_eq!(read(&path), vec!["a", "b"]);
    }

    #[test]
    fn read_returns_a_final_line_with_no_trailing_newline() {
        let dir = sandbox();
        let path = dir.path().join("f");
        fs::write(&path, "a\nb").unwrap();
        assert_eq!(read(&path), vec!["a", "b"]);
    }

    #[test]
    fn record_creates_parent_directories() {
        let dir = sandbox();
        let path = dir.path().join("deep/nested/f");
        record(&path, "one").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "one\n");
    }

    #[test]
    fn record_leaves_no_temp_files_behind() {
        let dir = sandbox();
        let path = dir.path().join("f");
        record(&path, "one").unwrap();
        record(&path, "two").unwrap();
        let entries: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(entries.len(), 1);
    }

    // The temp file must be created in the DESTINATION directory so the rename
    // stays on one filesystem. Regressing to NamedTempFile::new() would still
    // pass every other test, because on macOS the sandbox and the system temp
    // dir happen to share a filesystem. Pointing TMPDIR somewhere nonexistent
    // makes the two cases distinguishable.
    #[test]
    fn record_creates_its_temp_file_in_the_destination_directory() {
        // sandbox() takes ENV_LOCK itself (see its comment), so it must run
        // before we take the lock here — std::sync::Mutex is not reentrant.
        let dir = sandbox();
        let _env = crate::context::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("TMPDIR").ok();
        std::env::set_var("TMPDIR", dir.path().join("definitely-not-here"));

        let path = dir.path().join("tabs/w5");
        let result = record(&path, "w5:t1");

        match saved {
            Some(value) => std::env::set_var("TMPDIR", value),
            None => std::env::remove_var("TMPDIR"),
        }

        assert!(result.is_ok(), "record should not depend on TMPDIR: {result:?}");
        assert_eq!(read(&path), vec!["w5:t1"]);
    }

    #[test]
    fn record_keeps_most_recent_first_order() {
        let dir = sandbox();
        let path = dir.path().join("tabs");
        for id in ["w5:t1", "w5:t2", "w5:t3"] {
            record(&path, id).unwrap();
        }
        assert_eq!(read(&path), vec!["w5:t3", "w5:t2", "w5:t1"]);
    }

    #[test]
    fn record_moves_a_repeat_to_the_front_without_duplicating() {
        let dir = sandbox();
        let path = dir.path().join("tabs");
        for id in ["w5:t1", "w5:t2", "w5:t3", "w5:t1"] {
            record(&path, id).unwrap();
        }
        assert_eq!(read(&path), vec!["w5:t1", "w5:t3", "w5:t2"]);
    }

    #[test]
    fn record_ignores_an_empty_id() {
        let dir = sandbox();
        let path = dir.path().join("tabs");
        record(&path, "w5:t1").unwrap();
        record(&path, "").unwrap();
        assert_eq!(read(&path), vec!["w5:t1"]);
    }

    #[test]
    fn record_writes_nothing_at_all_for_an_empty_id() {
        let dir = sandbox();
        let path = dir.path().join("tabs");
        record(&path, "").unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn record_caps_the_file_at_mru_limit() {
        let dir = sandbox();
        let path = dir.path().join("capped");
        for i in 1..=12 {
            record(&path, &format!("w5:t{i}")).unwrap();
        }
        assert_eq!(read(&path).len(), MRU_LIMIT);
    }

    #[test]
    fn record_drops_the_oldest_entries_when_capping() {
        let dir = sandbox();
        let path = dir.path().join("capped");
        for i in 1..=12 {
            record(&path, &format!("w5:t{i}")).unwrap();
        }
        assert_eq!(
            read(&path),
            vec![
                "w5:t12", "w5:t11", "w5:t10", "w5:t9", "w5:t8", "w5:t7", "w5:t6", "w5:t5"
            ]
        );
    }

    // Guards the shell's `grep -vxF -x` flag: a whole-line match is required, so
    // re-recording w5:t1 must not evict w5:t12.
    #[test]
    fn record_dedups_on_whole_lines_not_prefixes() {
        let dir = sandbox();
        let path = dir.path().join("prefix");
        for id in ["w5:t12", "w5:t1", "w5:t1"] {
            record(&path, id).unwrap();
        }
        assert_eq!(read(&path), vec!["w5:t1", "w5:t12"]);
    }

    // Guards the shell's `grep -vxF -F` flag: ids are matched literally, so the
    // `.` in w5:t1.2 must not match the X in w5:t1X2.
    #[test]
    fn record_dedups_literally_not_as_a_regex() {
        let dir = sandbox();
        let path = dir.path().join("metachar");
        for id in ["w5:t1X2", "w5:t1.2", "w5:t1.2"] {
            record(&path, id).unwrap();
        }
        assert_eq!(read(&path), vec!["w5:t1.2", "w5:t1X2"]);
    }

    #[test]
    fn record_keeps_separate_workspaces_separate() {
        let dir = sandbox();
        let w5 = dir.path().join("tabs/w5");
        let w9 = dir.path().join("tabs/w9");
        record(&w5, "w5:t1").unwrap();
        record(&w9, "w9:t1").unwrap();
        assert_eq!(read(&w5), vec!["w5:t1"]);
        assert_eq!(read(&w9), vec!["w9:t1"]);
    }

    // The shell suite proved two concurrent sessions keep separate histories.
    // Every session's socket is named herdr.sock, so only the full path
    // distinguishes them — this exercises the record path end to end through
    // the real session-key rule rather than trusting session_key's own test.
    #[test]
    fn record_keeps_concurrent_sessions_separate() {
        let dir = sandbox();
        let default = dir
            .path()
            .join(crate::context::session_key("/home/test/.config/herdr/herdr.sock"))
            .join("tabs/w5");
        let named = dir
            .path()
            .join(crate::context::session_key(
                "/home/test/.config/herdr/sessions/work/herdr.sock",
            ))
            .join("tabs/w5");
        record(&default, "w5:t1").unwrap();
        record(&named, "w5:t7").unwrap();
        assert_eq!(read(&default), vec!["w5:t1"]);
        assert_eq!(read(&named), vec!["w5:t7"]);
    }

    fn write_mru(dir: &tempfile::TempDir, contents: &str) -> std::path::PathBuf {
        let path = dir.path().join("tabs");
        fs::write(&path, contents).unwrap();
        path
    }

    fn live(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn pick_target_skips_the_current_id() {
        let dir = sandbox();
        let path = write_mru(&dir, "w5:t2\nw5:t1\n");
        assert_eq!(
            pick_target(&path, "w5:t2", &live(&["w5:t1", "w5:t2"])),
            Some("w5:t1".to_string())
        );
    }

    #[test]
    fn pick_target_skips_ids_that_no_longer_exist() {
        let dir = sandbox();
        let path = write_mru(&dir, "w5:t2\nw5:t9\nw5:t1\n");
        assert_eq!(
            pick_target(&path, "w5:t2", &live(&["w5:t1", "w5:t2"])),
            Some("w5:t1".to_string())
        );
    }

    #[test]
    fn pick_target_is_none_when_only_the_current_id_remains() {
        let dir = sandbox();
        let path = write_mru(&dir, "w5:t2\n");
        assert_eq!(pick_target(&path, "w5:t2", &live(&["w5:t1", "w5:t2"])), None);
    }

    #[test]
    fn pick_target_is_none_when_every_candidate_is_dead() {
        let dir = sandbox();
        let path = write_mru(&dir, "w5:t9\n");
        assert_eq!(pick_target(&path, "w5:t2", &live(&["w5:t1", "w5:t2"])), None);
    }

    #[test]
    fn pick_target_is_none_for_a_missing_file() {
        let dir = sandbox();
        let path = dir.path().join("absent");
        assert_eq!(pick_target(&path, "w5:t2", &live(&["w5:t1"])), None);
    }

    #[test]
    fn pick_target_is_none_for_an_empty_file() {
        let dir = sandbox();
        let path = write_mru(&dir, "");
        assert_eq!(pick_target(&path, "w5:t2", &live(&["w5:t1"])), None);
    }

    #[test]
    fn pick_target_works_when_the_current_id_is_unknown() {
        let dir = sandbox();
        let path = write_mru(&dir, "w5:t2\nw5:t1\n");
        assert_eq!(
            pick_target(&path, "", &live(&["w5:t1", "w5:t2"])),
            Some("w5:t2".to_string())
        );
    }

    // Guards the shell's `grep -qxF -x` flag: a dead id that is a literal prefix
    // of a live id must not be treated as live.
    #[test]
    fn pick_target_requires_an_exact_match_not_a_prefix() {
        let dir = sandbox();
        let path = write_mru(&dir, "w5:t1\n");
        assert_eq!(pick_target(&path, "w5:t9", &live(&["w5:t12"])), None);
    }

    // Guards the shell's `grep -qxF -F` flag: the `.` in w5:t1.2 must be matched
    // literally against the live set, not as a regex wildcard.
    #[test]
    fn pick_target_matches_live_ids_literally_not_as_a_regex() {
        let dir = sandbox();
        let path = write_mru(&dir, "w5:t1.2\n");
        assert_eq!(pick_target(&path, "w5:t9", &live(&["w5:t1X2"])), None);
    }

    // Regression guard for a real defect: the shell `while read` loop dropped
    // the final entry when the file had no trailing newline.
    #[test]
    fn pick_target_selects_a_final_entry_with_no_trailing_newline() {
        let dir = sandbox();
        let path = write_mru(&dir, "w5:t1");
        assert_eq!(
            pick_target(&path, "w9", &live(&["w5:t1"])),
            Some("w5:t1".to_string())
        );
    }

    #[test]
    fn pick_target_returns_the_first_survivor_not_the_last() {
        let dir = sandbox();
        let path = write_mru(&dir, "w5:t3\nw5:t2\nw5:t1\n");
        assert_eq!(
            pick_target(&path, "w5:t9", &live(&["w5:t1", "w5:t2", "w5:t3"])),
            Some("w5:t3".to_string())
        );
    }
}
