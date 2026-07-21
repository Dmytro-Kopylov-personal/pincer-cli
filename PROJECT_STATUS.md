# pincer-cli

A terminal client for Lobsters + Hacker News, built with Rust + ratatui.

## Status

Functional local prototype with responsive comments browsing, explicit flow-state management, persistence, and CI/release workflows.

## Current functionality

- **Story list**: feed/source variants (Lobsters Hottest/Newest + HN Top/New), story metadata, selection, refresh, feed switch, page navigation (`[`/`]`, `PageUp`/`PageDown`).
- **Comments view**: threaded rendering with capped indentation, selected-row highlighting, scrollbar, comment permalink open (`c`), collapse/expand toggle (`z`), in-thread search (`/` + Enter), next search hit (`n`), and jump to next high-score comment (`H`).
- **Browser actions**: story link (`o`), story comments (`b`), selected comment permalink (`c`) with explicit status/error feedback.
- **Performance improvements**:
  - comments load in a background thread (UI stays responsive),
  - bounded in-memory story-detail/comments cache for fast reopen,
  - background story-page prefetch + cache warmup across feeds for faster source switches,
  - shared `reqwest` client reuse,
  - cached wrapped comment lines by width to avoid per-frame rewrap work.
- **UX polish**:
  - in-app help overlay (`?`) with active keybindings,
  - profiling mode (`p`) showing frame/load telemetry in status,
  - status/help line adapts for search mode input.
- **Flow model**:
  - explicit app flow state machine separates list browsing, comments browsing, in-comments search, help overlay, and quitting,
  - loading/error/browser actions remain effects/status updates instead of separate long-lived modes.
- **Accessibility quick wins**:
  - explicit mode visibility in the status area (`SEARCH: ...` prompt while entering comment search),
  - help visibility indicator via dedicated keybindings overlay (`?`) with in-context controls,
  - non-color selected-row cue in comments (`▶` marker) in addition to highlight styling,
  - keymap discoverability cues shown in help text for alternate paths (for example `j/k` and paging keys like `pgup/pgdn`, `[`/`]`).
- **Persistence**:
  - selection is persisted to `~/.config/pincer-cli/state.json`; startup defaults to Lobsters Hottest page 1.
- **Reliability hardening**:
  - network timeouts and simple retry policy are applied for API requests.

## Controls

- **Global/list**: `j/k`, arrows, `g/G`, `tab`, `r`, `enter`, `o`, `b`, `[`/`]`, `PageUp`/`PageDown`, `?`, `p`, `q`
- **Comments**: `j/k`, arrows, `g/G`, `/`, `n`, `H`, `z`, `o`, `b`, `c`, `Esc`, `?`, `p`, `q`

### Accessibility usage notes

- Press `/` in comments to enter search mode; the status/help area switches to `SEARCH: ...` with Enter/Esc guidance.
- Press `?` to open the keybindings overlay at any time, then `?` or `Esc` to close it.
- In comments, the selected entry uses a visible `▶` marker so selection remains clear even without color differentiation.

## Architecture

- `src/main.rs`: terminal loop, input handling, flow transitions, comments loader thread/channel, persistence load/save hooks.
- `src/app.rs`: app flow state machine, comments/wrap caches, search/collapse/profiling state, selection/navigation helpers.
- `src/api.rs`: provider adapters + HTTP fetch, shared `OnceLock` client, timeout/retry logic, permalink builder, and HN comment-tree flattening.
- `src/state.rs`: persisted local state load/save.
- `src/ui.rs`: ratatui rendering for list/comments/status + help overlay.
- `tests/*.rs`: render-oriented regression tests via `ratatui::TestBackend`.

## Test/quality status

- Current suite passes (`cargo test --quiet`).
- Clippy-clean with strict warnings enabled in the local review loop.

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

`ratatui 0.30`, `crossterm 0.28`, `reqwest 0.12` (blocking, rustls-tls, json), `serde`/`serde_json`, `open 5`, `anyhow`, and `textwrap 0.16`.
