//! Module containing traits used across most times.

use futures::Future;

/// Provides common methods across unordered and ordered
/// implementations.
pub trait Passthrough<F>
where
    F: Future,
{
    /// The type of the inner `Stream`. This can be either
    /// `FuturesUnordered` or `FuturesOrdered`.
    type FuturesHolder;

    /// Change the internal `max_concurrent` capacity at runtime.
    ///
    /// **IMPORTANT**: The new limit won't be applied until `poll_next()`
    /// method returns `Poll::Ready(T)`.
    fn set_max_concurrent(&mut self, max_concurrent: usize);

    /// Borrow the inner `Stream`.
    fn borrow_inner(&self) -> &Self::FuturesHolder;

    /// Mutably borrow the inner `Stream`.
    /// Note that this library might not work reliably if you
    /// modify the underlying `Stream`.
    fn borrow_mut_inner(&mut self) -> &mut Self::FuturesHolder;

    /// Consume the iterator and return the inner `Stream`.
    fn into_inner(self) -> Self::FuturesHolder
    where
        Self: Sized;
}
