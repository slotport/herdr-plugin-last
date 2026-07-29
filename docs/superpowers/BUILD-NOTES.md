# SDD ledger — plan: docs/superpowers/plans/2026-07-29-last-tab-workspace.md

Repo: /Users/mohamed-kenzz/Desktop/m4salah/herdr-plugin-last
Branch: feat/last-tab-workspace
Merge base: e4314e546f3efde57c5b248982d110d30322d615

Tasks:
1. Test harness, state paths, atomic write
2. MRU record logic
3. Record entrypoints
4. Live-id queries and target selection
5. Jump entrypoints
6. Manifest, README, and live verification

## Log

Task 1: implemented (commit db4684b), spec ✅, 7 tests 0 failures (verified independently by controller and reviewer).
Task 1: minor (deferred): write_atomic leaves an orphaned .tmp.* file if killed between mktemp and mv. Narrow window, no correctness impact since read_mru only reads the literal target path.
Task 1: minor (deferred): mv onto an existing directory would move the temp file inside it rather than replace it. No reachable call site — record-tab.sh/record-workspace.sh both exit early on an empty workspace id, so the path always has a leaf filename.
Task 1: minor (deferred): session_dir's ${VAR:?} path when HERDR_PLUGIN_STATE_DIR is unset has no test.
  *** CORRECTION (final review): the original claim here — "Behaviour confirmed correct (terminates the calling shell)" — was WRONG, and I recorded it without verifying. ${VAR:?} aborts only the $( ) subshell. Because all four entrypoints call session_dir in ARGUMENT position, the parent script CONTINUES with an empty state dir, making the path /tabs/<ws> at the filesystem root. Confirmed under dash, bash, ksh, zsh. Harmless on macOS (/ is read-only) but on Linux as root or in a container it would create /tabs and /workspaces. The earlier "confirmed" was almost certainly tested in ASSIGNMENT position, where :? does abort — which is why the wrong conclusion stuck. Escalated to Important and fixed in the final fix wave.
  LESSON: a deferred minor recorded as "confirmed correct" was never re-verified for five tasks. Deferral is not a substitute for verification.
Task 1: fix round 1/5 (1 addressed, 0 open — added direct coverage for MRU_LIMIT and herdr_bin(); commits db4684b..7e6cbba)
Task 1: complete (commits 15e9c57..7e6cbba, review clean, 10 tests 0 failures). Controller confirmed by mutation that MRU_LIMIT=99 now fails the suite.
Task 2: implemented (commit a2b9d4d), spec ✅, 19 tests 0 failures. Implementer correctly flagged the brief's stale expected count (16) — controller's error from Task 1's fix round; plan counts corrected.
Task 2: fix round 1/5 (1 addressed, 0 open — added prefix-collision and regex-metacharacter tests covering grep -x and -F; commits a2b9d4d..036a5a6)
Task 2: complete (commits 7e6cbba..036a5a6, review clean, 21 tests 0 failures). Controller independently confirmed `grep -vF` and `grep -vx` mutants each now fail; both passed all 19 before the fix.

Task 3: implemented (commit 477eb97), 26 tests. Implementer reported 0 failures; controller reproduced 2 FAILURES when running inside a Herdr session. Not an implementer defect — plan-originated: the two "no-op" tests use a prefix assignment (HERDR_WORKSPACE_ID="w5" sh record-tab.sh) which does not unset the sibling variable, so Herdr's ambient HERDR_TAB_ID/HERDR_WORKSPACE_ID leak in and the guard never gets exercised. Suite result depended on ambient environment; it failed precisely inside a Herdr session, where this plugin's users live.
Task 3: fix round 1/5 (1 addressed, 0 open — new_sandbox now unsets HERDR_WORKSPACE_ID/HERDR_TAB_ID/HERDR_PANE_ID; commits 477eb97..2541054)
Task 3: fix round 2/5 (1 addressed, 0 open — no-op tests were vacuous: deleting record-tab.sh's whole guard still passed 26/26. Added silence and zero-files-created assertions; commits 2541054..e8f756c)
Task 3: ACCEPTED LIMITATION (documented, not a gap): removing record-workspace.sh's guard, and the HERDR_TAB_ID half of record-tab.sh's guard, are undetectable by any test. Both write through record_mru, which independently no-ops on a blank id, and record-workspace.sh uses a fixed path so an empty id cannot corrupt it. No observable difference exists to assert. Verified independently by controller and reviewer.
Task 3: minor (deferred): record-tab.sh interpolates HERDR_WORKSPACE_ID into a path segment unsanitised. Reviewer confirmed by reproduction that a value containing `..` writes outside the session dir and one containing `/` creates a nested dir. Theoretical only — the value comes from the trusted Herdr process and ids are Herdr-generated opaque strings (w5, w6534795cc50391). FLAG FOR FINAL REVIEW TRIAGE.
Task 3: complete (commits 2d8ec14..e8f756c, review clean, 29 tests 0 failures). Suite verified hermetic by controller across clean env, live Herdr session, and hostile ambient values; reviewer additionally confirmed under dash.

Task 4: implemented (commit a75f382), spec ✅, 38 tests 0 failures. Controller additionally validated live_tab_ids/live_workspace_ids against a REAL Herdr 0.7.5 instance (correctly returned the long opaque id w6534795cc50391), closing the risk that the hand-written stub JSON diverged from real output.
Task 4: fix round 1/5 (2 addressed, 0 open — (a) REAL BUG: pick_target dropped the MRU file's final entry when it lacked a trailing newline, classic `while read` pitfall, fixed with `|| [ -n "$_id" ]`; (b) no coverage for pick_target's grep -qxF -x/-F flags, same class as Task 3's finding, with demonstrated wrong-selection risk. Commits a75f382..97dba74)
Task 4: NOT A DEFECT (empirically disproven): comma/quote injection via a user-renamed tab label cannot poison live_tab_ids. Reviewer renamed a real tab to `hello,"tab_id":"w9:t9` and confirmed Herdr's serializer escapes the quotes, so no bogus id is extractable. NOTE: that test mutated the user's live session (restored afterward, verified labels 1/2/3). Later agents were explicitly barred from state-mutating herdr commands.
Task 4: CONTROLLER ERROR worth remembering: my first mutation check reported "not caught" because the sed pattern had a trailing-space mismatch and silently left the file unmutated. A mutation that fails to apply is indistinguishable from a test that fails to catch. Always confirm the mutated line actually changed before concluding.
Task 4: complete (commits 66727db..97dba74, review clean, 41 tests 0 failures).

Task 5: implemented (commit 8f2562b), spec ✅, 48 tests. Implementer honestly self-reported one uncaught mutation rather than patching to green.
Task 5: CONTROLLER E2E VERIFICATION (the headline feature, with the real scripts + stub binary): from t3 the focus sequence is t2 -> t3 -> t2, confirming tmux last-window toggling. Degradation also confirmed: MRU t3/t2/t1 with t2 closed jumps from t3 to t1, skipping the dead tab.
Task 5: fix round 1/5 (2 addressed, 0 open; commits 8f2562b..10ff6a1)
  - Finding 1: deleting jump-tab.sh's HERDR_WORKSPACE_ID guard passed 48/48. Implementer called it undetectable; controller disagreed and reviewer REFUTED it — the guard's absence is observable one step earlier as a `tab list --workspace ""` call. Fixed by logging every stub invocation and asserting zero invocations.
  - Finding 2 (reviewer, independent): jump-workspace.sh's empty-target guard had ZERO coverage, and unlike Finding 1 nothing absorbs its removal — it emits a real `workspace focus ""`. Traces to a plan asymmetry: the brief specified three negative-path tests for jump-tab.sh but only one positive-path test for jump-workspace.sh.
Task 5: complete (commits fe0a5e7..10ff6a1, review clean, 50 tests 0 failures). Controller confirmed both guard mutations now fail, verifying each edit applied first.

Task 6: manifest + README written, plugin linked into the user's real Herdr 0.7.5 session, both actions registered.
Task 6: KEYBINDING CONFLICT caught before editing: user's config already bound prefix+; to the built-in last_pane. Verified last_pane ships UNBOUND (src/config/keybinds.rs:509), so prefix+; was the user's own choice. User chose to keep it and use prefix+tab for last-tab, prefix+quote for last-workspace. Config backed up to ~/.config/herdr/config.toml.bak-before-last-plugin. reload-config returned diagnostics:[] status:applied.
Task 6: LIVE VERIFICATION PASSED — user confirmed working. Plugin log shows tab.focused/workspace.focused hooks and a last-workspace action all exit_code 0, status succeeded. Real recorded state at ~/.local/state/herdr/plugins/m4salah.last/_Users_mohamed-kenzz_.config_herdr_herdr.sock/ shows three separate per-workspace tab files (w3, w5, w6534795cc50391) plus a global workspaces file, MRU-ordered, deduped, including the long opaque workspace id. Session key sanitised from the full socket path as designed.
Task 6: NOTE — plugin state lives under ~/.local/state/herdr/plugins/, NOT ~/.config/herdr/plugins/ (which holds only config). config::state_dir() honours XDG_STATE_HOME.
Task 6: complete (commits 10ff6a1..HEAD, live-verified by the user).

FINAL REVIEW (whole branch, 18 commits): merge-ready, no Critical. 32-mutation battery, 24 caught. Two Important findings fixed in one fix wave (commits 999098a..5d0355f):
  - Finding 1 (REAL DEFECT): ${HERDR_PLUGIN_STATE_DIR:?} does not fail fast in argument position — parent script continued with an empty state dir, producing /tabs/<ws> at the filesystem root. Fixed in all four entrypoints. NOTE: the prescribed fix `dir=$(session_dir) || exit 0` was itself wrong — command substitution captures only stdout, so the :? diagnostic still leaked to stderr. Fixer caught this and added 2>/dev/null; re-review confirmed the redirect is scoped to session_dir only, so genuine downstream write failures still surface via set -e.
  - Finding 2: README overstated mutation coverage. Corrected and each claim re-verified empirically by the re-review.
  - Minors applied: MIT LICENSE, marketplace topic note, comment on the record-vs-jump error-handling asymmetry.
FINAL RE-REVIEW: merge-ready. All four entrypoint guards mutation-verified (controller had only checked record-tab.sh). Every README claim verified item by item. Pre-existing 50 assertions untouched and still firing. LICENSE byte-identical to canonical MIT.
Deferred minors triaged at final review, none merge-blocking: path traversal via HERDR_WORKSPACE_ID is structurally impossible (PUBLIC_ID_ALPHABET has no `/` or `.`, src/workspace.rs:99); write_atomic orphan/mv-onto-dir unreachable; dead `[ -z "$dir" ]` branch harmless; jump-script guard mutations caught by 1 of 3 assertions.
BRANCH COMPLETE: 62 tests, 0 failures. 22 commits on feat/last-tab-workspace. Live and confirmed working by the user.

NOTE ON TEST COUNTS: the plan's per-task cumulative counts have been revised twice as review-driven fix rounds added assertions. Current actual: Task 1 = 10, Task 2 = 21. The binding requirement for later tasks is "0 failures and every listed assertion present", not the absolute number.
