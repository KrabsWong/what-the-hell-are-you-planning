use crate::app::TaskApp;

#[cfg(feature = "qa")]
pub(crate) static STATUS_COMPUTATIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl TaskApp {
    pub(crate) fn display_task_status(&self, id: u64) -> crate::workflow::TaskStatus {
        if let Some(status) = self.render_statuses.borrow().get(&id) {
            return status.clone();
        }
        #[cfg(feature = "qa")]
        STATUS_COMPUTATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // Goal summaries and visible child rows share the same status projection.
        // The Store's uncached API remains available to admission/business logic.
        let status = self.store.task_status_with_execution(id, self.todo_batch.as_ref(), |step| {
            if let Some(status) = self.render_executions.borrow().get(&step) {
                return *status;
            }
            let status = self.store.execution_status(step, self.todo_batch.as_ref());
            self.render_executions.borrow_mut().insert(step, status);
            status
        });
        // Retry explanations include a wall-clock countdown.
        if status.execution != crate::execution_status::ExecutionStatus::Retrying {
            self.render_statuses.borrow_mut().insert(id, status.clone());
        }
        status
    }

}
