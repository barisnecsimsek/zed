mod thread_row;

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, atomic::AtomicBool};

use agent_ui::thread_metadata_store::ThreadMetadataStore;
use fuzzy::{StringMatchCandidate, match_strings};
use gpui::{
    Action as _, AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle,
    Focusable, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Task,
    WeakEntity, Window, actions, rems,
};
use picker::{Picker, PickerDelegate};
use sidebar::Sidebar;
use sidebar::thread_switcher::{ThreadSwitcherEntry, ThreadSwitcherSelection};
use ui::{
    AgentThreadStatus, Button, ContextMenu, IconButton, IconPosition, KeyBinding, ListItem,
    ListItemSpacing, PopoverMenu, PopoverMenuHandle, Tooltip, prelude::*, rems_from_px,
};
use util::ResultExt as _;
use workspace::{ModalView, MultiWorkspace, Workspace};

use crate::thread_row::ThreadRow;

actions!(
    fork,
    [
        /// Toggles the fork's sticky thread switcher with multi-field fuzzy search.
        ToggleThreadSwitcher,
        /// Archives the thread highlighted in the fork's thread switcher.
        ArchiveSelectedThread,
        /// Toggles the filter options menu in the fork's thread switcher footer.
        ToggleFilterMenu
    ]
);

const ALL_STATUSES: [AgentThreadStatus; 4] = [
    AgentThreadStatus::Completed,
    AgentThreadStatus::Running,
    AgentThreadStatus::WaitingForConfirmation,
    AgentThreadStatus::Error,
];

fn status_label(status: AgentThreadStatus) -> &'static str {
    match status {
        AgentThreadStatus::Completed => "Completed",
        AgentThreadStatus::Running => "Running",
        AgentThreadStatus::WaitingForConfirmation => "Waiting for Confirmation",
        AgentThreadStatus::Error => "Error",
    }
}

pub fn init(cx: &mut App) {
    cx.observe_new(ThreadSwitcher::register).detach();
}

const TITLE_WEIGHT: f64 = 3.0;
const WORKTREE_WEIGHT: f64 = 2.0;
const BRANCH_WEIGHT: f64 = 1.0;

pub struct ThreadSwitcher {
    picker: Entity<Picker<ThreadSwitcherDelegate>>,
    _subscriptions: Vec<Subscription>,
}

impl ThreadSwitcher {
    fn register(
        workspace: &mut Workspace,
        _window: Option<&mut Window>,
        _: &mut Context<Workspace>,
    ) {
        workspace.register_action(|workspace, _: &ToggleThreadSwitcher, window, cx| {
            Self::toggle(workspace, window, cx);
        });
    }

    fn toggle(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
        let Some(multi_workspace) = window.root::<MultiWorkspace>().flatten() else {
            return;
        };
        let Some(sidebar) = multi_workspace
            .read(cx)
            .sidebar()
            .and_then(|handle| handle.to_any().downcast::<Sidebar>().ok())
        else {
            return;
        };
        workspace.toggle_modal(window, cx, |window, cx| ThreadSwitcher::new(sidebar, window, cx));
    }

    fn new(sidebar: Entity<Sidebar>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let delegate = ThreadSwitcherDelegate::new(sidebar.clone(), cx);
        let picker = cx.new(|cx| Picker::list(delegate, window, cx));
        let picker_focus_handle = picker.focus_handle(cx);
        picker.update(cx, |picker, _| {
            picker.delegate.focus_handle = picker_focus_handle;
        });
        let subscriptions = vec![
            cx.subscribe(&picker, |_, _, _: &DismissEvent, cx| {
                cx.emit(DismissEvent);
            }),
            cx.observe_in(&sidebar, window, |this, _, window, cx| {
                this.picker.update(cx, |picker, cx| {
                    let query = picker.query(cx);
                    picker.delegate.reload(cx);
                    picker.update_matches(query, window, cx);
                });
            }),
        ];

        // Defer the initial reload until the current effect cycle ends —
        // building entries reads workspace entities, and we're currently
        // inside Workspace::toggle_modal which mutably leases the active
        // workspace. Reading it now panics with a double-lease.
        let window_handle = window.window_handle();
        let picker_handle = picker.downgrade();
        cx.defer(move |cx| {
            let _ = window_handle.update(cx, |_, window, cx| {
                picker_handle
                    .update(cx, |picker, cx| {
                        let query = picker.query(cx);
                        picker.delegate.reload(cx);
                        picker.update_matches(query, window, cx);
                    })
                    .ok();
            });
        });

        Self {
            picker,
            _subscriptions: subscriptions,
        }
    }

    fn archive_selected(&mut self, cx: &mut Context<Self>) {
        let Some(selection) = self.picker.read(cx).delegate.selected_selection() else {
            return;
        };
        if let ThreadSwitcherSelection::Thread { metadata, .. } = selection
            && let Some(store) = ThreadMetadataStore::try_global(cx)
        {
            store.update(cx, |store, cx| {
                store.archive(metadata.thread_id, None, cx)
            });
        }
    }

    fn toggle_filter_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.picker.update(cx, |picker, cx| {
            picker
                .delegate
                .filter_popover_menu_handle
                .toggle(window, cx);
        });
    }

}

fn refresh_after_filter_change(
    picker: &mut Picker<ThreadSwitcherDelegate>,
    window: &mut Window,
    cx: &mut Context<Picker<ThreadSwitcherDelegate>>,
) {
    let query = picker.query(cx);
    picker.delegate.reload(cx);
    picker.update_matches(query, window, cx);
}

fn build_filter_menu(
    window: &mut Window,
    cx: &mut App,
    focus_handle: FocusHandle,
    picker_handle: WeakEntity<Picker<ThreadSwitcherDelegate>>,
    enabled_statuses: Vec<AgentThreadStatus>,
    disabled_agents: HashSet<SharedString>,
    distinct_agents: Vec<SharedString>,
) -> Entity<ContextMenu> {
    ContextMenu::build(window, cx, move |mut menu, _window, _cx| {
        menu = menu
            .context(focus_handle.clone())
            .header("Status");
        for status in ALL_STATUSES {
            let toggled = enabled_statuses.contains(&status);
            let picker = picker_handle.clone();
            menu = menu.toggleable_entry(
                status_label(status),
                toggled,
                IconPosition::End,
                None,
                move |window, cx| {
                    let picker = picker.clone();
                    let _ = picker.update(cx, |picker, cx| {
                        if let Some(pos) = picker
                            .delegate
                            .enabled_statuses
                            .iter()
                            .position(|s| *s == status)
                        {
                            picker.delegate.enabled_statuses.remove(pos);
                        } else {
                            picker.delegate.enabled_statuses.push(status);
                        }
                        refresh_after_filter_change(picker, window, cx);
                    });
                },
            );
        }
        if !distinct_agents.is_empty() {
            menu = menu.separator().header("Agent");
            for agent in distinct_agents {
                let toggled = !disabled_agents.contains(&agent);
                let picker = picker_handle.clone();
                let agent_for_handler = agent.clone();
                menu = menu.toggleable_entry(
                    agent.clone(),
                    toggled,
                    IconPosition::End,
                    None,
                    move |window, cx| {
                        let picker = picker.clone();
                        let agent_key = agent_for_handler.clone();
                        let _ = picker.update(cx, |picker, cx| {
                            if picker.delegate.disabled_agents.contains(&agent_key) {
                                picker.delegate.disabled_agents.remove(&agent_key);
                            } else {
                                picker.delegate.disabled_agents.insert(agent_key);
                            }
                            refresh_after_filter_change(picker, window, cx);
                        });
                    },
                );
            }
        }
        menu
    })
}

impl Render for ThreadSwitcher {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context("ForkThreadSwitcher")
            .w(rems(34.))
            .on_action(cx.listener(|this, _: &ArchiveSelectedThread, _, cx| {
                this.archive_selected(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleFilterMenu, window, cx| {
                this.toggle_filter_menu(window, cx);
            }))
            .child(self.picker.clone())
    }
}

impl Focusable for ThreadSwitcher {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl EventEmitter<DismissEvent> for ThreadSwitcher {}
impl ModalView for ThreadSwitcher {
    fn on_before_dismiss(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> workspace::DismissDecision {
        let submenu_focused = self.picker.update(cx, |picker, cx| {
            picker
                .delegate
                .filter_popover_menu_handle
                .is_focused(window, cx)
        });
        workspace::DismissDecision::Dismiss(!submenu_focused)
    }
}

#[derive(Clone, Default)]
struct EntryMatch {
    entry_index: usize,
    score: f64,
    title_positions: Vec<usize>,
    worktree_positions: Vec<usize>,
}

pub struct ThreadSwitcherDelegate {
    sidebar: Entity<Sidebar>,
    entries: Vec<ThreadSwitcherEntry>,
    title_candidates: Vec<StringMatchCandidate>,
    worktree_candidates: Vec<StringMatchCandidate>,
    branch_candidates: Vec<StringMatchCandidate>,
    matches: Vec<EntryMatch>,
    selected_index: usize,
    previous_query: String,
    focus_handle: FocusHandle,
    filter_popover_menu_handle: PopoverMenuHandle<ContextMenu>,
    enabled_statuses: Vec<AgentThreadStatus>,
    disabled_agents: HashSet<SharedString>,
}

impl ThreadSwitcherDelegate {
    fn new(sidebar: Entity<Sidebar>, cx: &mut Context<ThreadSwitcher>) -> Self {
        Self {
            sidebar,
            entries: Vec::new(),
            title_candidates: Vec::new(),
            worktree_candidates: Vec::new(),
            branch_candidates: Vec::new(),
            matches: Vec::new(),
            selected_index: 0,
            previous_query: String::new(),
            focus_handle: cx.focus_handle(),
            filter_popover_menu_handle: PopoverMenuHandle::default(),
            enabled_statuses: ALL_STATUSES.to_vec(),
            disabled_agents: HashSet::new(),
        }
    }

    fn reload(&mut self, cx: &App) {
        let previously_selected_id = self.selected_element_id();
        let mut entries = self.sidebar.read(cx).mru_entries_for_switcher(cx);
        self.apply_filters(&mut entries, cx);
        entries.sort_by_key(|entry| !entry.notified());

        self.title_candidates = entries
            .iter()
            .enumerate()
            .map(|(i, e)| StringMatchCandidate::new(i, &e.title()))
            .collect();
        self.worktree_candidates = entries
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let text = e
                    .worktrees()
                    .iter()
                    .filter_map(|wt| wt.worktree_name.clone())
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                StringMatchCandidate::new(i, &text)
            })
            .collect();
        self.branch_candidates = entries
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let text = e
                    .worktrees()
                    .iter()
                    .filter_map(|wt| wt.branch_name.clone())
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                StringMatchCandidate::new(i, &text)
            })
            .collect();
        self.matches = default_matches(entries.len());
        self.entries = entries;
        self.selected_index = previously_selected_id
            .as_ref()
            .and_then(|id| self.find_index_by_element_id(id))
            .unwrap_or(0);
    }

    fn selected_element_id(&self) -> Option<SharedString> {
        let mat = self.matches.get(self.selected_index)?;
        self.entries.get(mat.entry_index).map(|e| e.element_id())
    }

    fn apply_filters(&self, entries: &mut Vec<ThreadSwitcherEntry>, _cx: &App) {
        entries.retain(|e| match e.selection() {
            ThreadSwitcherSelection::Thread { metadata, .. } => !metadata.archived,
            ThreadSwitcherSelection::Terminal { .. } => true,
        });
        if !self.enabled_statuses.is_empty() && self.enabled_statuses.len() < ALL_STATUSES.len() {
            entries.retain(|e| self.enabled_statuses.iter().any(|s| *s == e.status()));
        }
        if !self.disabled_agents.is_empty() {
            entries.retain(|e| match e.selection() {
                ThreadSwitcherSelection::Thread { metadata, .. } => {
                    let agent_key: SharedString = metadata.agent_id.as_ref().to_string().into();
                    !self.disabled_agents.contains(&agent_key)
                }
                ThreadSwitcherSelection::Terminal { .. } => true,
            });
        }
    }

    fn distinct_agents(&self) -> Vec<SharedString> {
        let mut seen: HashSet<SharedString> = HashSet::new();
        let mut out: Vec<SharedString> = Vec::new();
        for entry in &self.entries {
            if let ThreadSwitcherSelection::Thread { metadata, .. } = entry.selection() {
                let key: SharedString = metadata.agent_id.as_ref().to_string().into();
                if seen.insert(key.clone()) {
                    out.push(key);
                }
            }
        }
        out.sort();
        out
    }

    fn find_index_by_element_id(&self, id: &SharedString) -> Option<usize> {
        self.matches.iter().position(|m| {
            self.entries
                .get(m.entry_index)
                .is_some_and(|e| e.element_id() == *id)
        })
    }

    fn selected_selection(&self) -> Option<ThreadSwitcherSelection> {
        let mat = self.matches.get(self.selected_index)?;
        self.entries.get(mat.entry_index).map(|e| e.selection())
    }
}

fn default_matches(count: usize) -> Vec<EntryMatch> {
    (0..count)
        .map(|i| EntryMatch {
            entry_index: i,
            ..Default::default()
        })
        .collect()
}

impl PickerDelegate for ThreadSwitcherDelegate {
    type ListItem = AnyElement;

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(
        &mut self,
        index: usize,
        _: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        self.selected_index = index;
        cx.notify();
    }

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> Arc<str> {
        "Search threads by title, worktree, or branch...".into()
    }

    fn update_matches(
        &mut self,
        query: String,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let query_changed = query != self.previous_query;
        self.previous_query = query.clone();
        let executor = cx.background_executor().clone();
        let title_candidates = self.title_candidates.clone();
        let worktree_candidates = self.worktree_candidates.clone();
        let branch_candidates = self.branch_candidates.clone();
        let entry_count = self.entries.len();

        cx.spawn_in(window, async move |this, cx| {
            let matches = if query.is_empty() {
                default_matches(entry_count)
            } else {
                let cancel = AtomicBool::new(false);
                let title_matches = match_strings(
                    &title_candidates,
                    &query,
                    false,
                    true,
                    100,
                    &cancel,
                    executor.clone(),
                )
                .await;
                let worktree_matches = match_strings(
                    &worktree_candidates,
                    &query,
                    false,
                    true,
                    100,
                    &cancel,
                    executor.clone(),
                )
                .await;
                let branch_matches = match_strings(
                    &branch_candidates,
                    &query,
                    false,
                    true,
                    100,
                    &cancel,
                    executor,
                )
                .await;

                let mut by_entry: BTreeMap<usize, EntryMatch> = BTreeMap::new();
                for mat in title_matches {
                    let entry = by_entry.entry(mat.candidate_id).or_insert(EntryMatch {
                        entry_index: mat.candidate_id,
                        ..Default::default()
                    });
                    entry.score += mat.score * TITLE_WEIGHT;
                    entry.title_positions = mat.positions;
                }
                for mat in worktree_matches {
                    let entry = by_entry.entry(mat.candidate_id).or_insert(EntryMatch {
                        entry_index: mat.candidate_id,
                        ..Default::default()
                    });
                    entry.score += mat.score * WORKTREE_WEIGHT;
                    entry.worktree_positions = mat.positions;
                }
                for mat in branch_matches {
                    let entry = by_entry.entry(mat.candidate_id).or_insert(EntryMatch {
                        entry_index: mat.candidate_id,
                        ..Default::default()
                    });
                    entry.score += mat.score * BRANCH_WEIGHT;
                }

                let mut matches: Vec<EntryMatch> = by_entry.into_values().collect();
                matches.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                matches
            };

            this.update_in(cx, |this, _window, cx| {
                let preserve_id = if query_changed {
                    None
                } else {
                    this.delegate.selected_element_id()
                };
                this.delegate.matches = matches;
                this.delegate.selected_index = preserve_id
                    .as_ref()
                    .and_then(|id| this.delegate.find_index_by_element_id(id))
                    .unwrap_or(0);
                cx.notify();
            })
            .log_err();
        })
    }

    fn confirm(&mut self, _: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        if let Some(selection) = self.selected_selection() {
            self.sidebar.update(cx, |sidebar, cx| {
                sidebar.confirm_switcher_selection(&selection, window, cx);
            });
        }
        cx.emit(DismissEvent);
    }

    fn dismissed(&mut self, _: &mut Window, cx: &mut Context<Picker<Self>>) {
        cx.emit(DismissEvent);
    }

    fn render_footer(
        &self,
        _: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<AnyElement> {
        let focus_handle = self.focus_handle.clone();
        let enabled_statuses = self.enabled_statuses.clone();
        let disabled_agents = self.disabled_agents.clone();
        let distinct_agents = self.distinct_agents();
        let any_filter_active =
            enabled_statuses.len() < ALL_STATUSES.len() || !disabled_agents.is_empty();
        let picker_handle = cx.entity().downgrade();

        Some(
            h_flex()
                .w_full()
                .p_1p5()
                .justify_between()
                .border_t_1()
                .border_color(cx.theme().colors().border_variant)
                .child(
                    PopoverMenu::new("fork-thread-switcher-filter-menu")
                        .with_handle(self.filter_popover_menu_handle.clone())
                        .attach(gpui::Anchor::BottomRight)
                        .anchor(gpui::Anchor::BottomLeft)
                        .offset(gpui::Point {
                            x: gpui::px(1.0),
                            y: gpui::px(1.0),
                        })
                        .trigger_with_tooltip(
                            IconButton::new("fork-thread-switcher-filter-trigger", IconName::Sliders)
                                .icon_size(IconSize::Small)
                                .toggle_state(any_filter_active)
                                .when(any_filter_active, |this| {
                                    this.indicator(ui::Indicator::dot().color(Color::Info))
                                }),
                            {
                                let focus_handle = focus_handle.clone();
                                move |_window, cx| {
                                    Tooltip::for_action_in(
                                        "Filter Options",
                                        &ToggleFilterMenu,
                                        &focus_handle,
                                        cx,
                                    )
                                }
                            },
                        )
                        .menu({
                            let focus_handle = focus_handle.clone();
                            move |window, cx| {
                                Some(build_filter_menu(
                                    window,
                                    cx,
                                    focus_handle.clone(),
                                    picker_handle.clone(),
                                    enabled_statuses.clone(),
                                    disabled_agents.clone(),
                                    distinct_agents.clone(),
                                ))
                            }
                        }),
                )
                .child(
                    Button::new("archive-thread", "Archive")
                        .key_binding(
                            KeyBinding::for_action_in(
                                &ArchiveSelectedThread,
                                &focus_handle,
                                cx,
                            )
                            .map(|kb| kb.size(rems_from_px(12.))),
                        )
                        .on_click(|_, window, cx| {
                            window.dispatch_action(ArchiveSelectedThread.boxed_clone(), cx)
                        }),
                )
                .into_any(),
        )
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let mat = self.matches.get(ix)?;
        let entry = self.entries.get(mat.entry_index)?;

        let mut worktrees = entry.worktrees();
        if let Some(first) = worktrees.first_mut() {
            first.highlight_positions = mat.worktree_positions.clone();
        }

        let mut row = ThreadRow::new(("thread-row", ix), entry.title())
            .icon(entry.icon())
            .status(entry.status())
            .notified(entry.notified())
            .timestamp(entry.timestamp())
            .highlight_positions(mat.title_positions.clone())
            .worktrees(worktrees)
            .selected(selected);
        if let Some(svg) = entry.icon_from_external_svg() {
            row = row.custom_icon_from_external_svg(svg);
        }
        if let Some(project_name) = entry.project_name() {
            row = row.project_name(project_name);
        }

        Some(
            ListItem::new(ix)
                .inset(true)
                .toggle_state(selected)
                .spacing(ListItemSpacing::Sparse)
                .child(row)
                .into_any_element(),
        )
    }
}
