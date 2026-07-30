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

---

# SDD ledger — plan: docs/superpowers/plans/2026-07-29-rust-port.md

Repo: /Users/mohamed-kenzz/Desktop/m4salah/herdr-plugin-last
Branch: feat/rust-port
Merge base: b8c5af9 (docs: preserve build notes and accepted limitations)

Replaced the four POSIX sh entrypoints (record-tab.sh, record-workspace.sh,
jump-tab.sh, jump-workspace.sh) and lib.sh, all recorded above, with one Rust
binary `last` (src/main.rs, context.rs, mru.rs, herdr.rs), built by
`cargo build --release` and invoked by the manifest's four commands as
`./target/release/last <subcommand>`. All five shell files are deleted.

Tasks:
1. Cargo scaffold and context.rs — commits a7a4097, d2764f1 (fix round)
2. mru.rs — read, atomic write, record — commit c04b7f3
3. mru.rs — pick_target — commit 5f76b81
4. herdr.rs — CLI client — commits fb45a6f, 90ba54f, e9a402a (fix rounds)
5. main.rs, manifest, retargeted test.sh — commits ba2b4b2, 33c09ca (fix
   round), eae8b82 (second fix round: `cd "$SANDBOX"` added to all four
   HERDR_PLUGIN_STATE_DIR guard blocks — see the S4/S4b correction below)
6. Documentation (this entry) — README + this section only; live verification
   handled separately, see below.

Counts at the end of Task 5, before the final review: `cargo test` 42 passed,
`./test.sh` 33 assertions (29 retargeted from the shell suite + 4 new, covering
the record/jump exit asymmetry).

FINAL COUNTS for the branch, after the final review's fix wave (commit
abb0703): `cargo test` **43** passed / 0 failed. `./test.sh` **37** assertions,
0 failures. `cargo clippy --all-targets -- -D warnings` clean. The wave added
one Rust test (the atomic write's temp file must be created in the destination
directory) and four shell assertions (silence on a successful jump and a
successful record, each paired with a "still did the thing" check so a binary
that silently did nothing could not pass).

Coverage went from the shell suite's 62 assertions to 80.

## PLAN-MANDATED FINDING, escalated to the user (Task 1)

Test `env_id_treats_empty_as_absent` (src/context.rs) called only the private
`non_empty` helper and never `env_id` itself, so `env_id`'s actual env-reading
contract — including the genuinely-unset branch — had zero coverage while the
test's name claimed otherwise. The plan mandated this test verbatim, but the
controller had authored that plan text, so the plan carried no independent
authority to justify leaving the gap.
USER RULING: add a real `env_id` test AND rename the `non_empty` one. Coverage
gap closed, not relabelled.
It mattered beyond bookkeeping: Task 3's mutation battery (M8, env_id drops
the unset-or-empty rule) is CAUGHT only because of the test this ruling added.
Before the fix, no test called `env_id` at all, so nothing could have detected
a regression in it.
LESSON: a test name asserting coverage of a function is not evidence the
function is actually invoked. Read what a test calls, not what it's named.

## Coverage gaps controller mutation testing found that review missed

- GAP (Task 4, mutation M12): `herdr_bin()`'s fallback `"herdr" -> ""` went
  uncaught. Traced to a PLAN defect, not an implementer one — the plan
  assigned the shell suite's two herdr_bin assertions to "Task 1, Task 4" but
  neither task's concrete test list actually contained either one, silently
  dropping them despite the plan's "no case is dropped" claim. Low real-world
  risk (Herdr always injects HERDR_BIN_PATH; the fallback only keeps the
  binary runnable by hand outside a session) but cheap to close. Fixed in
  Task 4 fix round 1 with a dedicated test covering set / empty / unset.
- GAP (Task 5, mutation S6): `report()` swallowing I/O errors would collapse
  the record/jump exit-code asymmetry — the spec's most important behaviour to
  port correctly — and nothing in the suite caught it. Controller verified a
  deterministic trigger: make `$(session_dir)/tabs` a regular file so
  `create_dir_all` fails with EEXIST. Measured directly: record-tab exits 1
  with `herdr-plugin-last: File exists (os error 17)` on stderr; jump-tab
  exits 0 silently on the identical failure. The shell version had this same
  untested gap, so closing it is a coverage improvement over the original, not
  a regression. Fixed in Task 5 fix round 1 (four new shell assertions); the
  reviewer additionally confirmed the asymmetry is enforced at the type level
  (record_* returns `io::Result`, jump_* returns `()` with no way to
  propagate failure even deliberately), not just by convention.
- *** CORRECTION: RULING OVERTURNED (final review) ***. This entry originally
  read: "NON-GAP, ruled (Task 5, mutation S4): `record_*` replacing its
  state-dir guard with a fallback `Context` also went uncaught, but is not a
  real gap. The mutation had to invent a fallback Context — the accidental
  form (forgetting the guard) cannot even be written, because `let ... else`
  requires a diverging else branch. [...] Flagged to final review rather than
  looped on."
  THAT RULING WAS WRONG, and was disproven during the final review's own
  execution, not merely argued against. The guard does not live only at the
  call sites in `main.rs`, where `let ... else` genuinely does force a
  diverging branch — it also lives inside `Context::from_env`, where
  replacing the `?` with `.unwrap_or_default()` is an ordinary refactor, not
  a contrivance:

      -  let state_dir = env_id("HERDR_PLUGIN_STATE_DIR")?;
      +  let state_dir = env_id("HERDR_PLUGIN_STATE_DIR").unwrap_or_default();

  Mutation S4b, measured: NOT CAUGHT. All 33 assertions still passed, and the
  binary wrote a stray `_home_test_.config_herdr_herdr.sock/` tree into the
  process's current working directory. In production that CWD is
  `plugin_root`, so an unfixed regression here would litter the plugin's own
  install directory on every focus event — a real, production-visible defect
  that the original ruling declared unwritable.
  WHY THE ORIGINAL RULING MISSED IT: it generalised from "the call sites in
  main.rs are protected by let-else" to "the guard is structurally enforced,"
  without trying the accidental form one layer deeper, inside `from_env`
  itself — the more natural place someone would actually "simplify" the `?`.
  FIX (commit eae8b82): all four `HERDR_PLUGIN_STATE_DIR` guard blocks in
  `test.sh` now run the binary with `cd "$SANDBOX"` first, so a relative-path
  write lands where the existing "creates no file" assertions already look
  instead of escaping them. This strengthens the 4 existing blocks rather
  than adding new ones — the assertion count is unchanged at 33. Reverting
  the fix reproduces exactly 2 failures (the record-side "creates no file"
  assertions; the jump-side pair stays green because the jump paths never
  write), confirming the fix closes precisely the gap S4b found.
  LESSON: this is the same failure mode this file's own Task 1 LESSON already
  named, and it recurred in the very branch that LESSON was written on. Ruling
  a mutation "contrived" because you could only think of a contrived version
  of it is not verification. A deferred or adjudicated finding stays unverified
  until the specific accidental form has actually been tried.

## Controller errors this ledger records

- Stale test counts: the plan's per-task cumulative counts went stale
  repeatedly as review-driven fix rounds added assertions — Task 3 caught the
  plan claiming "32 tests total" against an actual 34. Counts were revised
  more than once across the branch (see the plan's own NOTE ON TEST COUNTS).
  Absolute numbers were never the binding requirement; "0 failures and every
  listed assertion present" was.
- 42-vs-41 arithmetic slip (Task 4): a fix-round instruction told the
  implementer to expect 42 tests; 40 + 1 = 41 (the bundled `"result": null`
  assertion landed inside an existing test and added nothing). The
  implementer reported the true 41 rather than matching the wrong number, and
  the re-reviewer independently confirmed it from the diff.
- Shallow M11 conclusion, corrected by the reviewer (Task 4).
  CORRECTION: the controller had recorded that mutating `Envelope::result`
  from `Option<T>` to plain `T` "does not compile", implying the Option was
  type-enforced. The reviewer's analysis was deeper and correct: with plain
  `T`, a missing `result` key just fails deserialization, `.ok()` maps that to
  `None`, and the parser still returns an empty vec — behaviour is IDENTICAL,
  so no existing test could distinguish the two designs. The mutation had only
  failed to compile because the controller didn't also adjust the
  `.and_then()` call site; the Option was justified by design reasoning alone,
  not by any discriminating test. Fixed by adding an explicit
  `"result": null` fixture in the same fix round, which does discriminate.
- Doc count typo (Task 5): the controller wrote "41 as executed" in a doc
  where every other figure said 42. Caught by the reviewer and fixed.

## Deferred minors (not merge-blocking; for a future reader to triage)

- RESOLVED (final fix wave, src/mru.rs): write_atomic's temp file is created
  via `NamedTempFile::new_in(dir)`, but until now no test asserted it lands
  specifically inside the destination directory rather than system tmp
  (Task 2). On macOS both currently share a filesystem, so a regression to
  `NamedTempFile::new()` would still have passed every prior test.
  `record_creates_its_temp_file_in_the_destination_directory` now points
  `TMPDIR` at a nonexistent path for the duration of one `record()` call,
  which only `NamedTempFile::new()` would consult and fail on. Discovered
  while adding it: `TMPDIR` is process-global, so this races every other
  test's `sandbox()` helper (which creates its tempdir via `TMPDIR` too) if
  run concurrently — not just tests that call `set_var`/`remove_var`
  directly, which is what `ENV_LOCK` was originally scoped to. Fixed by
  having `sandbox()` itself take `ENV_LOCK` briefly.
- write_atomic's "path has no parent" branch (reachable via
  `record(Path::new("/"), id)`) is untested (Task 2).
- *** CORRECTION ***: this entry originally read: "`pick_target`'s correctness
  for `current == ""` depends on `read()` filtering blank lines — an implicit,
  documented-but-not-compiler-checked cross-function contract (Task 3)." That
  had it backwards. `pick_target` is safe either way, independent of whether
  `read()` filters blank lines: with `current == ""` a blank id is dropped by
  the `id != current` comparison; with `current != ""` a blank id is dropped
  by live-set membership, since `live` comes from Herdr and never contains
  `""`. `read()`'s blank-line filter earns its keep in `record`, not in
  `pick_target` — a blank line there would otherwise consume an MRU slot
  (Task 3).
- src/mru.rs is ~360 lines holding read/record/write_atomic/pick_target plus
  28 colocated tests. Plan-mandated placement, not an implementer choice;
  worth watching if more selection logic accretes (Task 3).
- The ba2b4b2 commit (Task 5) swept in two uncommitted doc edits of the
  controller's own via `git add -A` — content was legitimate (test-count
  corrections belonging on this branch) but bundled into the wrong commit.
  Not worth rewriting history; recorded so it isn't a surprise later.
- Build-time variance, observed but not previously recorded here: three clean
  cold-build measurements clustered at 5.3-8.6s, but a build under concurrent
  load (a second cargo invocation contending for the machine) was observed to
  take up to 42s, and a separate final-review run stalled for close to 10
  minutes under similar contention before completing successfully. The
  README's build-time line is worded to reflect the worst case, not the
  typical one, so a slow or stalled-looking build under load isn't mistaken
  for a hang.

## Live verification — NOT PERFORMED ON THIS BRANCH

Read this before assuming the port is proven. The shell build's Task 6 entry
above (line 51) records a live pass that genuinely happened, with plugin-log
exit codes and real recorded state paths. **This branch has no equivalent.**

The plan's verification bar has four criteria. Three are met:

1. `cargo test` — 43 passed, 0 failed ✅
2. `./test.sh` — 37 assertions, 0 failures ✅
3. `cargo clippy --all-targets -- -D warnings` — clean ✅
4. Both keybindings confirmed by hand in a real Herdr session — **NOT DONE** ❌

The plugin was never linked into a running session on this branch,
`herdr server reload-config` was never run, and no key was ever pressed. The
user elected to run the live pass themselves rather than have the controller
touch their live terminal session; it had not been reported back when this was
written.

The specific claim no test in this repository can establish: the manifest's
`command` arrays use the **relative** path `./target/release/last`, and nothing
here proves it resolves at runtime. The evidence for it is that Herdr spawns
plugin commands with `current_dir(plugin_root)`
(`src/app/api/plugins/runtime.rs:123`), read directly in Herdr's source. That
is a strong reason to expect it to work. It is not a demonstration that it
does.

Calibrated statement of confidence: the suites are green, the `[[build]]` key
and the install-time build path were verified against Herdr's own source rather
than assumed, and the CLI subcommand spelling was checked against the real
0.7.5 binary. High confidence, not demonstrated. Anyone merging this should
either run the live pass or record explicitly that they chose not to.

DECISION (2026-07-30): the user was offered both options — run the live pass
first, or merge with it outstanding — and explicitly chose to merge without it.
This is that record. The branch was merged with criterion 4 unmet, knowingly.

First thing to do if the plugin misbehaves after install: check whether
`./target/release/last` resolved at all. `herdr plugin list --json` should show
version 0.2.0 with both actions registered, and the plugin log should show
`exit_code 0` for the `tab.focused` hook. If the log shows a spawn failure
instead, the relative path is the prime suspect and an absolute path in
`herdr-plugin.toml` is the fix.
