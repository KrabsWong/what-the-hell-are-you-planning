//! Internal tracked entity collections for the T14 persistence boundary.
//!
//! Reads expose an immutable slice. Mutation is only through tracked guards or
//! whole-collection methods that record keys/tombstones. There is deliberately
//! no `DerefMut`, `IndexMut`, raw `&mut Vec`, or untracked `iter_mut` escape.

use crate::persistence_journal::{EntityChange, EntityKey, EntityPayload};
use std::collections::BTreeSet;
use std::ops::{Deref, DerefMut, Index};
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub(crate) struct DirtySet {
    upserts: BTreeSet<EntityKey>,
    deletes: BTreeSet<EntityKey>,
    violation: Option<String>,
    /// Monotonic local change counter. A drained group remembers this value so
    /// an acknowledgement cannot clear writes that arrived after the drain.
    epoch: u64,
}

impl DirtySet {
    fn tick(&mut self) -> u64 {
        self.epoch = self.epoch.saturating_add(1);
        self.epoch
    }

    fn mark_upsert(&mut self, key: EntityKey) {
        self.tick();
        self.deletes.remove(&key);
        self.upserts.insert(key);
    }

    fn mark_delete(&mut self, key: EntityKey) {
        self.tick();
        self.upserts.remove(&key);
        self.deletes.insert(key);
    }
}

pub struct TrackedVec<T> {
    /// Shared immutable item storage. Cloning clones the item backing store in
    /// O(1) and still copies the owned dirty set; the first mutation on one clone
    /// copies only that collection's items (`Arc::make_mut`).
    items: Arc<Vec<T>>,
    dirty: DirtySet,
    identity: fn(&T) -> EntityKey,
    payload: fn(&T) -> EntityPayload,
}

impl<T> TrackedVec<T> {
    pub(in crate::domain) fn new(
        identity: fn(&T) -> EntityKey,
        payload: fn(&T) -> EntityPayload,
    ) -> Self {
        Self {
            items: Arc::new(Vec::new()),
            dirty: DirtySet::default(),
            identity,
            payload,
        }
    }

    pub(in crate::domain) fn from_vec(
        items: Vec<T>,
        identity: fn(&T) -> EntityKey,
        payload: fn(&T) -> EntityPayload,
    ) -> Result<Self, String> {
        let mut seen = BTreeSet::new();
        for item in &items {
            let key = (identity)(item);
            if !seen.insert(key) {
                return Err(format!(
                    "loaded data contains duplicate tracked entity identity {key:?}"
                ));
            }
        }
        Ok(Self {
            items: Arc::new(items),
            dirty: DirtySet::default(),
            identity,
            payload,
        })
    }

    pub(crate) fn as_slice(&self) -> &[T] {
        &self.items
    }

    /// Expose one item for mutation. Access itself records the upsert, so a
    /// later early return cannot hide a write; identity changes remain a sticky
    /// violation checked by the next `pending_changes`.
    pub(crate) fn tracked_mut(&mut self, key: EntityKey) -> Option<&mut T>
    where
        T: Clone,
    {
        let index = self.items.iter().position(|item| (self.identity)(item) == key)?;
        self.dirty.mark_upsert(key);
        Some(&mut Arc::make_mut(&mut self.items)[index])
    }

    /// O(1) indexed mutation for callers that already hold a verified index
    /// (for example an id→index map built in one history pass). The identity is
    /// rechecked so a stale index cannot write the wrong entity.
    pub(crate) fn tracked_mut_index(&mut self, index: usize, key: EntityKey) -> Option<&mut T>
    where
        T: Clone,
    {
        if index >= self.items.len() || (self.identity)(&self.items[index]) != key {
            return None;
        }
        self.dirty.mark_upsert(key);
        Arc::make_mut(&mut self.items).get_mut(index)
    }

    pub(crate) fn push_tracked(&mut self, item: T)
    where
        T: Clone,
    {
        let key = (self.identity)(&item);
        if self.items.iter().any(|existing| (self.identity)(existing) == key) {
            self.dirty.violation = Some(format!(
                "tracked collection already contains identity {key:?}"
            ));
            return;
        }
        Arc::make_mut(&mut self.items).push(item);
        self.dirty.mark_upsert(key);
    }

    pub(crate) fn insert_tracked(&mut self, index: usize, item: T)
    where
        T: Clone,
    {
        let key = (self.identity)(&item);
        if self.items.iter().any(|existing| (self.identity)(existing) == key) {
            self.dirty.violation = Some(format!(
                "tracked collection already contains identity {key:?}"
            ));
            return;
        }
        Arc::make_mut(&mut self.items).insert(index, item);
        self.dirty.mark_upsert(key);
        self.mark_reorder_unsupported();
    }

    /// Repair a loaded identity in place, updating the dirty/tombstone sets so
    /// the replacement key is persisted and the old key is removed.
    pub(crate) fn reassign_identity(
        &mut self,
        index: usize,
        key: EntityKey,
        apply: impl FnOnce(&mut T),
    ) -> bool
    where
        T: Clone,
    {
        if self.items.iter().enumerate().any(|(other, item)| {
            other != index && (self.identity)(item) == key
        }) {
            self.dirty.violation = Some(format!(
                "identity repair would duplicate tracked identity {key:?}"
            ));
            return false;
        }
        let Some(item) = Arc::make_mut(&mut self.items).get_mut(index) else {
            return false;
        };
        let old = (self.identity)(item);
        apply(item);
        if (self.identity)(item) != key {
            self.dirty.violation = Some(format!(
                "identity repair produced {:?} instead of {key:?}",
                (self.identity)(item)
            ));
            return false;
        }
        if old != key {
            self.dirty.mark_delete(old);
        }
        self.dirty.mark_upsert(key);
        true
    }

    pub(crate) fn pop_tracked(&mut self) -> Option<T>
    where
        T: Clone,
    {
        let item = Arc::make_mut(&mut self.items).pop()?;
        let key = (self.identity)(&item);
        self.dirty.mark_delete(key);
        Some(item)
    }

    pub(crate) fn remove_tracked(&mut self, key: EntityKey) -> Option<T>
    where
        T: Clone,
    {
        let index = self.items.iter().position(|item| (self.identity)(item) == key)?;
        let item = Arc::make_mut(&mut self.items).remove(index);
        self.dirty.mark_delete(key);
        Some(item)
    }

    pub(crate) fn retain_tracked(&mut self, mut keep: impl FnMut(&T) -> bool)
    where
        T: Clone,
    {
        let mut removed = Vec::new();
        Arc::make_mut(&mut self.items).retain(|item| {
            if keep(item) {
                true
            } else {
                removed.push((self.identity)(item));
                false
            }
        });
        for key in removed {
            self.dirty.mark_delete(key);
        }
    }

    pub(crate) fn clear_tracked(&mut self) {
        for item in self.items.iter() {
            let key = (self.identity)(item);
            self.dirty.mark_delete(key);
        }
        // Replacing the Arc avoids cloning the old Vec merely to empty a clone.
        self.items = Arc::new(Vec::new());
    }

    /// Explicit whole-collection setter. This is not dirty detection: the
    /// caller states the complete new contents. Removed identities become
    /// tombstones and every retained item is an upsert. Duplicate identities
    /// are rejected without mutating the live collection.
    pub(crate) fn replace_tracked(&mut self, items: Vec<T>) {
        let mut seen = BTreeSet::new();
        for item in &items {
            if !seen.insert((self.identity)(item)) {
                self.dirty.violation = Some(format!(
                    "replacement contains duplicate tracked identity {:?}",
                    (self.identity)(item)
                ));
                return;
            }
        }
        for item in self.items.iter() {
            let key = (self.identity)(item);
            if !seen.contains(&key) {
                self.dirty.mark_delete(key);
            }
        }
        for key in &seen {
            self.dirty.mark_upsert(*key);
        }
        self.items = Arc::new(items);
    }

    /// Explicit positional reorder. Persisted order is positional, so every
    /// retained entity is re-upserted at its new position.
    pub(crate) fn swap_tracked(&mut self, first: usize, second: usize) -> bool
    where
        T: Clone,
    {
        if first >= self.items.len() || second >= self.items.len() || first == second {
            return false;
        }
        Arc::make_mut(&mut self.items).swap(first, second);
        for item in self.items.iter() {
            self.dirty.mark_upsert((self.identity)(item));
        }
        true
    }

    pub(crate) fn reverse_tracked(&mut self)
    where
        T: Clone,
    {
        Arc::make_mut(&mut self.items).reverse();
        for item in self.items.iter() {
            self.dirty.mark_upsert((self.identity)(item));
        }
    }

    /// Replace one item without marking a change. Only for hydrating a value
    /// that is already durable (for example a lazily fetched report body).
    pub(crate) fn set_clean(&mut self, index: usize, apply: impl FnOnce(&mut T)) -> bool
    where
        T: Clone,
    {
        let Some(item) = Arc::make_mut(&mut self.items).get_mut(index) else {
            return false;
        };
        apply(item);
        true
    }

    pub(crate) fn epoch(&self) -> u64 {
        self.dirty.epoch
    }

    pub(crate) fn is_pristine(&self) -> bool {
        self.items.is_empty()
            && self.dirty.upserts.is_empty()
            && self.dirty.deletes.is_empty()
            && self.dirty.violation.is_none()
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty.violation.is_some()
            || !self.dirty.upserts.is_empty()
            || !self.dirty.deletes.is_empty()
    }

    /// Snapshot exact dirty payloads/tombstones without clearing. Sticky
    /// violations remain visible until explicit recovery.
    pub(crate) fn pending_changes(&self) -> Result<Vec<EntityChange>, String> {
        if let Some(violation) = &self.dirty.violation {
            return Err(violation.clone());
        }
        let mut changes = Vec::new();
        for key in self.dirty.deletes.iter().copied() {
            changes.push(EntityChange::Delete(key));
        }
        for key in self.dirty.upserts.iter().copied() {
            let item = self
                .items
                .iter()
                .find(|item| (self.identity)(item) == key)
                .ok_or_else(|| {
                    format!("dirty entity {key:?} disappeared before drain; identity changed")
                })?;
            changes.push(EntityChange::Upsert((self.payload)(item)));
        }
        Ok(changes)
    }

    pub(crate) fn clear_dirty(&mut self) {
        self.dirty.upserts.clear();
        self.dirty.deletes.clear();
        self.dirty.violation = None;
    }

    pub(crate) fn mark_reorder_unsupported(&mut self) {
        self.dirty.violation =
            Some("tracked entity reordering is not supported by the incremental backend".into());
    }
}

impl<T> Deref for TrackedVec<T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

impl<T, I> Index<I> for TrackedVec<T>
where
    I: std::slice::SliceIndex<[T]>,
{
    type Output = I::Output;
    fn index(&self, index: I) -> &Self::Output {
        &self.items[index]
    }
}

impl<'a, T> IntoIterator for &'a TrackedVec<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl<T: Clone> Clone for TrackedVec<T> {
    fn clone(&self) -> Self {
        // Ordinary clones preserve pending intent. Clean bootstrap uses
        // `from_vec` explicitly.
        Self {
            items: Arc::clone(&self.items),
            dirty: self.dirty.clone(),
            identity: self.identity,
            payload: self.payload,
        }
    }
}

impl<T: PartialEq> PartialEq for TrackedVec<T> {
    fn eq(&self, other: &Self) -> bool {
        self.items.as_ref() == other.items.as_ref()
    }
}

impl<T: PartialEq> PartialEq<Vec<T>> for TrackedVec<T> {
    fn eq(&self, other: &Vec<T>) -> bool {
        self.items.as_ref() == other
    }
}

impl<T: PartialEq> PartialEq<TrackedVec<T>> for Vec<T> {
    fn eq(&self, other: &TrackedVec<T>) -> bool {
        self == other.items.as_ref()
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for TrackedVec<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackedVec")
            .field("items", &self.items)
            .field("dirty", &self.dirty)
            .finish()
    }
}

impl<T: serde::Serialize> serde::Serialize for TrackedVec<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.items.iter())
    }
}

/// One tracked scalar/config value. The value is readable through `Deref`,
/// but direct assignment is impossible: only `Store` setters and
/// `TrackedScalarMut` (an explicit mutate guard) can change it. Deserialization
/// and clean load start pristine.
pub struct TrackedScalar<T> {
    value: T,
    dirty: bool,
    epoch: u64,
}

impl<T> TrackedScalar<T> {
    pub(in crate::domain) fn new(value: T) -> Self {
        Self {
            value,
            dirty: false,
            epoch: 0,
        }
    }

    /// Clean load: adopt a persisted value without claiming a change.
    pub(crate) fn load(&mut self, value: T) {
        // Generations are monotonic for the lifetime of this value. A load of
        // an existing handle must not rewind the stale-receipt barrier.
        self.value = value;
        self.dirty = false;
    }

    pub(crate) fn get(&self) -> &T {
        &self.value
    }

    pub(in crate::domain) fn set(&mut self, value: T)
    where
        T: PartialEq,
    {
        if self.value != value {
            self.value = value;
            self.mark();
        }
    }

    fn mark(&mut self) {
        self.dirty = true;
        self.epoch = self.epoch.saturating_add(1);
    }

    pub(in crate::domain) fn mutate(&mut self, apply: impl FnOnce(&mut T))
    where
        T: Clone + PartialEq,
    {
        let before = self.value.clone();
        apply(&mut self.value);
        if self.value != before {
            self.mark();
        }
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub(in crate::domain) fn clear_dirty(&mut self) {
        self.dirty = false;
        // `epoch` is intentionally not reset: an acknowledged receipt keeps its
        // identity and can never match a later generation.
    }

    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }

    pub(crate) fn tracked_mut(&mut self) -> TrackedScalarMut<'_, T> {
        TrackedScalarMut { scalar: self }
    }
}

pub struct TrackedScalarMut<'a, T> {
    scalar: &'a mut TrackedScalar<T>,
}

impl<T> Deref for TrackedScalarMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.scalar.value
    }
}

impl<T> DerefMut for TrackedScalarMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.scalar.value
    }
}

impl<T> Drop for TrackedScalarMut<'_, T> {
    fn drop(&mut self) {
        self.scalar.mark();
    }
}

impl<T> Deref for TrackedScalar<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for TrackedScalar<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // Nested-config mutation is a tracked write. Whole-value assignment
        // still cannot target `TrackedScalar<T>` directly.
        self.mark();
        &mut self.value
    }
}

impl<T> AsRef<T> for TrackedScalar<T> {
    fn as_ref(&self) -> &T {
        &self.value
    }
}

impl<T> std::borrow::Borrow<T> for TrackedScalar<T> {
    fn borrow(&self) -> &T {
        &self.value
    }
}

impl<T: Copy> Copy for TrackedScalar<T> {}

impl<T: Clone> Clone for TrackedScalar<T> {
    fn clone(&self) -> Self {
        // Clones preserve pending intent exactly like tracked collections.
        Self {
            value: self.value.clone(),
            dirty: self.dirty,
            epoch: self.epoch,
        }
    }
}

impl<T: Default> Default for TrackedScalar<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: PartialEq> PartialEq for TrackedScalar<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: PartialEq> PartialEq<T> for TrackedScalar<T> {
    fn eq(&self, other: &T) -> bool {
        &self.value == other
    }
}

impl<T: PartialOrd> PartialOrd<T> for TrackedScalar<T> {
    fn partial_cmp(&self, other: &T) -> Option<std::cmp::Ordering> {
        self.value.partial_cmp(other)
    }
}

impl<T: PartialOrd> PartialOrd for TrackedScalar<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl<T: std::fmt::Display> std::fmt::Display for TrackedScalar<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for TrackedScalar<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackedScalar")
            .field("value", &self.value)
            .field("dirty", &self.dirty)
            .finish()
    }
}

impl<T: serde::Serialize> serde::Serialize for TrackedScalar<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value.serialize(serializer)
    }
}

impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for TrackedScalar<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::new(T::deserialize(deserializer)?))
    }
}

/// A metadata-persisted collection (publications, model profiles). These are
/// stored as one JSON array, so an edit marks the whole collection for a single
/// typed `MetadataDelta`; membership itself is never discovered by comparison.
pub struct TrackedList<T> {
    items: Arc<Vec<T>>,
    dirty: bool,
    epoch: u64,
}

impl<T> TrackedList<T> {
    pub(in crate::domain) fn new() -> Self {
        Self {
            items: Arc::new(Vec::new()),
            dirty: false,
            epoch: 0,
        }
    }

    pub(in crate::domain) fn from_vec(items: Vec<T>) -> Self {
        Self {
            items: Arc::new(items),
            dirty: false,
            epoch: 0,
        }
    }

    pub(crate) fn as_slice(&self) -> &[T] {
        &self.items
    }

    fn mark(&mut self) {
        self.dirty = true;
        self.epoch = self.epoch.saturating_add(1);
    }

    pub(crate) fn index_mut(&mut self, index: usize) -> Option<&mut T>
    where
        T: Clone,
    {
        self.mark();
        Arc::make_mut(&mut self.items).get_mut(index)
    }

    pub(crate) fn push_tracked(&mut self, item: T)
    where
        T: Clone,
    {
        Arc::make_mut(&mut self.items).push(item);
        self.mark();
    }

    pub(crate) fn remove_tracked(&mut self, index: usize) -> Option<T>
    where
        T: Clone,
    {
        let item = Arc::make_mut(&mut self.items).remove(index);
        self.mark();
        Some(item)
    }

    pub(crate) fn retain_tracked(&mut self, mut keep: impl FnMut(&T) -> bool)
    where
        T: Clone,
    {
        let before = self.items.len();
        Arc::make_mut(&mut self.items).retain(|item| keep(item));
        if self.items.len() != before {
            self.mark();
        }
    }

    pub(crate) fn replace_tracked(&mut self, items: Vec<T>) {
        self.items = Arc::new(items);
        self.mark();
    }

    pub(crate) fn load(&mut self, items: Vec<T>) {
        self.items = Arc::new(items);
        self.dirty = false;
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub(crate) fn clear_dirty(&mut self) {
        self.dirty = false;
        // Monotonic; never rewound by an acknowledgement.
    }

    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }
}

pub struct TrackedListMut<'a, T> {
    item: &'a mut T,
}

impl<T> Deref for TrackedListMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.item
    }
}

impl<T> DerefMut for TrackedListMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.item
    }
}

impl<T> Deref for TrackedList<T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

impl<T: Clone> DerefMut for TrackedList<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.mark();
        Arc::make_mut(&mut self.items).as_mut_slice()
    }
}

impl<T, I> Index<I> for TrackedList<T>
where
    I: std::slice::SliceIndex<[T]>,
{
    type Output = I::Output;
    fn index(&self, index: I) -> &Self::Output {
        &self.items[index]
    }
}

impl<'a, T> IntoIterator for &'a TrackedList<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl<T: Clone> Clone for TrackedList<T> {
    fn clone(&self) -> Self {
        Self {
            items: Arc::clone(&self.items),
            dirty: self.dirty,
            epoch: self.epoch,
        }
    }
}

impl<T: PartialEq> PartialEq for TrackedList<T> {
    fn eq(&self, other: &Self) -> bool {
        self.items.as_ref() == other.items.as_ref()
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for TrackedList<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackedList")
            .field("items", &self.items)
            .field("dirty", &self.dirty)
            .finish()
    }
}

impl<T: serde::Serialize> serde::Serialize for TrackedList<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.items.iter())
    }
}

pub struct TrackedMut<'a, T> {
    value: &'a mut T,
    key: EntityKey,
    dirty: &'a mut DirtySet,
    identity: fn(&T) -> EntityKey,
}

impl<T> Deref for TrackedMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<T> DerefMut for TrackedMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value
    }
}

impl<T> Drop for TrackedMut<'_, T> {
    fn drop(&mut self) {
        if (self.identity)(self.value) != self.key {
            self.dirty.violation = Some(format!(
                "entity identity changed while tracked: expected {:?}",
                self.key
            ));
            return;
        }
        self.dirty.mark_upsert(self.key);
    }
}

pub(crate) fn task_new() -> TrackedVec<crate::domain::Task> {
    TrackedVec::new(
        |task| EntityKey::Task(task.id),
        |task| EntityPayload::Task(task.clone()),
    )
}

pub(crate) fn project_new() -> TrackedVec<crate::domain::Project> {
    TrackedVec::new(
        |project| EntityKey::Project(project.id),
        |project| EntityPayload::Project(project.clone()),
    )
}

pub(crate) fn topic_new() -> TrackedVec<crate::domain::Topic> {
    TrackedVec::new(
        |topic| EntityKey::Topic(topic.id),
        |topic| EntityPayload::Topic(topic.clone()),
    )
}

pub(crate) fn request_new() -> TrackedVec<crate::request_planning::ResearchRequest> {
    TrackedVec::new(
        |request| EntityKey::ResearchRequest(request.id),
        |request| EntityPayload::ResearchRequest(request.clone()),
    )
}

pub(crate) fn policy_new() -> TrackedVec<crate::domain::StagePolicy> {
    TrackedVec::new(
        |policy| EntityKey::policy(policy.stage),
        |policy| EntityPayload::Policy(policy.clone()),
    )
}

pub(crate) fn task_from(
    items: Vec<crate::domain::Task>,
) -> Result<TrackedVec<crate::domain::Task>, String> {
    TrackedVec::from_vec(
        items,
        |task| EntityKey::Task(task.id),
        |task| EntityPayload::Task(task.clone()),
    )
}

pub(crate) fn project_from(
    items: Vec<crate::domain::Project>,
) -> Result<TrackedVec<crate::domain::Project>, String> {
    TrackedVec::from_vec(
        items,
        |project| EntityKey::Project(project.id),
        |project| EntityPayload::Project(project.clone()),
    )
}

pub(crate) fn topic_from(
    items: Vec<crate::domain::Topic>,
) -> Result<TrackedVec<crate::domain::Topic>, String> {
    TrackedVec::from_vec(
        items,
        |topic| EntityKey::Topic(topic.id),
        |topic| EntityPayload::Topic(topic.clone()),
    )
}

pub(crate) fn request_from(
    items: Vec<crate::request_planning::ResearchRequest>,
) -> Result<TrackedVec<crate::request_planning::ResearchRequest>, String> {
    TrackedVec::from_vec(
        items,
        |request| EntityKey::ResearchRequest(request.id),
        |request| EntityPayload::ResearchRequest(request.clone()),
    )
}

pub(crate) fn policy_from(
    items: Vec<crate::domain::StagePolicy>,
) -> Result<TrackedVec<crate::domain::StagePolicy>, String> {
    TrackedVec::from_vec(
        items,
        |policy| EntityKey::policy(policy.stage),
        |policy| EntityPayload::Policy(policy.clone()),
    )
}

pub(crate) fn automation_new() -> TrackedVec<crate::automation::AutomationDefinition> {
    TrackedVec::new(
        |definition| EntityKey::Automation(definition.id),
        |definition| EntityPayload::Automation(definition.clone()),
    )
}

pub(crate) fn occurrence_new() -> TrackedVec<crate::automation::AutomationOccurrence> {
    TrackedVec::new(
        |occurrence| EntityKey::Occurrence(occurrence.id),
        |occurrence| EntityPayload::Occurrence(occurrence.clone()),
    )
}

pub(crate) fn run_new() -> TrackedVec<crate::domain::AgentRun> {
    TrackedVec::new(
        |run| EntityKey::Run(run.id),
        |run| EntityPayload::Run(run.clone()),
    )
}

pub(crate) fn automation_from(items: Vec<crate::automation::AutomationDefinition>) -> Result<TrackedVec<crate::automation::AutomationDefinition>, String> {
    TrackedVec::from_vec(
        items,
        |definition| EntityKey::Automation(definition.id),
        |definition| EntityPayload::Automation(definition.clone()),
    )
}

pub(crate) fn occurrence_from(items: Vec<crate::automation::AutomationOccurrence>) -> Result<TrackedVec<crate::automation::AutomationOccurrence>, String> {
    TrackedVec::from_vec(
        items,
        |occurrence| EntityKey::Occurrence(occurrence.id),
        |occurrence| EntityPayload::Occurrence(occurrence.clone()),
    )
}

pub(crate) fn run_from(items: Vec<crate::domain::AgentRun>) -> Result<TrackedVec<crate::domain::AgentRun>, String> {
    TrackedVec::from_vec(
        items,
        |run| EntityKey::Run(run.id),
        |run| EntityPayload::Run(run.clone()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Priority, Stage, Task};

    fn task(id: u64, title: &str) -> Task {
        Task {
            id,
            created_at: String::new(),
            automation_id: None,
            title: title.into(),
            description: String::new(),
            project_id: 1,
            topic_id: None,
            approved_plan: None,
            stage: Stage::Backlog,
            priority: Priority::None,
            labels: Vec::new(),
            cycle: None,
            bucket: String::new(),
            due_date: String::new(),
            parent_id: None,
            comments: Vec::new(),
            activity: Vec::new(),
            archived: false,
            attachments: Vec::new(),
            research: None,
        }
    }

    fn tasks() -> TrackedVec<Task> {
        TrackedVec::new(
            |task| EntityKey::Task(task.id),
            |task| EntityPayload::Task(task.clone()),
        )
    }

    #[test]
    fn clone_shares_items_until_one_side_mutates() {
        let mut base = tasks();
        base.push_tracked(task(1, "one"));
        base.push_tracked(task(2, "two"));
        base.clear_dirty();
        let mut clone = base.clone();
        assert!(
            Arc::ptr_eq(&base.items, &clone.items),
            "clone must share the immutable Vec allocation"
        );
        clone.tracked_mut(EntityKey::Task(1)).unwrap().title = "clone changed".into();
        assert_eq!(base[0].title, "one", "base must remain isolated");
        assert_eq!(clone[0].title, "clone changed");
        assert!(
            !Arc::ptr_eq(&base.items, &clone.items),
            "first mutation must take a private copy"
        );
        // A clean clone with no mutation keeps sharing.
        let untouched = base.clone();
        assert!(Arc::ptr_eq(&base.items, &untouched.items));
    }

    #[test]
    fn clone_has_owned_dirty_epochs_and_tombstones() {
        let mut base = tasks();
        base.push_tracked(task(1, "one"));
        base.push_tracked(task(2, "two"));
        base.clear_dirty();
        let mut clone = base.clone();
        let base_epoch = base.epoch();
        clone.remove_tracked(EntityKey::Task(2)).unwrap();
        assert_eq!(base.len(), 2);
        assert_eq!(clone.len(), 1);
        assert!(base.pending_changes().unwrap().is_empty());
        assert_eq!(
            clone.pending_changes().unwrap(),
            vec![EntityChange::Delete(EntityKey::Task(2))]
        );
        assert!(clone.epoch() > base_epoch);
        assert_eq!(base.epoch(), base_epoch, "clone mutation must not advance base epoch");
    }

    #[test]
    fn identity_violation_is_clone_local() {
        let mut base = tasks();
        base.push_tracked(task(1, "one"));
        base.clear_dirty();
        let mut clone = base.clone();
        clone.push_tracked(task(1, "duplicate"));
        assert!(clone.pending_changes().is_err());
        assert_eq!(base.len(), 1);
        assert!(base.pending_changes().unwrap().is_empty());
        assert!(base.pending_changes().is_ok());
    }

    #[test]
    fn clone_serialization_matches_wire_format() {
        let mut base = tasks();
        base.push_tracked(task(1, "one"));
        base.push_tracked(task(2, "two"));
        base.clear_dirty();
        let clone = base.clone();
        assert_eq!(
            serde_json::to_string(&base).unwrap(),
            serde_json::to_string(&clone).unwrap()
        );
        let mut changed = clone;
        changed.tracked_mut(EntityKey::Task(1)).unwrap().title = "changed".into();
        assert_ne!(
            serde_json::to_string(&base).unwrap(),
            serde_json::to_string(&changed).unwrap()
        );
    }

    #[test]
    fn clean_hydration_and_replacement_do_not_touch_the_other_clone() {
        let mut base = tasks();
        base.push_tracked(task(1, "one"));
        base.push_tracked(task(2, "two"));
        base.clear_dirty();
        let mut clone = base.clone();
        assert!(clone.set_clean(0, |task| task.title = "hydrated".into()));
        assert_eq!(clone[0].title, "hydrated");
        assert_eq!(base[0].title, "one");
        assert!(!clone.is_dirty(), "clean hydration must not mark dirty");
        assert!(!base.is_dirty());
        clone.replace_tracked(vec![task(3, "three")]);
        assert_eq!(base.len(), 2);
        assert_eq!(clone.len(), 1);
        assert_eq!(
            clone.pending_changes().unwrap(),
            vec![
                EntityChange::Delete(EntityKey::Task(1)),
                EntityChange::Delete(EntityKey::Task(2)),
                EntityChange::Upsert(EntityPayload::Task(task(3, "three"))),
            ]
        );
    }

    #[test]
    fn clone_retain_reorder_and_delete_are_isolated() {
        let mut base = tasks();
        for id in 1..=3 {
            base.push_tracked(task(id, &format!("task {id}")));
        }
        base.clear_dirty();
        let mut clone = base.clone();
        assert!(clone.swap_tracked(0, 2));
        clone.retain_tracked(|task| task.id != 2);
        assert_eq!(base.iter().map(|task| task.id).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(clone.iter().map(|task| task.id).collect::<Vec<_>>(), vec![3, 1]);
        assert!(clone
            .pending_changes()
            .unwrap()
            .contains(&EntityChange::Delete(EntityKey::Task(2))));
        assert!(base.pending_changes().unwrap().is_empty());
    }

    #[test]
    fn tracked_list_clone_is_copy_on_write_and_keeps_epochs() {
        let mut base = TrackedList::from_vec(vec![10u64, 20, 30]);
        let mut clone = base.clone();
        assert!(Arc::ptr_eq(&base.items, &clone.items));
        clone.push_tracked(40);
        assert_eq!(base.as_slice(), &[10, 20, 30]);
        assert_eq!(clone.as_slice(), &[10, 20, 30, 40]);
        assert!(!Arc::ptr_eq(&base.items, &clone.items));
        assert!(clone.is_dirty());
        assert!(!base.is_dirty());
        assert_eq!(serde_json::to_string(&clone).unwrap(), "[10,20,30,40]");
        assert_eq!(serde_json::to_string(&base).unwrap(), "[10,20,30]");
        base.push_tracked(50);
        assert_eq!(base.as_slice(), &[10, 20, 30, 50]);
        assert_eq!(clone.as_slice(), &[10, 20, 30, 40]);
    }

    #[test]
    fn identity_repair_rejects_collision_before_mutating() {
        let mut values = tasks();
        values.push_tracked(task(1, "first"));
        values.push_tracked(task(2, "second"));
        values.clear_dirty();
        assert!(!values.reassign_identity(0, EntityKey::Task(2), |task| {
            task.id = 2;
            task.title = "must not apply".into();
        }));
        assert_eq!(values[0].id, 1);
        assert_eq!(values[0].title, "first");
        assert_eq!(values[1].id, 2);
        assert!(values.pending_changes().is_err());
    }

    #[test]
    fn duplicate_identity_is_rejected_without_mutating_the_live_vec() {
        let mut vec = tasks();
        vec.push_tracked(task(1, "first"));
        vec.clear_dirty();
        vec.push_tracked(task(1, "replacement"));
        assert_eq!(vec.len(), 1, "duplicate was inserted");
        assert!(vec.pending_changes().is_err(), "duplicate must be sticky");
        assert_eq!(vec[0].title, "first");
    }

    #[test]
    fn clean_load_does_not_mark_dirty_and_clone_preserves_intent() {
        let mut vec = TrackedVec::from_vec(
            vec![task(1, "loaded")],
            |task| EntityKey::Task(task.id),
            |task| EntityPayload::Task(task.clone()),
        )
        .unwrap();
        assert!(vec.pending_changes().unwrap().is_empty());
        let mut visible = vec.tracked_mut(EntityKey::Task(1)).unwrap();
        visible.title = "changed".into();
        drop(visible);
        let cloned = vec.clone();
        assert_eq!(cloned.pending_changes().unwrap().len(), 1);
        assert_eq!(vec.pending_changes().unwrap().len(), 1);
    }

    #[test]
    fn identity_change_is_sticky_until_explicit_clear() {
        let mut vec = TrackedVec::from_vec(
            vec![task(1, "loaded")],
            |task| EntityKey::Task(task.id),
            |task| EntityPayload::Task(task.clone()),
        )
        .unwrap();
        {
            let mut guard = vec.tracked_mut(EntityKey::Task(1)).unwrap();
            guard.id = 2;
        }
        assert!(vec.pending_changes().is_err());
        assert!(vec.pending_changes().is_err(), "violation must remain sticky");
        vec.clear_dirty();
        assert!(vec.pending_changes().is_ok());
    }

    #[test]
    fn replacement_marks_tombstones_and_retained_upserts() {
        let mut vec = TrackedVec::from_vec(
            vec![task(1, "first"), task(2, "second")],
            |task| EntityKey::Task(task.id),
            |task| EntityPayload::Task(task.clone()),
        )
        .unwrap();
        vec.replace_tracked(vec![task(1, "renamed"), task(3, "third")]);
        let changes = vec.pending_changes().unwrap();
        assert!(changes.contains(&EntityChange::Delete(EntityKey::Task(2))));
        assert!(changes.contains(&EntityChange::Upsert(EntityPayload::Task(task(
            1, "renamed"
        )))));
        assert!(changes.contains(&EntityChange::Upsert(EntityPayload::Task(task(
            3, "third"
        )))));
    }

    #[test]
    fn replacement_with_duplicate_identity_changes_nothing() {
        let mut vec = TrackedVec::from_vec(
            vec![task(1, "first")],
            |task| EntityKey::Task(task.id),
            |task| EntityPayload::Task(task.clone()),
        )
        .unwrap();
        vec.replace_tracked(vec![task(2, "second"), task(2, "duplicate")]);
        assert_eq!(vec.len(), 1);
        assert_eq!(vec[0].id, 1);
        assert!(vec.is_dirty(), "duplicate replacement is a sticky violation");
        assert!(vec.pending_changes().is_err());
    }

    #[test]
    fn loaded_duplicate_identity_fails_construction() {
        let result = TrackedVec::from_vec(
            vec![task(1, "first"), task(1, "duplicate")],
            |task| EntityKey::Task(task.id),
            |task| EntityPayload::Task(task.clone()),
        );
        assert!(result.is_err(), "duplicate loaded identities must fail");
    }
}
