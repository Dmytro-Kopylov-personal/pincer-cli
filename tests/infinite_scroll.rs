use pincer_cli::api::{Feed, Story};
use pincer_cli::app::{App, NavMode};

/// Helper: create a test story with an index-based id/title
fn make_story(id: u32, title: &str) -> Story {
    Story {
        short_id: format!("s{id}"),
        title: title.to_string(),
        url: "https://example.com".into(),
        score: (id % 100) as i32,
        comment_count: (id % 50) as i32,
        tags: vec!["hn".into()],
        submitter_user: format!("user{id}"),
        comments_url: format!("https://example.com/comments/{id}"),
    }
}

/// Seed stories directly into the app's story list
fn seed_stories(app: &mut App, count: usize) {
    app.stories = (0..count)
        .map(|i| make_story(i as u32, &format!("Story {i}")))
        .collect();
}

/// Make a page-worth of test stories, like HN returns
fn make_page(page: u32, page_size: usize, base_id_offset: u32) -> Vec<Story> {
    let start = (page - 1) as usize * page_size;
    (0..page_size)
        .map(|i| {
            let id = base_id_offset + start as u32 + i as u32;
            make_story(id, &format!("Story {id}"))
        })
        .collect()
}

// ─── App-level unit tests for the infinite scroll logic ───

/// needs_more_stories triggers correctly after multiple page append operations.
#[test]
fn needs_more_stories_triggers_across_multiple_pages() {
    let mut app = App::new();
    app.nav_mode = NavMode::Infinite;

    // Simulate loading page 1 (25 stories)
    let page1 = make_page(1, 25, 0);
    app.append_stories(page1.clone());
    app.page = 1;
    assert_eq!(app.stories.len(), 25);

    // At position 0+15=15, it shouldn't trigger yet (15 < 25)
    app.selected = 9;
    assert!(!app.needs_more_stories(), "should not trigger at pos 9/25");

    // At position 10, 10+15=25 >= 25, should trigger
    app.selected = 10;
    assert!(app.needs_more_stories(), "should trigger at pos 10/25");

    // Simulate page 2 arriving
    let page2 = make_page(2, 25, 25);
    app.append_stories(page2.clone());
    app.page = 2;
    assert_eq!(app.stories.len(), 50);

    // At position 10 of 50, 10+15=25 < 50, shouldn't trigger
    assert!(!app.needs_more_stories(), "should not trigger at pos 10/50");

    // At position 35, 35+15=50 >= 50, should trigger
    app.selected = 35;
    assert!(app.needs_more_stories(), "should trigger at pos 35/50");

    // Page 3 arrives (75 total)
    let page3 = make_page(3, 25, 50);
    app.append_stories(page3);
    app.page = 3;
    assert_eq!(app.stories.len(), 75);

    // At position 35 of 75, 35+15=50 < 75, no trigger
    assert!(!app.needs_more_stories(), "should not trigger at pos 35/75");

    // At position 60, 60+15=75 >= 75, trigger
    app.selected = 60;
    assert!(app.needs_more_stories(), "should trigger at pos 60/75");
}

/// needs_more_stories returns false when stories_loading is true
#[test]
fn needs_more_stories_blocks_during_loading() {
    let mut app = App::new();
    app.nav_mode = NavMode::Infinite;
    seed_stories(&mut app, 25);
    app.selected = 20; // 20+15=35 >= 25, would trigger normally
    app.stories_loading = true;

    assert!(
        !app.needs_more_stories(),
        "should not trigger while loading"
    );
}

/// needs_more_stories returns false when stories list is empty
#[test]
fn needs_more_stories_blocks_when_empty() {
    let mut app = App::new();
    app.nav_mode = NavMode::Infinite;
    app.selected = 0;

    assert!(!app.needs_more_stories(), "should not trigger when empty");
}

/// append_stories grows the list correctly and updates status
#[test]
fn append_stories_increments_status() {
    let mut app = App::new();
    app.nav_mode = NavMode::Infinite;

    app.append_stories(make_page(1, 25, 0));
    assert_eq!(app.stories.len(), 25);
    assert!(app.status.contains("25 stories"));

    app.append_stories(make_page(2, 25, 25));
    assert_eq!(app.stories.len(), 50);
    assert!(app.status.contains("50 stories"));

    app.append_stories(make_page(3, 25, 50));
    assert_eq!(app.stories.len(), 75);
    assert!(app.status.contains("75 stories"));
}

/// Moving selection down through pages: each lookahead trigger increments page
#[test]
fn scroll_down_triggers_incremental_page_loads() {
    let mut app = App::new();
    app.nav_mode = NavMode::Infinite;

    // Start with 25 stories (page 1 loaded)
    app.append_stories(make_page(1, 25, 0));
    app.page = 1;
    app.selected = 0;

    // The lookahead is 15. Trigger condition: selected + 15 >= len
    // With 25 stories, trigger at selected >= 10.

    // Scroll from 0 to 24. Only trigger at positions 10, 25 (after append), 40, 55, 70.
    let mut total_triggers = 0;
    for _step in 0..=100 {
        // Check before moving: is lookahead triggered?
        let was_at_trigger = app.needs_more_stories();
        if was_at_trigger {
            // This is what the event loop does
            let next_page = app.page.saturating_add(1);
            let page_stories = make_page(next_page, 25, (next_page - 1) * 25);
            app.page = next_page;
            app.append_stories(page_stories);
            total_triggers += 1;
        }
        // Move selection down by 1 (like pressing j)
        if app.selected < app.stories.len().saturating_sub(1) {
            app.selected += 1;
        }
    }

    // With 25 story pages and lookahead 15, we should have loaded several pages
    assert!(total_triggers > 0, "should have triggered at least once");
    assert!(
        app.stories.len() > 25,
        "should have loaded more than 25 stories, got {}",
        app.stories.len()
    );
    println!(
        "Scrolled {} times, triggered {} page loads, total stories: {}",
        100,
        total_triggers,
        app.stories.len()
    );
}

/// should_preload_next_page triggers automatically without scroll lookahead
#[test]
fn should_preload_next_page_fires_automatically() {
    let mut app = App::new();
    app.nav_mode = NavMode::Infinite;

    // No stories loaded yet — should not preload
    assert!(!app.should_preload_next_page(20));

    // Page 1 loaded, page=1, not loading — should preload
    app.append_stories(make_page(1, 25, 0));
    app.page = 1;
    assert!(app.should_preload_next_page(20));

    // While loading — should not preload
    app.stories_loading = true;
    assert!(!app.should_preload_next_page(20));
    app.stories_loading = false;

    // At max page — should not preload
    app.page = 20;
    assert!(!app.should_preload_next_page(20));

    // One before max — should preload
    app.page = 19;
    assert!(app.should_preload_next_page(20));
}

/// should_preload_next_page works for Lobsters feeds too
#[test]
fn should_preload_next_page_works_for_any_feed() {
    let mut app = App::new();
    app.nav_mode = NavMode::Infinite;
    app.append_stories(make_page(1, 25, 0));
    app.page = 1;

    assert!(app.should_preload_next_page(20));

    // Paged mode should not auto-preload
    app.nav_mode = NavMode::Paged;
    assert!(!app.should_preload_next_page(20));
}

/// The chain: auto-preload fires repeatedly until prefetch_max_pages
#[test]
fn auto_preload_chains_until_max() {
    let mut app = App::new();
    app.nav_mode = NavMode::Infinite;

    // Simulate the chain: page 1 loaded → auto-preload fires → page 2 starts
    // → page 2 loads → auto-preload fires → ...
    app.append_stories(make_page(1, 25, 0));
    app.page = 1;
    let mut loaded_pages = 0u32;
    for _ in 0..25 {
        if app.should_preload_next_page(20) {
            app.page = app.page.saturating_add(1);
            app.append_stories(make_page(app.page, 25, (app.page - 1) * 25));
            loaded_pages += 1;
            // Simulate begin → finish loading
            app.stories_loading = true;
            app.finish_stories_loading();
        }
    }

    // Should have loaded pages 2 through 20 (19 pages)
    assert_eq!(loaded_pages, 19);
    assert_eq!(app.page, 20);
    assert_eq!(app.stories.len(), 500); // 20 pages × 25 stories
    assert!(!app.should_preload_next_page(20)); // at max
}

#[test]
fn finish_loading_enables_next_scroll_trigger() {
    let mut app = App::new();
    app.nav_mode = NavMode::Infinite;
    seed_stories(&mut app, 25);
    app.selected = 10;

    // Should trigger
    assert!(app.needs_more_stories());

    // Simulate loading page 2
    app.stories_loading = true;
    assert!(!app.needs_more_stories()); // blocked

    // Simulate page 2 arriving
    app.finish_stories_loading();
    app.append_stories(make_page(2, 25, 25));
    assert!(!app.stories_loading);

    // After page 2, selected (10) + 15 = 25 < 50, no trigger immediately
    assert!(!app.needs_more_stories());

    // But scroll further to trigger page 3
    app.selected = 35;
    assert!(app.needs_more_stories());
}

/// Simulate the effect of stale-while-revalidate: cache load followed by background refresh.
/// In infinite mode, cached pages get appended and the flag stays false.
#[test]
fn cache_hit_does_not_set_loading_flag() {
    let mut app = App::new();
    app.nav_mode = NavMode::Infinite;

    // Pre-cache page 1 and 2
    let page1 = make_page(1, 25, 0);
    let page2 = make_page(2, 25, 25);
    app.cache_stories(Feed::HnTop, 1, &page1);
    app.cache_stories(Feed::HnTop, 2, &page2);

    // Load page 1 from cache (simulating what refresh_stories does on cache hit)
    if let Some(cached) = app.cached_stories(Feed::HnTop, 1) {
        app.append_stories(cached.stories);
    }
    assert!(
        !app.stories_loading,
        "cache hit should not set loading flag"
    );
    assert_eq!(app.stories.len(), 25);

    // Scroll to trigger page 2
    app.selected = 10;
    assert!(app.needs_more_stories());

    // Load page 2 from cache (simulating scroll-triggered cache hit)
    app.page = 2;
    if let Some(cached) = app.cached_stories(Feed::HnTop, 2) {
        app.append_stories(cached.stories);
    }
    assert!(
        !app.stories_loading,
        "cache hit for page 2 should not set loading"
    );
    assert_eq!(app.stories.len(), 50);
}
