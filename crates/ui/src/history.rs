use std::fmt::Debug;
use std::time::{Duration, Instant};

pub trait HistoryItem: Clone {
    fn version(&self) -> usize;
    fn set_version(&mut self, version: usize);
}

/// The History is used to keep track of changes to a model and to allow undo and redo operations.
///
/// This is now used in Input for undo/redo operations. You can also use this in
/// your own models to keep track of changes, for example to track the tab
/// history for prev/next features.
#[derive(Debug)]
pub struct History<I: HistoryItem> {
    undos: Vec<I>,
    redos: Vec<I>,
    last_changed_at: Instant,
    version: usize,
    pub(crate) ignore: bool,
    max_undo: usize,
    group_interval: Option<Duration>,
}

impl<I> History<I>
where
    I: HistoryItem,
{
    pub fn new() -> Self {
        Self {
            undos: Default::default(),
            redos: Default::default(),
            ignore: false,
            last_changed_at: Instant::now(),
            version: 0,
            max_undo: 1000,
            group_interval: None,
        }
    }

    /// Set the maximum number of undo steps to keep, defaults to 1000.
    pub fn max_undo(mut self, max_undo: usize) -> Self {
        self.max_undo = max_undo;
        self
    }

    /// Set the interval in milliseconds to group changes, defaults to None.
    pub fn group_interval(mut self, group_interval: Duration) -> Self {
        self.group_interval = Some(group_interval);
        self
    }

    /// Increment the version number if the last change was made more than `GROUP_INTERVAL` milliseconds ago.
    fn inc_version(&mut self) -> usize {
        let t = Instant::now();
        if Some(self.last_changed_at.elapsed()) > self.group_interval {
            self.version += 1;
        }

        self.last_changed_at = t;
        self.version
    }

    /// Get the current version number.
    pub fn version(&self) -> usize {
        self.version
    }

    pub fn push(&mut self, item: I) {
        let version = self.inc_version();

        if self.undos.len() >= self.max_undo {
            self.undos.remove(0);
        }

        let mut item = item;
        item.set_version(version);
        self.undos.push(item);
    }

    pub fn undo(&mut self) -> Option<Vec<I>> {
        if let Some(first_change) = self.undos.pop() {
            let mut changes = vec![first_change.clone()];
            // pick the next all changes with the same version
            while self
                .undos
                .iter()
                .filter(|c| c.version() == first_change.version())
                .count()
                > 0
            {
                let change = self.undos.pop().unwrap();
                changes.push(change);
            }

            self.redos.extend(changes.iter().rev().cloned());
            Some(changes)
        } else {
            None
        }
    }

    pub fn redo(&mut self) -> Option<Vec<I>> {
        if let Some(first_change) = self.redos.pop() {
            let mut changes = vec![first_change.clone()];
            // pick the next all changes with the same version
            while self
                .redos
                .iter()
                .filter(|c| c.version() == first_change.version())
                .count()
                > 0
            {
                let change = self.redos.pop().unwrap();
                changes.push(change);
            }
            self.undos.extend(changes.iter().rev().cloned());
            Some(changes)
        } else {
            None
        }
    }
}

impl<I> Default for History<I>
where
    I: HistoryItem,
{
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct TabIndex {
        tab_index: usize,
        version: usize,
    }

    impl From<usize> for TabIndex {
        fn from(value: usize) -> Self {
            TabIndex {
                tab_index: value,
                version: 0,
            }
        }
    }

    impl HistoryItem for TabIndex {
        fn version(&self) -> usize {
            self.version
        }

        fn set_version(&mut self, version: usize) {
            self.version = version;
        }
    }

    #[test]
    fn test_history() {
        let mut history: History<TabIndex> = History::new().max_undo(100);
        history.push(0.into());
        history.push(3.into());
        history.push(2.into());
        history.push(1.into());

        assert_eq!(history.version(), 4);
        let changes = history.undo().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].tab_index, 1);

        let changes = history.undo().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].tab_index, 2);

        history.push(5.into());

        let changes = history.redo().unwrap();
        assert_eq!(changes[0].tab_index, 2);

        let changes = history.redo().unwrap();
        assert_eq!(changes[0].tab_index, 1);

        let changes = history.undo().unwrap();
        assert_eq!(changes[0].tab_index, 1);

        let changes = history.undo().unwrap();
        assert_eq!(changes[0].tab_index, 2);

        let changes = history.undo().unwrap();
        assert_eq!(changes[0].tab_index, 5);

        let changes = history.undo().unwrap();
        assert_eq!(changes[0].tab_index, 3);

        let changes = history.undo().unwrap();
        assert_eq!(changes[0].tab_index, 0);

        assert!(history.undo().is_none());
    }
}
