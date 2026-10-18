use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Cheap, cloneable, `Send + Sync` view of a cancellation flag. Heavy algorithms check it
/// cooperatively; cancelling never produces a half result as `Ok`.
#[derive(Debug, Clone)]
pub struct CancellationToken {
    flag: Option<Arc<AtomicBool>>,
}

/// Owns a cancellation flag and hands out tokens for it.
#[derive(Debug, Default)]
pub struct CancellationSource {
    flag: Arc<AtomicBool>,
}

/// The work was cancelled through its [`CancellationToken`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("cancelled")
    }
}

impl std::error::Error for Cancelled {}

impl CancellationSource {
    pub fn new() -> Self {
        CancellationSource::default()
    }

    pub fn token(&self) -> CancellationToken {
        CancellationToken { flag: Some(Arc::clone(&self.flag)) }
    }

    /// Idempotent.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Release);
    }
}

impl CancellationToken {
    /// A token that is never cancelled, for synchronous callers and tests.
    pub fn never() -> Self {
        CancellationToken { flag: None }
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.as_ref().is_some_and(|f| f.load(Ordering::Acquire))
    }

    /// For use with `?` inside loops.
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() { Err(Cancelled) } else { Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_source_is_seen_by_every_token_and_is_idempotent() {
        let source = CancellationSource::new();
        let (a, b) = (source.token(), source.token().clone());
        assert!(!a.is_cancelled());
        assert_eq!(b.check(), Ok(()));
        source.cancel();
        source.cancel();
        assert!(a.is_cancelled());
        assert_eq!(b.check(), Err(Cancelled));
        let worker = std::thread::spawn(move || a.is_cancelled());
        assert!(worker.join().unwrap(), "visible from another thread");
    }

    #[test]
    fn never_token_and_other_sources_stay_uncancelled() {
        let one = CancellationSource::new();
        let other = CancellationSource::new();
        one.cancel();
        assert!(!other.token().is_cancelled());
        assert!(!CancellationToken::never().is_cancelled());
    }
}
