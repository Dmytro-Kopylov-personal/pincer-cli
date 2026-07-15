# pincer-cli

A minimal terminal client for lobste.rs (https://lobste.rs), built with
Rust + ratatui. Formerly named `claw`; renamed to `pincer-cli` since `claw`
was heavily overloaded on crates.io/GitHub.

## Status: early, functional prototype

Runs, fetches real data, navigable. Not yet published anywhere (no GitHub
remote, not on crates.io). Local repo only at `~/dev/pincer-cli`, branch
`master`.

## What works today

- **Story list view** — fetches Hottest/Newest feeds from lobste.rs's JSON
  API (`/hottest.json`, `/newest.json`), renders score, title, tags,
  comment count, submitter. `j/k` to move, `tab` to switch feed, `r` to
  refresh, `o` to open the story URL in the browser, `enter` to open
  comments, `q` to quit.
- **Comment view** — fetches a story's full comment tree
  (`/s/<short_id>.json`), renders threaded comments with depth-based
  indentation, upvote score, commenter name. Long comment bodies are
  hard-wrapped to the terminal width (via `textwrap`) so they can't
  overflow past the panel border. The selected comment gets a highlighted
  background band, a `▶` marker, and a yellow badge on the username; the
  list auto-scrolls the selection into view via `ratatui::ListState`
  (mirrors how the story list already behaved).
- **Regression tests** (`cargo test`, 5 passing) using ratatui's
  `TestBackend` to render into an in-memory buffer and assert on actual
  cell styles/content — not just "does it compile":
  - `tests/ui_highlight.rs` — selected-row background is genuinely
    distinct from unselected rows (this caught a real bug: the first
    highlight implementation styled the `Line` but ratatui's per-span
    styling shadowed it, so nothing was visible).
  - `tests/comments_wrap_and_scroll.rs` — a very long comment rendered in
    a narrow (40-col) terminal doesn't corrupt the right border, and
    scrolling deep into a 200-comment thread doesn't panic. These target
    the two root causes behind the "browsing comments is broken, borders
    get messed up when you step" bug report: (1) unwrapped comment text
    overflowing past the box-drawing border, (2) no `ListState` meant the
    viewport never followed the selection past the first screenful.

## Architecture

- `src/api.rs` — `Story`/`Comment`/`StoryDetail`/`Feed` types, blocking
  `reqwest` calls against lobste.rs's public JSON endpoints. No auth.
- `src/app.rs` — `App` state struct (`View::List | View::Comments`,
  selection indices, current feed/page, fetched stories/comments,
  `story_detail_title`).
- `src/ui.rs` — all ratatui rendering (`draw`, `draw_list`,
  `draw_comments`, `draw_status`).
- `src/main.rs` — terminal setup/teardown, event loop, key handling,
  wires `api` fetch results into `App` state.
- `src/lib.rs` — thin `pub mod api; pub mod app; pub mod ui;` re-export so
  the binary's internals are reachable from integration tests (bin-only
  crates can't otherwise be exercised by `tests/*.rs`).

## Known rough edges / open items

- `short_id` on `Comment` is currently unused (1 dead-code warning) —
  reserved for a planned "open this comment in browser" (`b` key) feature
  that isn't wired up yet.
- No GitHub remote yet. Plan is to push under the
  `Dmytro-Kopylov-personal` GitHub account (HTTPS+PAT), matching the
  pattern used for `aether`/`azulejos`.
- Comment typography was flagged as "looks bad, might be centered" in the
  most recent session — investigated with a `TestBackend` dump of real
  rendered output (see transcript/tests) and confirmed text is left-
  aligned, not centered; no `Alignment::Center` anywhere in `ui.rs`. Likely
  candidate for the visual complaint is depth-based indentation (`"  "
  .repeat(depth)`) making nested replies drift right, which can *look*
  like centering at a glance, especially since the header's indent
  (`depth`) and body's indent (`depth + 1`) are offset by one level from
  each other — worth double-checking/aligning if it still looks off.
  **Not yet fixed — paused per user request to write this doc and stop
  for now.**
- No scrollbar indicator for the comments list, so the ratatui-managed
  auto-scroll (via `ListState`) has no visual scrollbar affordance; user
  has to infer position from the highlighted row.
- No pagination beyond `page` counter on the story list feed fetch (i.e.
  no visible "load next page" UX beyond whatever key/behavior already
  exists — check `main.rs` for exact binding).
- Interactive PTY-based automated testing (feeding synthetic keypresses
  via a scripted terminal) has proven unreliable in this environment for
  driving the app end-to-end; verification instead relies on ratatui's
  `TestBackend` for deterministic rendering assertions, and the user
  running `cargo run` directly in their own terminal for real interactive
  QA.

## Commit history (local, not pushed)

```
da1a37d Fix comments view border corruption and scroll desync
da7906e Fix selection highlight to actually apply bg to each span; add lib target + regression test
1431db5 Improve comment selection visibility: highlight bg, bold body text, badge on selected user
33effce Rename project claw -> pincer-cli
b9d2315 Initial scaffold: lobsters TUI client (list + comment view, ratatui/crossterm/termimad)
```

## Dependencies

`ratatui 0.29`, `crossterm 0.28`, `reqwest 0.12` (blocking, rustls-tls,
json), `serde`/`serde_json`, `open 5`, `termimad 0.30` (not yet actually
used anywhere in `src/` — pulled in early, likely intended for markdown-
rendered comment bodies later but currently comments render as plain
wrapped text), `anyhow`, `textwrap 0.16`.
