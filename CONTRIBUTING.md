# Contributing

## Scope

This repository contains a Rust terminal client (`pincer-cli`) for Lobsters and Hacker News.

## Development setup

1. Install Rust toolchain `1.97.0` (see `rust-toolchain.toml`).
2. Clone the repository.
3. Build and test:

```bash
cargo test
```

## Required checks

Run the review loop before opening a pull request:

```bash
./scripts/review-loop.sh
```

The loop enforces:

- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --quiet`
- `cargo audit` (if available)

## Pull request requirements

1. Keep changes scoped to the target issue.
2. Update tests when behavior changes.
3. Update `README.md` and `PROJECT_STATUS.md` when user-facing behavior changes.
4. Keep documentation technical and concise.
5. Do not commit secrets or local artifacts.

## Commit guidance

- Use descriptive commit messages.
- Keep commits logically grouped.

## Reporting issues

Use issue templates for bug reports and feature requests.
