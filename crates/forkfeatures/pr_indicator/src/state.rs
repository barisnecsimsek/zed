use std::collections::HashMap;
use std::time::SystemTime;

use gpui::{App, Context};
use title_bar::ExtraBranchChip;

use crate::REFRESH_INTERVAL;

/// What we last learned about a particular `(owner, repo, branch)` triple.
#[derive(Clone, Debug)]
pub enum PrState {
    /// A PR was found — open, draft, merged, or closed.
    Found(PullRequestInfo),
    /// We queried GitHub and got an empty result. Tracked separately from
    /// "haven't queried yet" so the chip can stay hidden without re-fetching.
    None,
}

#[derive(Clone, Debug)]
pub struct PullRequestInfo {
    pub number: u64,
    pub title: String,
    pub html_url: String,
    /// One of `open`, `draft`, `merge_queue`, `merged`, `closed`. The chip is
    /// currently rendered in a neutral color in all states — color tinting is
    /// a deferred follow-up (see NEC-16).
    pub state: &'static str,
    pub fetched_at: SystemTime,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct Target {
    pub owner: String,
    pub repo: String,
    pub branch: String,
}

struct CacheEntry {
    state: PrState,
    fetched_at: SystemTime,
}

pub struct PrIndicatorState {
    cache: HashMap<Target, CacheEntry>,
    observed: Vec<Target>,
    current_visible: Option<Target>,
}

impl PrIndicatorState {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            cache: HashMap::new(),
            observed: Vec::new(),
            current_visible: None,
        }
    }

    pub fn current_pr(&self) -> Option<&PullRequestInfo> {
        let target = self.current_visible.as_ref()?;
        match self.cache.get(target).map(|entry| &entry.state)? {
            PrState::Found(pr) => Some(pr),
            PrState::None => None,
        }
    }

    pub fn pr_for(&self, owner: &str, repo: &str, branch: &str) -> Option<&PullRequestInfo> {
        let target = Target {
            owner: owner.to_string(),
            repo: repo.to_string(),
            branch: branch.to_string(),
        };
        match self.cache.get(&target).map(|entry| &entry.state)? {
            PrState::Found(pr) => Some(pr),
            PrState::None => None,
        }
    }

    /// Called by the title-bar renderer on every render pass. Records the
    /// triple the UI is asking about, schedules a fetch if we've never seen
    /// it, and updates the command palette action's visibility.
    pub fn observe_target(
        &mut self,
        owner: String,
        repo: String,
        branch: String,
        cx: &mut Context<Self>,
    ) {
        let target = Target { owner, repo, branch };
        let changed = self.current_visible.as_ref() != Some(&target);
        self.current_visible = Some(target.clone());

        if !self.observed.iter().any(|t| t == &target) {
            self.observed.push(target.clone());
        }

        let needs_fetch = !self.cache.contains_key(&target);
        if changed {
            update_palette_visibility(self, cx);
        }
        if needs_fetch {
            cx.spawn(async move |this, cx| {
                let Some(this) = this.upgrade() else { return };
                if let Err(err) = crate::refresh_target(&this, target, cx).await {
                    log::warn!("pr_indicator: failed to fetch PR: {err:#}");
                }
            })
            .detach();
        }
    }

    /// Updates the cache after a fetch completes. Bumps the title-bar global
    /// so observing title bars re-render and updates command-palette filtering.
    pub fn record_result(&mut self, target: Target, state: PrState, cx: &mut Context<Self>) {
        self.cache.insert(
            target,
            CacheEntry {
                state,
                fetched_at: SystemTime::now(),
            },
        );
        ExtraBranchChip::notify_changed(cx);
        update_palette_visibility(self, cx);
    }

    /// The poll loop calls this to discover which triples need a refresh.
    pub(crate) fn targets_due_for_refresh(&self) -> Vec<Target> {
        let now = SystemTime::now();
        self.observed
            .iter()
            .filter(|target| {
                self.cache
                    .get(*target)
                    .map(|entry| {
                        now.duration_since(entry.fetched_at)
                            .map(|d| d >= REFRESH_INTERVAL)
                            .unwrap_or(true)
                    })
                    .unwrap_or(true)
            })
            .cloned()
            .collect()
    }
}

fn update_palette_visibility(state: &PrIndicatorState, cx: &mut App) {
    let visible = state.current_pr().is_some();
    crate::update_action_visibility(cx, visible);
}
