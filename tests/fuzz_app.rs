use pincer_cli::api::Story;
use pincer_cli::app::{App, NavMode};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a story stub — only the fields that App methods touch.
fn stub_story(short_id: &str, title: &str) -> Story {
    Story {
        short_id: short_id.to_string(),
        title: title.to_string(),
        url: String::new(),
        score: 0,
        comment_count: 0,
        tags: Vec::new(),
        submitter_user: String::new(),
        comments_url: String::new(),
    }
}

/// Seed App with N stories so fuzz operations have something to navigate.
fn seed_stories(app: &mut App, n: usize) {
    app.stories = (0..n)
        .map(|i| stub_story(&format!("s{i}"), &format!("Story {i}")))
        .collect();
    if app.selected >= app.stories.len() && !app.stories.is_empty() {
        app.selected = 0;
    }
}

// ---------------------------------------------------------------------------
// Invariants — checked after every operation
// ---------------------------------------------------------------------------

fn check_invariants(app: &App) {
    // 1. selected is always in-bounds when stories exist
    if !app.stories.is_empty() {
        assert!(
            app.selected < app.stories.len(),
            "selected {} out of bounds for {} stories",
            app.selected,
            app.stories.len()
        );
    }

    // 2. page is never 0 (1-based)
    assert!(app.page >= 1, "page is {}. must be >= 1", app.page);

    // 3. stories_loading is consistent: if false, is_current_stories_request may be stale
    //    (can't easily check the channel side here, but the flag shouldn't stay true forever)

    // 4. After toggle_nav_mode, stories are cleared and needs_initial_load is true
    //    (checked in the operation function)

    // 5. needs_more_stories is only true in Infinite mode
    if app.needs_more_stories() {
        assert_eq!(
            app.nav_mode,
            NavMode::Infinite,
            "needs_more_stories() true but nav_mode is {:?}",
            app.nav_mode
        );
    }

    // 6. needs_fill_stories is always false (we removed auto-fill)
    assert!(
        !app.needs_fill_stories(),
        "needs_fill_stories should always be false"
    );
}

// ---------------------------------------------------------------------------
// Action strategy — generates random operations on App
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum FuzzOp {
    MoveSelection(i32),
    JumpTop,
    JumpBottom,
    NextPage,
    PrevPage,
    ToggleNavMode,
    ResetStories,
    AppendStories(usize),
    FinishLoading,
    BeginLoading,
}

fn arb_fuzz_op() -> impl Strategy<Value = FuzzOp> {
    prop_oneof![
        4 => (0i32..20).prop_map(FuzzOp::MoveSelection),    // move: common
        2 => (0i32..40).prop_map(|d| FuzzOp::MoveSelection(-d)), // move up: common
        1 => Just(FuzzOp::JumpTop),
        1 => Just(FuzzOp::JumpBottom),
        1 => Just(FuzzOp::NextPage),
        1 => Just(FuzzOp::PrevPage),
        1 => Just(FuzzOp::ToggleNavMode),
        1 => Just(FuzzOp::ResetStories),
        2 => (1usize..50).prop_map(FuzzOp::AppendStories),
        1 => Just(FuzzOp::FinishLoading),
        1 => Just(FuzzOp::BeginLoading),
    ]
}

proptest! {
    /// Fuzz the App state machine with a sequence of random operations.
    /// Verifies invariants hold after every step.
    #[test]
    fn app_state_machine_fuzz(ops in prop::collection::vec(arb_fuzz_op(), 1..100)) {
        let mut app = App::new();
        // Seed with 0-5 stories
        let initial_stories = ops[0..1].len() % 6;
        seed_stories(&mut app, initial_stories);
        check_invariants(&app);

        for op in &ops {
            apply_op(&mut app, op);
            check_invariants(&app);
        }
    }

    /// Fuzz with no initial stories — covers empty-list edge cases.
    #[test]
    fn app_state_machine_empty_fuzz(ops in prop::collection::vec(arb_fuzz_op(), 1..50)) {
        let mut app = App::new();
        check_invariants(&app);
        for op in &ops {
            apply_op(&mut app, op);
            check_invariants(&app);
        }
    }

    /// Fuzz with a large story list — covers boundary near selected.
    #[test]
    fn app_state_machine_large_fuzz(ops in prop::collection::vec(arb_fuzz_op(), 1..50)) {
        let mut app = App::new();
        seed_stories(&mut app, 200);
        check_invariants(&app);
        for op in &ops {
            apply_op(&mut app, op);
            check_invariants(&app);
        }
    }
}

// ---------------------------------------------------------------------------
// Apply a single operation to App
// ---------------------------------------------------------------------------

fn apply_op(app: &mut App, op: &FuzzOp) {
    match op {
        FuzzOp::MoveSelection(delta) => {
            let _prev = app.selected;
            app.move_selection(*delta);
            // Invariant checked by check_invariants — selected stays in bounds
        }

        FuzzOp::JumpTop => {
            app.jump_top();
            if !app.stories.is_empty() {
                assert_eq!(
                    app.selected, 0,
                    "jump_top should put selected at 0, got {}",
                    app.selected
                );
            }
        }

        FuzzOp::JumpBottom => {
            app.jump_bottom();
            if !app.stories.is_empty() {
                assert_eq!(
                    app.selected,
                    app.stories.len() - 1,
                    "jump_bottom should put selected at {}, got {}",
                    app.stories.len() - 1,
                    app.selected
                );
            }
        }

        FuzzOp::NextPage => {
            let prev = app.page;
            app.next_page();
            assert!(
                app.page >= prev,
                "next_page decreased page from {} to {}",
                prev,
                app.page
            );
        }

        FuzzOp::PrevPage => {
            app.prev_page();
            assert!(app.page >= 1, "prev_page went below 1");
        }

        FuzzOp::ToggleNavMode => {
            let was_infinite = app.nav_mode == NavMode::Infinite;
            app.toggle_nav_mode();
            // After toggle: stories cleared, page=1, needs_initial_load=true
            assert!(
                app.stories.is_empty(),
                "stories not cleared after nav toggle"
            );
            assert_eq!(app.page, 1, "page not reset after nav toggle");
            assert!(
                app.needs_initial_load,
                "needs_initial_load not set after nav toggle"
            );
            assert_eq!(
                app.nav_mode == NavMode::Infinite,
                !was_infinite,
                "nav mode didn't flip"
            );
        }

        FuzzOp::ResetStories => {
            app.reset_stories();
            assert!(app.stories.is_empty(), "stories not cleared by reset");
            assert_eq!(app.selected, 0, "selected not reset");
            assert_eq!(app.page, 1, "page not reset to 1");
        }

        FuzzOp::AppendStories(n) => {
            let before = app.stories.len();
            let new_stories: Vec<Story> = (0..*n)
                .map(|i| {
                    stub_story(
                        &format!("fuzz-{}-{i}", app.stories.len()),
                        &format!("Fuzz {i}"),
                    )
                })
                .collect();
            app.append_stories(new_stories);
            assert_eq!(
                app.stories.len(),
                before + n,
                "append_stories didn't add {n} stories (before={before}, after={})",
                app.stories.len()
            );
        }

        FuzzOp::FinishLoading => {
            app.finish_stories_loading();
            assert!(
                !app.stories_loading,
                "finish_stories_loading didn't clear flag"
            );
        }

        FuzzOp::BeginLoading => {
            // begin_stories_loading borrows feed from app, which is fine
            let request_id = app.begin_stories_loading();
            assert!(app.stories_loading, "begin_stories_loading didn't set flag");
            assert!(
                app.is_current_stories_request(request_id),
                "fresh request should be current"
            );
        }
    }
}
