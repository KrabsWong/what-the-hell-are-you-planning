//! T13 scoped settings foundation for execution-limit changes.

use crate::domain::Store;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsChange {
    ExecutionConcurrency(usize),
    AutomationConcurrency(usize),
    Appearance(crate::appearance::Appearance),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SettingsUndo {
    pub before: SettingsChange,
    pub after: SettingsChange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsError {
    Rejected(&'static str),
    NoChange,
}

pub(crate) fn apply_settings_change(
    store: &mut Store,
    change: SettingsChange,
) -> Result<SettingsUndo, SettingsError> {
    let before = match change {
        SettingsChange::ExecutionConcurrency(limit) => {
            let limit = limit.clamp(1, crate::scheduler::MAX_CONCURRENT_RUNS);
            if store.max_concurrent_runs() == limit {
                return Err(SettingsError::NoChange);
            }
            let before = store.max_concurrent_runs();
            store.set_max_concurrent_runs(limit);
            SettingsChange::ExecutionConcurrency(before)
        }
        SettingsChange::AutomationConcurrency(limit) => {
            let limit = limit.clamp(1, crate::scheduler::MAX_CONCURRENT_RUNS);
            if store.max_concurrent_automations() == limit {
                return Err(SettingsError::NoChange);
            }
            let before = store.max_concurrent_automations();
            store.set_max_concurrent_automations(limit);
            SettingsChange::AutomationConcurrency(before)
        }
        SettingsChange::Appearance(mode) => {
            if store.appearance() == mode {
                return Err(SettingsError::NoChange);
            }
            let before = store.appearance();
            store.set_appearance(mode);
            SettingsChange::Appearance(before)
        }
    };
    Ok(SettingsUndo {
        before,
        after: change,
    })
}

pub(crate) fn undo_settings_change(
    store: &mut Store,
    undo: &SettingsUndo,
) -> Result<(), SettingsError> {
    match undo.before {
        SettingsChange::ExecutionConcurrency(limit) => {
            store.set_max_concurrent_runs(limit.clamp(1, crate::scheduler::MAX_CONCURRENT_RUNS));
        }
        SettingsChange::AutomationConcurrency(limit) => {
            store.set_max_concurrent_automations(limit.clamp(1, crate::scheduler::MAX_CONCURRENT_RUNS));
        }
        SettingsChange::Appearance(mode) => {
            store.set_appearance(mode);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_undo_restores_only_the_changed_limit() {
        let mut store = Store::empty();
        store.set_max_concurrent_runs(3);
        store.set_max_concurrent_automations(2);
        let undo =
            apply_settings_change(&mut store, SettingsChange::ExecutionConcurrency(5)).unwrap();
        assert_eq!(store.max_concurrent_runs(), 5);
        assert_eq!(store.max_concurrent_automations(), 2);
        undo_settings_change(&mut store, &undo).unwrap();
        assert_eq!(store.max_concurrent_runs(), 3);
        assert_eq!(store.max_concurrent_automations(), 2);
    }

    #[test]
    fn appearance_undo_restores_only_theme() {
        let mut store = Store::empty();
        store.set_appearance(crate::appearance::Appearance::Light);
        let undo = apply_settings_change(
            &mut store,
            SettingsChange::Appearance(crate::appearance::Appearance::Dark),
        )
        .unwrap();
        assert_eq!(store.appearance(), crate::appearance::Appearance::Dark);
        undo_settings_change(&mut store, &undo).unwrap();
        assert_eq!(store.appearance(), crate::appearance::Appearance::Light);
    }

    #[test]
    fn same_limit_is_a_no_op() {
        let mut store = Store::empty();
        store.set_max_concurrent_runs(3);
        assert_eq!(
            apply_settings_change(&mut store, SettingsChange::ExecutionConcurrency(3)),
            Err(SettingsError::NoChange)
        );
        assert_eq!(store.max_concurrent_runs(), 3);
    }
}
