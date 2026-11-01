use crate::{app::TaskApp, domain::Task, views::*, workflow::ExecutionGroup};
use gpui_kit::{component::Selectable, prelude::FluentBuilder, *};

impl TaskApp {
    pub(crate) fn execution_filter_bar(
        &self,
        tasks: &[Task],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let groups: Vec<_> = tasks
            .iter()
            .map(|t| self.store.task_status(t.id, self.todo_batch.as_ref()).group)
            .collect();
        col()
            .px_1()
            .gap_2()
            .child(label("Waiting → Queued → Running → Done", cx))
            .child(
                row()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        quiet_button("execution-filter-all", format!("All {}", tasks.len()))
                            .selected(self.board_execution_filter.is_none())
                            .on_click(cx.listener(|a, _, _, cx| {
                                a.board_execution_filter = None;
                                cx.notify();
                            })),
                    )
                    .children(ExecutionGroup::ALL.into_iter().map(|group| {
                        let count = groups.iter().filter(|value| **value == Some(group)).count();
                        quiet_button(
                            format!("execution-filter-{}", group.key()),
                            format!("{} {count}", group.label()),
                        )
                        .selected(self.board_execution_filter == Some(group))
                        .on_click(cx.listener(move |a, _, _, cx| {
                            a.board_execution_filter = Some(group);
                            cx.notify();
                        }))
                    })),
            )
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
                    self.store
                        .task_status(task.id, self.todo_batch.as_ref())
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
        if content.is_empty() {
            content.push(label("No tasks in this category. Dropping here still submits execution; the scheduler chooses the status.", cx).into_any_element());
        }
        content
    }
}
