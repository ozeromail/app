# Ozero Mail

Mail client: shared Rust core (`core/`), native apps in Jetpack Compose (`android/`) and SwiftUI (`ios/`).

## Git and PRs

- Author every commit as `Khizri Makhmudov <khizrim@khizrim.ru>`. No `Co-Authored-By`, `Claude-Session` or "Generated with Claude Code" lines.
- Conventional Commits for commits and PR titles: `feat(core): add IMAP login`.
- Everything in the repo is in English: code, comments, docs, commits, PRs. Keep it short and plain.
- Before opening a PR, run `/simplify`, then `/code-review`, and fix what they find.

## Core layers

`domain` (types and ports, no I/O) ← `app` (use cases) ← `api` (facade for the apps); adapters in `core/adapters/` implement `domain` ports. The rules live in `tools/check-layers.py`.

## Code

- No `unsafe` outside generated bridge code. No `unwrap`, `expect`, `panic!` or `todo!` outside tests.
- Add a dependency only when writing it ourselves is riskier, with a license allowed in `core/deny.toml`, in its own commit.
- The UI reads only the local database; every action changes the database first and goes through the action queue.

## Strings

- Code uses keys only. `i18n/en.json` is the source, `i18n/ru.json` the second locale.
- Add every key to both files; `tools/check-i18n.py` enforces this.
- The name is always "Ozero Mail" in text; lowercase only in the logo.

## Before a PR

```sh
cd core && cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test
python3 tools/check-layers.py && python3 tools/check-i18n.py
```
