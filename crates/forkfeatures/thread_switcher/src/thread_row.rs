use gpui::{App, ElementId, IntoElement, ParentElement, SharedString, Styled, Window, px};
use ui::{
    AgentThreadStatus, CommonAnimationExt, GradientFade, HighlightedLabel, IconName,
    ThreadItemWorktreeInfo, prelude::*,
};

#[derive(IntoElement)]
pub struct ThreadRow {
    id: ElementId,
    icon: IconName,
    icon_color: Option<Color>,
    custom_icon_from_external_svg: Option<SharedString>,
    title: SharedString,
    highlight_positions: Vec<usize>,
    timestamp: SharedString,
    notified: bool,
    status: AgentThreadStatus,
    selected: bool,
    worktrees: Vec<ThreadItemWorktreeInfo>,
    project_name: Option<SharedString>,
}

impl ThreadRow {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            icon: IconName::ZedAgent,
            icon_color: None,
            custom_icon_from_external_svg: None,
            title: title.into(),
            highlight_positions: Vec::new(),
            timestamp: "".into(),
            notified: false,
            status: AgentThreadStatus::default(),
            selected: false,
            worktrees: Vec::new(),
            project_name: None,
        }
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = icon;
        self
    }

    pub fn custom_icon_from_external_svg(mut self, svg: impl Into<SharedString>) -> Self {
        self.custom_icon_from_external_svg = Some(svg.into());
        self
    }

    pub fn status(mut self, status: AgentThreadStatus) -> Self {
        self.status = status;
        self
    }

    pub fn notified(mut self, notified: bool) -> Self {
        self.notified = notified;
        self
    }

    pub fn timestamp(mut self, timestamp: impl Into<SharedString>) -> Self {
        self.timestamp = timestamp.into();
        self
    }

    pub fn highlight_positions(mut self, positions: Vec<usize>) -> Self {
        self.highlight_positions = positions;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn worktrees(mut self, worktrees: Vec<ThreadItemWorktreeInfo>) -> Self {
        self.worktrees = worktrees;
        self
    }

    pub fn project_name(mut self, name: impl Into<SharedString>) -> Self {
        self.project_name = Some(name.into());
        self
    }
}

impl RenderOnce for ThreadRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();

        let row_bg = if self.selected {
            colors.ghost_element_selected
        } else {
            colors.elevated_surface_background
        };
        let hover_bg = colors.ghost_element_hover;
        let active_bg = colors.ghost_element_active;

        let gradient_overlay = GradientFade::new(row_bg, hover_bg, active_bg)
            .width(px(64.0))
            .right(px(0.0))
            .gradient_stop(0.7)
            .group_name("list_item");

        let separator_color = Color::Custom(colors.text_muted.opacity(0.4));
        let dot_separator = || {
            Label::new("•")
                .size(LabelSize::Small)
                .color(separator_color)
        };

        let icon_id = format!("icon-{:?}", self.id);
        let icon_container = || {
            h_flex()
                .id(SharedString::from(icon_id.clone()))
                .size_4()
                .flex_none()
                .justify_center()
        };

        let icon_color = self.icon_color.unwrap_or(Color::Muted);
        let agent_icon = if let Some(svg) = self.custom_icon_from_external_svg {
            Icon::from_external_svg(svg)
                .color(icon_color)
                .size(IconSize::Small)
                .into_any_element()
        } else {
            Icon::new(self.icon)
                .color(icon_color)
                .size(IconSize::Small)
                .into_any_element()
        };

        let status_icon = if self.status == AgentThreadStatus::Error {
            Some(
                Icon::new(IconName::Close)
                    .size(IconSize::Small)
                    .color(Color::Error),
            )
        } else if self.status == AgentThreadStatus::WaitingForConfirmation {
            Some(
                Icon::new(IconName::Warning)
                    .size(IconSize::XSmall)
                    .color(Color::Warning),
            )
        } else if self.notified {
            Some(
                Icon::new(IconName::Circle)
                    .size(IconSize::Small)
                    .color(Color::Accent),
            )
        } else {
            None
        };

        let icon = if self.status == AgentThreadStatus::Running {
            icon_container()
                .child(
                    Icon::new(IconName::LoadCircle)
                        .size(IconSize::Small)
                        .color(Color::Muted)
                        .with_rotate_animation(2),
                )
                .into_any_element()
        } else if let Some(status_icon) = status_icon {
            icon_container().child(status_icon).into_any_element()
        } else {
            icon_container().child(agent_icon).into_any_element()
        };

        let title_label = if self.highlight_positions.is_empty() {
            Label::new(self.title).into_any_element()
        } else {
            HighlightedLabel::new(self.title, self.highlight_positions).into_any_element()
        };

        let has_timestamp = !self.timestamp.is_empty();
        let timestamp = self.timestamp;

        let linked_worktrees: Vec<ThreadItemWorktreeInfo> = self
            .worktrees
            .into_iter()
            .filter(|wt| wt.worktree_name.is_some() || wt.branch_name.is_some())
            .collect();

        let has_worktree = !linked_worktrees.is_empty();
        let has_project_name = self.project_name.is_some();
        let has_metadata = has_project_name || has_worktree || has_timestamp;

        v_flex()
            .id(self.id.clone())
            .relative()
            .flex_shrink_0()
            .overflow_hidden()
            .w_full()
            .child(
                h_flex()
                    .min_w_0()
                    .w_full()
                    .h_6()
                    .gap_2()
                    .justify_between()
                    .child(
                        h_flex()
                            .id("content")
                            .min_w_0()
                            .flex_1()
                            .gap_1p5()
                            .child(icon)
                            .child(title_label),
                    )
                    .child(gradient_overlay),
            )
            .when(has_metadata, |this| {
                this.child(
                    h_flex()
                        .gap_1p5()
                        .child(icon_container())
                        .when_some(self.project_name, |this, name| {
                            this.child(
                                Label::new(name).size(LabelSize::Small).color(Color::Muted),
                            )
                        })
                        .when(has_project_name && has_worktree, |this| {
                            this.child(dot_separator())
                        })
                        .when(has_worktree, |this| {
                            this.children(linked_worktrees.into_iter().map(|wt| {
                                let worktree_label = wt.worktree_name.clone().map(|name| {
                                    if wt.highlight_positions.is_empty() {
                                        Label::new(name)
                                            .size(LabelSize::Small)
                                            .color(Color::Muted)
                                            .truncate()
                                            .into_any_element()
                                    } else {
                                        HighlightedLabel::new(
                                            name,
                                            wt.highlight_positions.clone(),
                                        )
                                        .size(LabelSize::Small)
                                        .color(Color::Muted)
                                        .truncate()
                                        .into_any_element()
                                    }
                                });

                                let chip_icon = if wt.worktree_name.is_none()
                                    && wt.branch_name.is_some()
                                {
                                    IconName::GitBranch
                                } else {
                                    IconName::GitWorktree
                                };

                                let branch_label = wt.branch_name.map(|branch| {
                                    Label::new(branch)
                                        .size(LabelSize::Small)
                                        .color(Color::Muted)
                                        .truncate()
                                        .into_any_element()
                                });

                                let show_separator =
                                    worktree_label.is_some() && branch_label.is_some();

                                h_flex()
                                    .min_w_0()
                                    .gap_0p5()
                                    .child(
                                        Icon::new(chip_icon)
                                            .size(IconSize::XSmall)
                                            .color(Color::Muted),
                                    )
                                    .when_some(worktree_label, |this, label| this.child(label))
                                    .when(show_separator, |this| {
                                        this.child(
                                            Label::new("/")
                                                .size(LabelSize::Small)
                                                .color(separator_color)
                                                .flex_shrink_0(),
                                        )
                                    })
                                    .when_some(branch_label, |this, label| this.child(label))
                            }))
                        })
                        .when((has_project_name || has_worktree) && has_timestamp, |this| {
                            this.child(dot_separator())
                        })
                        .when(has_timestamp, |this| {
                            this.child(
                                Label::new(timestamp)
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            )
                        }),
                )
            })
    }
}
