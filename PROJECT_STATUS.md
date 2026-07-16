# pincer-cli

A terminal client for lobste.rs, built with Rust + ratatui.

## Status

Functional local prototype on `master` with responsive comments browsing, UX/navigation enhancements, persistence, and CI/release workflows.

## Current functionality

- **Story list**: Hottest/Newest feeds, story metadata, selection, refresh, feed switch, page navigation (`[`/`]`, `PageUp`/`PageDown`).
- **Comments view**: threaded rendering with capped indentation, selected-row highlighting, scrollbar, comment permalink open (`c`), collapse/expand toggle (`z`), in-thread search (`/` + Enter), next search hit (`n`), and jump to next high-score comment (`H`).
- **Browser actions**: story link (`o`), story comments (`b`), selected comment permalink (`c`) with explicit status/error feedback.
- **Performance improvements**:
  - comments load in a background thread (UI stays responsive),
  - bounded in-memory story-detail/comments cache for fast reopen,
  - shared `reqwest` client reuse,
  - cached wrapped comment lines by width to avoid per-frame rewrap work.
- **UX polish**:
  - in-app help overlay (`?`) with active keybindings,
  - profiling mode (`p`) showing frame/load telemetry in status,
  - status/help line adapts for search mode input.
- **Persistence**:
  - feed/page/selection are persisted to `~/.config/pincer-cli/state.json` and restored on startup.
- **Reliability hardening**:
  - network timeouts and simple retry policy are applied for Lobsters API requests.

## Controls

- **Global/list**: `j/k`, arrows, `g/G`, `tab`, `r`, `enter`, `o`, `b`, `[`/`]`, `PageUp`/`PageDown`, `?`, `p`, `q`
- **Comments**: `j/k`, arrows, `g/G`, `/`, `n`, `H`, `z`, `o`, `b`, `c`, `Esc`, `?`, `p`, `q`

## Architecture

- `src/main.rs`: terminal loop, input handling, comments loader thread/channel, search/collapse/help/profiling key handling, persistence load/save hooks.
- `src/app.rs`: app state, comments/wrap caches, search/collapse/profiling state, selection/navigation helpers.
- `src/api.rs`: Lobsters models + HTTP fetch, shared `OnceLock` client, timeout/retry logic, permalink builder.
- `src/state.rs`: persisted local state load/save (`feed/page/selected`).
- `src/ui.rs`: ratatui rendering for list/comments/status + help overlay.
- `tests/*.rs`: render-oriented regression tests via `ratatui::TestBackend`.

## Test/quality status

- Current suite passes (`cargo test --quiet`): **11 tests**.
- Clippy-clean with strict warnings enabled in loop workflow.

## Dev workflow

- `scripts/review-loop.sh` runs iterative code + security review checks until green:
  - `cargo fmt --all --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test --quiet`
  - `cargo audit` if installed (or enforce with `--require-audit`)
- GitHub workflows:
  - `.github/workflows/ci.yml` for fmt/clippy/tests/security audit (actions pinned to immutable SHAs).
  - `.github/workflows/release.yml` for tagged release artifact publishing with pinned actions, provenance attestation, and main-branch ancestry verification.

## Open items

- Create/push GitHub remote and verify workflows on hosted CI.
- Expand release artifacts beyond Linux x86_64 if multi-platform distribution is required.
- Configure GitHub environment protection rules for `release` and protected tag policies in repository settings.

## Dependencies

`ratatui 0.29`, `crossterm 0.28`, `reqwest 0.12` (blocking, rustls-tls, json), `serde`/`serde_json`, `open 5`, `anyhow`, `textwrap 0.16`, plus currently unused `termimad 0.30`.
