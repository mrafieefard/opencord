# Instructions for Gemini

## Current work

You are implementing `docs/superpowers/plans/2026-10-18-media-uploads.md`. The design it builds on is `docs/superpowers/specs/2026-10-18-media-uploads-design.md`.

- The plan's **Working Rules** section applies to everything you do in this repository.
- The user releases one milestone at a time. Stop at the end of each milestone and report.

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
