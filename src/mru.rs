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

    fn sandbox() -> tempfile::TempDir {
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
}
