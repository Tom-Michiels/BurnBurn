use std::fmt;

/// A typed stable identity. IDs are allocated deterministically by an [`IdAllocator`];
/// they are never random and never reused within a project.
pub trait StableId: Copy + Ord + fmt::Debug {
    fn from_raw(raw: u64) -> Self;
    fn raw(self) -> u64;
}

macro_rules! stable_id {
    ($(#[$doc:meta])* $name:ident, $prefix:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u64);

        impl StableId for $name {
            fn from_raw(raw: u64) -> Self {
                $name(raw)
            }
            fn raw(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }
    };
}

stable_id!(
    /// Identity of a geometry object in a document; survives edits, undo, save and reopen.
    GeometryId,
    "g"
);
stable_id!(
    /// Identity of a process operation (IM13); survives save and reopen.
    OperationId,
    "op"
);

stable_id!(
    /// Identity of a group of objects (IM09); shares the document id space with geometry and
    /// operations, so a group id is never reused.
    GroupId,
    "grp"
);

stable_id!(
    /// One open document session in this process. Not persisted (a file's identity lives in io).
    DocumentId,
    "doc"
);
stable_id!(
    /// Correlates a UI intent / document command with the outcome it produced.
    RequestId,
    "req"
);

stable_id!(
    /// One background task submission. A resubmission gets a new TaskId. Not persisted.
    TaskId,
    "task"
);

/// Deterministic, project-wide ID source. One counter is shared by all ID kinds, so a raw
/// value identifies at most one entity of any kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdAllocator {
    next: u64,
}

/// The allocator ran out of 64-bit identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdExhausted;

impl fmt::Display for IdExhausted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("stable id space exhausted")
    }
}

impl std::error::Error for IdExhausted {}

impl Default for IdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl IdAllocator {
    /// A fresh allocator; the first id is 1.
    pub fn new() -> Self {
        IdAllocator { next: 1 }
    }

    /// Resume after loading: the next id handed out is `next_raw` (stored in the project).
    pub fn resume_at(next_raw: u64) -> Self {
        IdAllocator { next: next_raw.max(1) }
    }

    /// The raw value the next allocation will return (persisted by `burnburn-io`).
    pub fn peek_next(&self) -> u64 {
        self.next
    }

    pub fn allocate<I: StableId>(&mut self) -> Result<I, IdExhausted> {
        if self.next == u64::MAX {
            return Err(IdExhausted);
        }
        let raw = self.next;
        self.next += 1;
        Ok(I::from_raw(raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_deterministic_and_never_reused() {
        let mut a = IdAllocator::new();
        let mut b = IdAllocator::new();
        let from_a: Vec<GeometryId> = (0..3).map(|_| a.allocate().unwrap()).collect();
        let from_b: Vec<GeometryId> = (0..3).map(|_| b.allocate().unwrap()).collect();
        assert_eq!(from_a, from_b, "same sequence without randomness");
        assert_eq!(from_a.iter().map(|g| g.raw()).collect::<Vec<_>>(), [1, 2, 3]);

        let op: OperationId = a.allocate().unwrap();
        assert_eq!(op.raw(), 4, "kinds share one counter");

        let mut resumed = IdAllocator::resume_at(a.peek_next());
        let next: GeometryId = resumed.allocate().unwrap();
        assert!(next.raw() > op.raw(), "resumed allocator does not reuse ids");

        let mut full = IdAllocator::resume_at(u64::MAX);
        assert_eq!(full.allocate::<GeometryId>(), Err(IdExhausted));
    }
}
