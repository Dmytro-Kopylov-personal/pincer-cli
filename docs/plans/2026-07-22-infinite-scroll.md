# Infinite Scroll Mode Implementation Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Add a toggleable infinite scroll mode alongside the existing paged mode, so users can seamlessly scroll through all stories without pressing `[`/`]` to flip pages.

**Architecture:** A new `NavMode` enum (`Paged | Infinite`) stored in `App` and `PersistedConfig`. In infinite mode, `move_selection` past the end of loaded stories triggers a background fetch of the next page, which appends to `app.stories`. The `stories_cache` is keyed by `(Feed, page)` and reused. A visual indicator shows how many pages are loaded. A toggle keybind (e.g. `m` or config-only) switches modes.

**Tech Stack:** Same as existing — ratatui, crossterm, reqwest, serde, mpsc channels. No new dependencies.

**Files touched:**
- `src/app.rs` — new field, nav logic, scroll-to-bottom detection
- `src/main.rs` — handle new key action, wire up page appending
- `src/ui.rs` — show mode in banner/status
- `src/keymap.rs` — new `ToggleNavMode` action
- `src/config.rs` — `nav_mode` config field
- `src/state.rs` — persist nav mode? (optional)

---

### Task 1: Add NavMode enum and field to App

**Objective:** Define the navigation mode type and add it to `App` with default `Paged`.

**Files:**
- Modify: `src/app.rs` - add enum and field

**Step 1: Add NavMode enum before App struct**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavMode {
    Paged,
    Infinite,
}

impl NavMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Paged => "paged",
            Self::Infinite => "infinite",
        }
    }
}
```

**Step 2: Add field to App struct**

Add `pub nav_mode: NavMode,` to the `App` struct fields.

**Step 3: Initialize in App::new()**

```rust
nav_mode: NavMode::Paged,
```

**Step 4: Add toggle method**

```rust
pub fn toggle_nav_mode(&mut self) {
    self.nav_mode = match self.nav_mode {
        NavMode::Paged => NavMode::Infinite,
        NavMode::Infinite => NavMode::Paged,
    };
    self.status = format!("Navigation mode: {}", self.nav_mode.as_str());
}
```

**Step 5: Build & test**

Run: `cargo test`
Expected: all tests pass.

**Step 6: Commit**

```bash
git add src/app.rs
git commit -m "feat: add NavMode enum (Paged/Infinite) to App"
```

---

### Task 2: Add scroll-to-bottom detection and next-page trigger to App

**Objective:** When in infinite mode and user selects the last story, automatically fetch the next page. Also reset on feed/refresh.

**Files:**
- Modify: `src/app.rs`

**Step 1: Add method to check if we need to load more**

```rust
#[must_use]
pub fn needs_more_stories(&self) -> bool {
    self.nav_mode == NavMode::Infinite
        && !self.stories.is_empty()
        && self.selected + 1 >= self.stories.len()
}
```

Note: this just checks "is the user at the last story". The actual fetch trigger happens in main.rs when we detect this.

**Step 2: Add method to track how many pages are loaded (for display)**

The existing `page` field tracks the current page. In infinite mode, `page` becomes "how many pages have been loaded so far". When the user hits the bottom, increment page and load.

**Step 3: Add method to reset for new feed/refresh**

```rust
pub fn reset_stories(&mut self) {
    self.stories.clear();
    self.selected = 0;
    self.page = 1;
    self.stories_cache.retain(|(f, _), _| *f != self.feed);
    self.prefetch_started_feeds.remove(&self.feed);
}
```

**Step 4: Add method to append stories from next page**

```rust
pub fn append_stories(&mut self, stories: Vec<Story>) {
    let offset = self.stories.len();
    self.stories.extend(stories);
    self.status = format!(
        "Loaded {} stories ({} pages)",
        self.stories.len(),
        self.page
    );
}
```

**Step 5: Build & test**

Run: `cargo test`
Expected: all tests pass.

**Step 6: Commit**

```bash
git add src/app.rs
git commit -m "feat: add infinite scroll helpers (needs_more_stories, append_stories, reset_stories)"
```

---

### Task 3: Add ToggleNavMode key action and keybinding

**Objective:** Define a new `ToggleNavMode` action and give it a keybinding (e.g. `m`).

**Files:**
- Modify: `src/keymap.rs`

**Step 1: Add variant to KeyAction enum**

```rust
pub enum KeyAction {
    // ... existing variants ...
    ToggleNavMode,
}
```

**Step 2: Add keybinding — add `KeyCode::Char('m')` to `global_action`**

```rust
fn global_action(code: KeyCode) -> Option<KeyAction> {
    match code {
        // ... existing ...
        KeyCode::Char('m') => Some(KeyAction::ToggleNavMode),
        _ => None,
    }
}
```

Note: `m` is unused and not near other one-letter keys. Keep it as global (works in both list and comments views).

**Step 3: Build & update tests if needed**

Run: `cargo test`
Expected: all tests pass. Existing keymap tests shouldn't be affected.

**Step 4: Commit**

```bash
git add src/keymap.rs
git commit -m "feat: add ToggleNavMode key action (m key)"
```

---

### Task 4: Wire up toggle action in main event loop

**Objective:** Handle `KeyAction::ToggleNavMode` in the `handle_key` function.

**Files:**
- Modify: `src/main.rs`

**Step 1: Add match arm after existing actions in handle_key**

In the large `match action { ... }` block, add:

```rust
KeyAction::ToggleNavMode => {
    app.toggle_nav_mode();
}
```

**Step 2: Build & test**

Run: `cargo test`
Expected: all tests pass.

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire ToggleNavMode key action to App.toggle_nav_mode"
```

---

### Task 5: Implement infinite scroll logic in main loop

**Objective:** When `move_selection` lands on the last story in infinite mode, trigger a background fetch of the next page.

**Files:**
- Modify: `src/main.rs`

**Step 1: After `move_selection` in the main loop, check if we need more**

In the `run()` function's event loop, after the `handle_key` call and before `terminal.draw`, add a check:

```rust
// After handle_key(...)
if matches!(action, KeyAction::MoveDown) && app.needs_more_stories() {
    app.page = app.page.saturating_add(1);
    refresh_stories(
        app,
        &stories_tx,
        &prefetch_tx,
        true,  // use_cache first
        settings.prefetch_max_pages,
    );
}
```

Wait, this won't work cleanly because `action` is scoped inside `handle_key`. Let me think about this differently.

Actually, the better approach: after `handle_key`, check if we need more stories and the stories are not currently loading. This fits better in the run loop:

In `run()`, after `handle_key(...)`:

```rust
if app.needs_more_stories() && !app.stories_loading {
    app.page = app.page.saturating_add(1);
    refresh_stories(app, &stories_tx, &prefetch_tx, false, settings.prefetch_max_pages);
}
```

`needs_more_stories()` checks `nav_mode == Infinite && selected + 1 >= stories.len()`.

**Step 2: Handle story load results in infinite mode — append instead of replace**

Modify `apply_stories_load_results` — when in infinite mode, instead of replacing `app.stories`, append to them:

```rust
match loaded.result {
    Ok(stories) => {
        if app.nav_mode == NavMode::Infinite && app.page > 1 {
            // Append to existing stories
            app.cache_stories(loaded.feed, loaded.resolved_page, stories.clone());
            app.append_stories(stories);
            app.finish_stories_loading();
        } else {
            // Existing behavior — replace
            app.cache_stories(loaded.feed, loaded.resolved_page, stories.clone());
            app.page = loaded.resolved_page;
            app.stories = stories;
            app.selected = 0;
            app.finish_stories_loading();
            // ... existing prefetch logic ...
        }
    }
```

Actually, let me simplify. `append_stories` should handle status and the page tracking. Let me keep it cleaner:

```rust
Ok(stories) => {
    app.cache_stories(loaded.feed, loaded.resolved_page, stories.clone());
    app.finish_stories_loading();
    
    if app.nav_mode == NavMode::Infinite && !loaded.fell_back_to_first_page && loaded.resolved_page > 1 {
        app.page = loaded.resolved_page;
        app.append_stories(stories);
    } else {
        app.page = loaded.resolved_page;
        app.stories = stories;
        app.selected = 0;
        // ... existing prefetch logic ...
    }
}
```

**Step 3: On refresh/cycle feed in infinite mode, reset to page 1**

When user presses `r` or cycles feed in infinite mode, reset: clear all accumulated stories, start fresh.

In `handle_key`, `KeyAction::Refresh` and `KeyAction::CycleFeed` already call `refresh_stories`. For infinite mode, also call `app.reset_stories()` before refresh.

**Step 4: On moving down past the last loaded story**

This is handled by the edge trigger in the main loop (step 1).

**Step 5: Build & test**

Run: `cargo test`
Expected: all tests pass.

**Step 6: Commit**

```bash
git add src/main.rs
git commit -m "feat: implement infinite scroll story loading and appending"
```

---

### Task 6: Update UI for infinite mode

**Objective:** Show the current nav mode in the banner and adjust the status bar.

**Files:**
- Modify: `src/ui.rs`

**Step 1: Update mode banner to show nav mode**

In `mode_banner_text()` (app.rs, not ui.rs — update it there):

```rust
pub fn mode_banner_text(&self) -> String {
    format!(
        " MODE {} | {} | CONTRAST {} ",
        self.mode_label(),
        self.nav_mode.as_str().to_uppercase(),
        contrast_mode,
    )
}
```

This adds to the top banner: `MODE LIST | INFINITE | CONTRAST DEFAULT`.

**Step 2: Update status bar help text for list view**

In `draw_status`, when on list view, show different help hints depending on mode:

- Paged: `j/k move • enter=comments • ... • [ ] pgup/pgdn=page • m=mode • ...`
- Infinite: `j/k move • enter=comments • ... • scroll=load more • m=mode • ...`

Wait, `[ ]` and `pgup/pgdn` still work in both modes — they just also advance pages. So the help text can stay the same. Just update the status line with page info.

**Step 3: Show page info in status**

In infinite mode, show `"p.1-3 loaded (75 stories)"` instead of `"page 3"`.

In `draw_status`, after computing the `status` string:

```rust
if app.nav_mode == NavMode::Infinite {
    status = format!("{} (pages 1-{} loaded)", status, app.page);
}
```

**Step 4: Build & test**

Run: `cargo test`
Expected: all tests pass.

**Step 5: Commit**

```bash
git add src/app.rs src/ui.rs
git commit -m "feat: show nav mode in banner and page count in status"
```

---

### Task 7: Add nav_mode to config (optional, config-only)

**Objective:** Allow setting `nav_mode` in `config.json` startup settings so the default can be infinite.

**Files:**
- Modify: `src/config.rs`
- Modify: `src/main.rs` (apply config on startup)

**Step 1: Add nav_mode field to StartupConfig**

```rust
#[derive(Debug, Clone, Deserialize, Default)]
pub struct StartupConfig {
    pub feed: Option<String>,
    pub page: Option<u32>,
    pub restore_feed_page: Option<bool>,
    pub nav_mode: Option<String>, // "paged" or "infinite"
}
```

**Step 2: Apply config in main.rs resolve_settings**

```rust
if let Some(startup) = cfg.startup.as_ref() {
    if let Some(mode_str) = startup.nav_mode.as_deref() {
        match mode_str {
            "infinite" => app.nav_mode = NavMode::Infinite,
            _ => {} // default to Paged
        }
    }
}
```

**Step 3: Build & test**

Run: `cargo test`
Expected: all tests pass.

**Step 4: Commit**

```bash
git add src/config.rs src/main.rs
git commit -m "feat: support nav_mode in config.json startup settings"
```

---

### Task 8: Update help overlay

**Objective:** Add `m` to the help overlay.

**Files:**
- Modify: `src/ui.rs`

**Step 1: Add to help string**

In `draw_help_overlay`, update the "Global" line:

```
Global: o open story, b open thread, m toggle mode, p profiling, ? help, q quit, Esc back
```

**Step 2: Build & test**

Run: `cargo test`
Expected: all tests pass.

**Step 3: Commit**

```bash
git add src/ui.rs
git commit -m "docs: add m toggle mode to help overlay"
```

---

## Edge Cases & Pitfalls

1. **Lobsters page boundary**: Lobsters returns 25 stories per page. When the last page is reached, the API returns fewer than 25 or an empty array. `fetch_stories_with_fallback` should handle this — it falls back to page 1 on 404/empty. For infinite mode, we need to detect "no more pages" and stop fetching. In `apply_stories_load_results`, if `loaded.resolved_page != loaded.requested_page` (fallback hit), or if `stories.is_empty()`, set a flag `infinite_exhausted` to stop trying.

2. **HN pageless API**: HN returns all 500 story IDs in one call. We slice them by page. So "no more pages" is when `(page - 1) * 25 >= ids.len()`. Already handled by `fetch_hn_stories` returning empty `Vec`.

3. **Cache interaction**: In infinite mode, we cache each page individually (`stories_cache[(feed, page)]`) but append to a flat list. Switching to paged mode then back should work — cache is shared.

4. **Prefetch**: Background prefetch still works but is less critical in infinite mode since we load pages on demand. Could reduce prefetch_max_pages to 1 when in infinite mode, or just let it run — it'll warm the cache for the next page load.

5. **State restore**: On restart with `restore_feed_page=true`, infinite mode should probably just load page 1 fresh rather than trying to restore dozens of pages.

6. **`needs_more_stories()` timing**: The check needs to happen AFTER `move_selection` has updated `selected`, not before. Since the check is in the main loop after `handle_key`, and `handle_key` calls `move_selection`, the timing is correct.
