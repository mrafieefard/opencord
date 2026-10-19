# Instructions for Gemini

## Current work

You are implementing `docs/superpowers/plans/2026-10-18-media-uploads.md`. The design it builds on is `docs/superpowers/specs/2026-10-18-media-uploads-design.md`.

- The plan's **Working Rules** and this file apply to everything you do in this repository. Read "How to write code here" before each milestone: it grows after every review.
- Each milestone's code was reviewed and fixed after you finished it, and its decisions entry has a **Review** bullet. Read the latest one before starting, and build on the fixed code, not on what you remember writing.
- From A5 on, the plan gives behavior, interfaces, names and tests, not finished code. You write the code, and the rules below are what the review checks it against.
- The user releases one milestone at a time. Stop at the end of each milestone and report.

## Every build command loads the environment

Each shell command you run starts in a fresh shell, so an `export` from an earlier command is gone. Start **every** `cargo`, `flutter`, `dart`, `sqlx` and `flutter_rust_bridge_codegen` command with:

```bash
source .superpowers/sdd/media/env.sh && <command>
```

Without it, builds keep incremental caches and debug info, and the nearly full disk fills up.

`env.sh` is git-ignored. If it's missing on this machine, create it with:

```bash
export PATH="$HOME/.cargo/bin:$HOME/.local/share/flutter/bin:$PATH"
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_RELEASE_DEBUG=0
```

Check `df -h /` before the full checks. With less than 10 GB free, delete `target/debug/incremental` and the test binaries in `target/debug/deps` older than 45 minutes. Never run `cargo clean` without asking.

## How to write code here

Every rule here was broken at least once and caught in review.

### Order of work

- **Check cheapest first:** who is asking (token, permissions held in memory), then other in-memory checks, then database reads, then request bodies or files, then writes.
  - A handler never reads a body before it knows who sent it.
  - When the plan lists a handler's steps, they are already in this order. Keep it.

### Errors

- **Never turn an error into "nothing".** Propagate with `?`.
  - Never wrap a query in `if let Ok(..)`, `.ok()` or `unwrap_or_default()` when its result decides what gets deleted or skipped. A failed query then looks like an empty one.
  - Never write `let _ =` on a `Result` (database, files, network). If a failure is truly harmless, log it with `tracing` and say why in a comment.

### Shared state and races

- **State changes are conditional.** Write each transition as `UPDATE … WHERE id = $1 AND state = '<expected>'` and return `rows_affected() == 1` as a `bool`.
  - `false` means the row was cancelled, sent or changed meanwhile. Handle it the way the task says; if the task doesn't say, ask.
- **A transaction holds only writes.**
  - SQLite runs in WAL mode. A transaction that has read fails at once with `SQLITE_BUSY_SNAPSHOT` when it writes after another connection has committed.
  - So read and check first, on a plain connection, then `begin()` for the writes.
  - When a read must still hold at the write, make the write conditional instead of reading inside the transaction.
- **A check holds only under the lock that guards the change.**
  - A handler that checks, then awaits (a network test, a file), then writes must check again after taking the write lock, next to the write. The early check only refuses sooner.
  - Work in flight counts. A background job records where it will write (a storage, a channel) on its row, under the write lock, before it writes. Then a check for "anything there?" sees it.
  - Don't rebuild in-memory state that another handler keeps in step with the database. Update it where the row changes.
- **A lock guard lives until the end of its statement.**
  - A guard taken in the head of a `for`, `while let`, `if let` or `match` is held for the whole body, so locking again inside deadlocks.
  - Copy what you need into a local first, then loop.
- **Std locks:**
  - Use `.lock().unwrap_or_else(PoisonError::into_inner)`, and the same for `read()` and `write()`, as `state.rs` does.
  - Never hold one across `.await`.

### Resources

- **Every in-memory map says when its entries leave.**
  - Remove the entry where it is used, and on every error path. A guard whose `Drop` removes it covers both.
  - No map may grow with the number of uploads, messages or users ever seen.
- **CPU-heavy work runs in `tokio::task::spawn_blocking`:** decoding images, audio or video; hashing, sealing or compressing large data.

### Reuse and conventions

- **Reuse what exists.** Before writing a helper, grep for one. The same ten lines in two places is a review finding.
  - The server's attachments code already has `attachments::open_key` and `key_label`, `attachments::http::attachment_to_proto`, `to_status_proto` and `readable_attachment`, and `attachments::process::base_fields`.
- **HTTP replies** are axum tuples (`(StatusCode, headers, body).into_response()`), never `Response::builder()…unwrap()`.
- **New dependencies:** check the licence of the crate and of everything it pulls in, on every system, not just Linux:

  ```bash
  cargo tree -p <crate> -e normal --target all --format '{p} {l}' | grep -v 'MIT\|Apache\|BSD\|ISC\|Zlib'
  ```

  Only the plan's licence list is allowed. Any line this prints that isn't LGPL goes to the user before you keep the crate.

### Tests and task boundaries

- **A test sets up everything it depends on.** Never rely on a default (an empty list, a setting's starting value) or on another test's data. Set it in the test.
- **Every commit passes the checks on its own.**
  - Code that only a later task uses goes in with that task. Otherwise `-D warnings` fails on dead code.
  - Don't add `#[allow(dead_code)]` to get past it.

## Write docs from the code, not from memory

Every milestone ends with a decisions entry in `docs/decisions.md`.

- **When the plan gives the entry's text:**
  - copy it exactly and fill in the commit hashes;
  - check every sentence against the code. If one doesn't match what you built, fix the code or the sentence, and say which in your report.
- **When the plan doesn't give it (from A5 on):** write it from `git diff <first commit>^..HEAD`, in the shape of the previous entry:
  - **Why**, with the commits;
  - one bullet per part, saying what it does;
  - **Tests**.
- **Numbers and behavior count too:** a wrong quality setting or a wrong "X doesn't happen" is as bad as a wrong name.
- **Name only things that exist.** Copy every message, request, event, function and file name, and every number, from the code or the diff. This covers the decisions entry, the progress ledger and your report.
- **Search before you finish.** Grep for each name you wrote (`grep -rn`). If a name isn't found, it doesn't go in.

## Reports and the ledger

- **A deviation** is anything you did differently from the plan, or added without the plan asking for it:
  - a public name, file, user-visible message, behavior or dependency;
  - where the plan gives code, any non-test line that isn't in it, even one that only makes a test pass.

  Name each one in the ledger and in the report, with the failure that made you add it. If `git diff` shows one, "Deviations: none" is wrong.
- **Every problem names its test and pastes its error line.** A claim without them is left out.
- **Re-run a flaky test alone,** one test per command, and paste the result:

  ```bash
  cargo test -p <crate> --lib -- --exact <module::path::test_name>
  ```

## When you're stuck: ask Claude

If the same step fails **three times in a row**, stop retrying and ask Claude. "Fails" means the same test, build, lint or check still fails, or fails in a new way, after three different fix attempts. Claude is a stronger model that knows this codebase and wrote the plan.

1. **Write the question** to `.superpowers/sdd/media/questions/NN-short-name.md`. `NN` counts up: `01`, `02`, … Use this template and fill in every part:

   ```
   I'm Gemini, implementing docs/superpowers/plans/2026-10-18-media-uploads.md in this repository. Don't edit any files: tell me the cause and the exact fix.

   Task and step: Task N, Step M: <title>
   What the plan asks: <quote the relevant lines>
   Command that fails: <the exact command>
   Error output (the last 80 lines or so):
   <paste>
   What I tried, and what happened each time:
   1. <attempt>: <result>
   2. <attempt>: <result>
   3. <attempt>: <result>
   Files involved: <paths>
   ```

2. **Ask** from the repository root. It can take a few minutes:

   ```bash
   claude -p "$(cat .superpowers/sdd/media/questions/NN-short-name.md)" --model opus --disallowedTools Edit Write NotebookEdit Bash
   ```

3. **Apply the answer.**
   - Add Claude's answer to the same file under a `## Answer` heading.
   - Make the fix yourself.
   - Run the failing command again.

4. **Record it** in that task's progress ledger entry: `Asked Claude: questions/NN-short-name.md`, plus one line saying what the fix was.

### Limits

- **At most two questions per problem.** If Claude's fix doesn't work either, stop and ask the user.
- **Claude does not replace the user.** These still go to the user, not Claude:
  - the plan and the design disagree;
  - a step can't be done as written;
  - a tool is missing;
  - anything about scope, product choices, branches, pushing, or deleting work.
- **Claude only advises.** In this mode it has no edit or shell tools. You make every change, and the change is yours.
