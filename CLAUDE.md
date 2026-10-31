# Instructions for Claude

## Every change is a pull request

Nothing is committed or pushed straight to `beta`, `canary` or `main`. Each push to them is a release (docs/releasing.md), so a mistake pushed there becomes a broken release, and every fix after it another one.

1. **Branch.** Each change gets its own branch from the branch it will go into: `feat/…`, `fix/…`, `ci/…`, `docs/…`.
2. **Check before pushing.** `.github/check.sh` runs the checks CI runs on Linux: format, clippy and tests for the server's and the app's crates, `flutter analyze` and `flutter test`. Everything passes before the branch is pushed. A check that fails for a reason that has nothing to do with the change is still reported, not skipped.
3. **Open the pull request** with `gh pr create`, into the branch the change is for (usually `canary`). Its description says what changed, why, and how it was tested, including anything that couldn't be tested (Windows, macOS, real hardware).
4. **Read the pull request's checks.** CI runs the full workflow on the merge the pull request would make: Linux, Windows and macOS, the integration tests and the Docker image, and uploads the installers as artifacts. It releases nothing. Follow it with `gh pr checks` or `gh run watch`. Read every failure (`gh run view --log-failed`) and fix it on the same branch. Run `.github/check.sh` again before pushing the fix.
5. **Review the whole diff** (`gh pr diff`) before saying the pull request is ready.
6. **The owner merges.** Claude merges only when the owner says so for that pull request, and never with failing checks. Use a merge commit, never squash or rebase: the version is counted from the commits a release contains.

Moving work between release tracks (`beta` → `canary` → `main`) is a pull request too.

## Build commands

Start every `cargo`, `flutter`, `dart` and `flutter_rust_bridge_codegen` command with `source .superpowers/sdd/media/env.sh &&` (GEMINI.md says why); for the phone apps, every `yarn`, `npx`, `ubrn`, `adb`, `emulator` and Gradle command starts with `source mobile/scripts/env.sh &&`. Check `df -h /` before the full checks. Never run `cargo clean` without asking.
