mod auth;
mod github;
mod remote;
mod state;

use std::any::TypeId;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use command_palette_hooks::CommandPaletteFilter;
use gpui::{
    AnyElement, App, AppContext as _, AsyncApp, Entity, MouseButton, SharedString, WeakEntity,
    Window, actions,
};
use project::git_store::Repository;
use title_bar::ExtraBranchChip;
use ui::{Button, TintColor, Tooltip, prelude::*};
use util::ResultExt as _;
use workspace::Workspace;

pub use state::PullRequestInfo;
use state::{PrIndicatorState, Target};

const REFRESH_INTERVAL: Duration = Duration::from_secs(90);

actions!(
    fork,
    [
        /// Opens the pull request associated with the current branch in the system browser.
        OpenPullRequestInBrowser
    ]
);

pub fn init(cx: &mut App) {
    let state = cx.new(PrIndicatorState::new);
    cx.set_global(GlobalPrIndicatorState(state.clone()));

    let renderer_state = state.clone();
    let renderer: title_bar::ExtraBranchChipRenderer = Arc::new(
        move |repository: &Entity<Repository>, _window: &mut Window, cx: &mut App| {
            render_chip(&renderer_state, repository, cx)
        },
    );
    ExtraBranchChip::set_renderer(renderer, cx);

    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        register_actions(workspace);
    })
    .detach();

    let poll_state = state.downgrade();
    cx.spawn(async move |cx| {
        poll_loop(poll_state, cx).await;
    })
    .detach();

    set_action_visibility(cx, false);
}

struct GlobalPrIndicatorState(Entity<PrIndicatorState>);
impl gpui::Global for GlobalPrIndicatorState {}

fn global_state(cx: &App) -> Entity<PrIndicatorState> {
    cx.global::<GlobalPrIndicatorState>().0.clone()
}

fn register_actions(workspace: &mut Workspace) {
    workspace.register_action(|_workspace, _: &OpenPullRequestInBrowser, _window, cx| {
        let state = global_state(cx);
        let Some(pr) = state.read(cx).current_pr().cloned() else {
            log::debug!("OpenPullRequestInBrowser invoked with no PR cached");
            return;
        };
        log::debug!("opening PR in browser: {}", pr.html_url);
        cx.open_url(&pr.html_url);
    });
}

fn set_action_visibility(cx: &mut App, visible: bool) {
    let type_id = TypeId::of::<OpenPullRequestInBrowser>();
    CommandPaletteFilter::update_global(cx, |filter, _| {
        if visible {
            filter.show_action_types([type_id].iter());
        } else {
            filter.hide_action_types(&[type_id]);
        }
    });
}

pub(crate) fn update_action_visibility(cx: &mut App, visible: bool) {
    set_action_visibility(cx, visible);
}

fn render_chip(
    state: &Entity<PrIndicatorState>,
    repository: &Entity<Repository>,
    cx: &mut App,
) -> Option<AnyElement> {
    let (owner, repo, branch) = {
        let repo = repository.read(cx);
        let remote_url = repo.remote_origin_url.clone()?;
        let branch_name = repo
            .branch
            .as_ref()
            .map(|branch| branch.name().to_string())?;
        let (owner, repo) = remote::parse_github_remote(&remote_url)?;
        (owner, repo, branch_name)
    };

    state.update(cx, |state, cx| {
        state.observe_target(owner.clone(), repo.clone(), branch.clone(), cx);
    });

    let pr = state.read(cx).pr_for(&owner, &repo, &branch).cloned()?;
    let pr_number = pr.number;
    let pr_html_url: SharedString = pr.html_url.into();
    let pr_tooltip_title: SharedString = pr.title.into();

    let chip = h_flex()
        .child(
            Button::new(("fork_pr_indicator_trigger", pr_number as usize), format!("#{pr_number}"))
                .selected_style(ButtonStyle::Tinted(TintColor::Accent))
                .label_size(LabelSize::Small)
                .color(Color::Muted)
                .start_icon(
                    Icon::new(IconName::Github)
                        .size(IconSize::XSmall)
                        .color(Color::Muted),
                )
                .tooltip(move |_window, cx| {
                    Tooltip::with_meta(
                        "Pull Request",
                        Some(&OpenPullRequestInBrowser),
                        pr_tooltip_title.clone(),
                        cx,
                    )
                })
                .on_click(move |_event, _window, cx| {
                    cx.open_url(&pr_html_url);
                }),
        )
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .into_any_element();

    Some(chip)
}

async fn poll_loop(state: WeakEntity<PrIndicatorState>, cx: &mut AsyncApp) {
    loop {
        cx.background_executor().timer(REFRESH_INTERVAL).await;
        let Some(state) = state.upgrade() else { break };
        let targets = state.update(cx, |state, _| state.targets_due_for_refresh());
        for target in targets {
            refresh_target(&state, target, cx).await.log_err();
        }
    }
}

pub(crate) async fn refresh_target(
    state: &Entity<PrIndicatorState>,
    target: Target,
    cx: &mut AsyncApp,
) -> Result<()> {
    let http_client = cx.update(|cx| cx.http_client());
    let token = auth::resolve_token().await;
    let pr = github::fetch_branch_pr(
        http_client,
        token.as_deref(),
        &target.owner,
        &target.repo,
        &target.branch,
    )
    .await?;
    state.update(cx, |state, cx| {
        state.record_result(target, pr, cx);
    });
    Ok(())
}

