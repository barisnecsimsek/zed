# Fork modification ledger

This file tracks every change to upstream files in the Zed fork. See the
[Fork Maintenance Playbook](https://linear.app/necprojects/document/fork-maintenance-playbook-42eeef8f5fea)
for the rules. Every `FORK-BEGIN(...)` block in upstream code must have a row here.

## Upstream file modifications

| Marker | Files | Reason | Consumer | Upstream candidate? |
| --- | --- | --- | --- | --- |
| `metal_shaders_runtime` | `Cargo.toml` (workspace root, `[workspace.dependencies] gpui_macos` line) | Force `runtime_shaders` feature on the workspace's gpui_macos dependency. Lets us build on macOS without full Xcode (`xcrun metal` ships only with the Xcode IDE, not Command Line Tools). Runtime cost is one shader compilation at app startup. | All dev builds on the fork. | No — release builds should keep ahead-of-time shader compilation for faster cold start. |
| `integration_dep` | `crates/zed/Cargo.toml` | Add `fork_integration` to the `zed` binary's dependencies so its `init()` runs at startup. | `fork_integration` (calls every feature crate's init). | No — fork-specific. |
| `integration_init` | `crates/zed/src/main.rs` | Call `fork_integration::init(cx)` from the binary's startup. Placed after all upstream inits so feature crates see a fully-registered workspace. | All fork features. | No — fork-specific. |
| `fork_thread_switcher_keybinds` | `assets/keymaps/default-macos.json` | Add a `ForkThreadSwitcher` context with `shift-backspace` → `fork::ArchiveSelectedThread`. Mirrors the sidebar's archive keybind in the fork's modal. Rename has no binding here because `shift-r` would conflict with the modal's filter input. | `thread_switcher` modal. | No — fork-specific. |
| `switcher_reuse` | `crates/sidebar/src/sidebar.rs`, `crates/sidebar/src/thread_switcher.rs` | Pure visibility changes (`pub(crate)`/`pub(super)`/private → `pub`): the `thread_switcher` module, `ThreadSwitcherEntry`/`ThreadSwitcherSelection` types + accessors, `Sidebar::mru_entries_for_switcher`, `Sidebar::confirm_switcher_selection`. Lets the fork's switcher consume the sidebar's exact data pipeline (live status, notified, icons, worktrees+branches) and activation behavior instead of duplicating it. No logic changes. | `thread_switcher` fork crate. | Maybe — harmless API surface increase; could be argued upstream. |

## Hook traits / accessors added upstream

_(None yet.)_

## Sync status

- Upstream remote: `https://github.com/zed-industries/zed.git`
- Sync cadence: weekly, on a `sync/YYYY-MM-DD` branch (see playbook).
- Last successful sync: _(none yet — fork initialized at upstream commit `d130d03f5d` baseline; `main` currently at `d7ac5e6cf4`)_

## Upstream PRs in flight

_(None yet.)_
