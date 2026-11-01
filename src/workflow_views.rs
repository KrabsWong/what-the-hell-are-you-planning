use crate::{app::TaskApp, domain::Task, views::*, workflow::ExecutionGroup};
use gpui_kit::{component::menu::{DropdownMenu, PopupMenuItem}, prelude::FluentBuilder, *};

impl TaskApp {
    pub(crate) fn display_task_status(&self, id: u64) -> crate::workflow::TaskStatus {
        if let Some(status) = self.render_statuses.borrow().get(&id) {
            return status.clone();
        }
        let status = self.store.task_status(id, self.todo_batch.as_ref());
        self.render_statuses.borrow_mut().insert(id, status.clone());
        status
    }

    pub(crate) fn execution_status_filter(
        &self,
        tasks: &[Task],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let groups: Vec<_> = tasks
            .iter()
            .map(|t| self.display_task_status(t.id).group)
            .collect();
        let selected = self.board_execution_filter;
        let mut options = vec![(None, format!("All statuses · {}", tasks.len()))];
        options.extend(ExecutionGroup::ALL.into_iter().map(|group| {
            let count = groups.iter().filter(|value| **value == Some(group)).count();
            (Some(group), format!("{} · {count}", group.label()))
        }));
        let title = match selected {
            None => "All statuses",
            Some(ExecutionGroup::Waiting) => "Waiting",
            Some(group) => group.label(),
        };
        let entity = cx.entity().downgrade();
        button("execution-status-filter", title)
            .h(px(28.)).px_2().text_xs().flex_shrink_0()
            .dropdown_caret(true)
            .tooltip("Filter execution status")
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |mut menu, _, _| {
                for (group, title) in &options {
                    let group = *group;
                    let entity = entity.clone();
                    menu = menu.item(PopupMenuItem::new(title.clone())
                        .checked(group == selected)
                        .on_click(move |_, _, cx| {
                            let _ = entity.update(cx, |a, cx| {
                                a.board_execution_filter = group;
                                cx.notify();
                            });
                        }));
                }
                menu
            })
            .into_any_element()
    }

    pub(crate) fn execution_board_cards(
        &self,
        cards: &[Task],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut content = vec![];
        for group in ExecutionGroup::ALL {
            if self
                .board_execution_filter
                .is_some_and(|filter| filter != group)
            {
                continue;
            }
            let matching: Vec<_> = cards
                .iter()
                .filter(|task| {
                    self.display_task_status(task.id)
                        .group
                        == Some(group)
                })
                .collect();
            if matching.is_empty() {
                continue;
            }
            content.push(
                col()
                    .gap_2()
                    .id(format!("execution-group-{}", group.key()))
                    .child(label(format!("{} · {}", group.label(), matching.len()), cx))
                    .children(matching.into_iter().map(|task| self.goal_card(task, cx)))
                    .map(observed)
                    .into_any_element(),
            );
        }
        if content.is_empty() && !cards.is_empty() {
            content.push(label("No tasks match this status.", cx).into_any_element());
        }
        content
    }
}
