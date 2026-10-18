use std::fmt;

/// Identity of one immutable document state within a session. Strictly increasing:
/// every committed command, undo and redo yields a newer revision, so background results
/// computed for an older revision can be recognised and rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RevisionId(u64);

impl RevisionId {
    pub const INITIAL: RevisionId = RevisionId(0);

    pub fn from_raw(raw: u64) -> Self {
        RevisionId(raw)
    }

    pub fn raw(self) -> u64 {
        self.0
    }

    /// The next revision, or `None` when the counter is exhausted.
    pub fn next(self) -> Option<RevisionId> {
        self.0.checked_add(1).map(RevisionId)
    }
}

impl fmt::Display for RevisionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "r{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_successor_is_strictly_newer() {
        let r0 = RevisionId::INITIAL;
        let r1 = r0.next().unwrap();
        let r2 = r1.next().unwrap();
        assert!(r0 < r1 && r1 < r2);
        assert_eq!(RevisionId::from_raw(u64::MAX).next(), None);
    }
}
