# Instructions for Gemini

## Current work

You are implementing `docs/superpowers/plans/2026-10-18-media-uploads.md`. The design it builds on is `docs/superpowers/specs/2026-10-18-media-uploads-design.md`.

- The plan's **Working Rules** section applies to everything you do in this repository. Rules 13–21 were added after the A1, A2 and A3 reviews: read them before each milestone.
- Each milestone's code was reviewed and fixed after you finished it, and its decisions entry has a **Review** bullet. Read the latest one before starting, and build on the fixed code, not on what you remember writing.
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

## Write docs from the code, not from memory

Each milestone's stop in the plan gives that milestone's decisions entry:

- Copy it exactly into `docs/decisions.md` and fill in the commit hashes.
- Check every sentence against the code. If one doesn't match what you built, fix the code or the sentence, and say which in your report.
- Numbers and behavior count too: a wrong quality setting or a wrong "X doesn't happen" is as bad as a wrong name.

When you write the progress ledger, a report, or a decisions entry the plan doesn't give:

- Name only things that exist. Copy every message, request, event, function and file name, and every number, from the code or from `git diff pre..HEAD`.
- Before you finish, search the code for each name you wrote (`grep -rn`). If a name isn't found, it doesn't go in.

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
