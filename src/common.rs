use futures::stream::{FuturesOrdered, FuturesUnordered};
use futures::Future;

pub trait UnorderedPassthrough<F>
where
    F: Future,
{
    /// Change the internal `max_concurrent` capacity at runtime.
    ///
    /// **Note**: The new limit won't be applied until `poll_next()`
    /// method returns `Poll::Ready(T)`.
    fn set_capacity(&mut self, max_concurrent: usize);

    /// Borrow the inner `FuturesUnordered`.
    fn borrow_inner(&self) -> &FuturesUnordered<F>;

    /// Mutably borrow the inner `FuturesUnordered`.
    /// Note that this library might not work reliably if you
    /// modify the underlying `FuturesUnordered`.
    fn borrow_mut_inner(&mut self) -> &mut FuturesUnordered<F>;

    /// Consume the `FuturesUnorderedIter` and return the
    /// inner `FuturesUnordered`.
    fn into_inner(self) -> FuturesUnordered<F>
    where
        Self: Sized;

    /// Convenience method for the inner `FuturesUnordered`.
    fn len_inner(&self) -> usize {
        self.borrow_inner().len()
    }

    /// Convenience  method for the inner `FuturesUnordered`.
    fn is_empty_inner(&self) -> bool {
        self.borrow_inner().is_empty()
    }

    /// Convenience method for the inner `FuturesUnordered`.
    fn clear_inner(&mut self) {
        self.borrow_mut_inner().clear();
    }

    /// Pass-through method for the inner `FuturesUnordered`.
    ///
    /// Warning: This method will add futures to the internal
    /// `FuturesUnordered`, completely bypassing the `max_concurrent`
    /// limit. You might as well use a vanilla `FuturesUnordered`
    /// in that case.
    fn push_inner(&mut self, future: F) {
        self.borrow_mut_inner().push(future);
    }
}

pub trait OrderedPassthrough<F>
where
    F: Future,
{
    /// Change the internal `max_concurrent` capacity at runtime.
    ///
    /// **Note**: The new limit won't be applied until `poll_next()`
    /// method returns `Poll::Ready(T)`.
    fn set_capacity(&mut self, max_concurrent: usize);

    /// Borrow the inner `FuturesOrdered`.
    fn borrow_inner(&self) -> &FuturesOrdered<F>;

    /// Mutably borrow the inner `FuturesOrdered`.
    /// Note that this library might not work reliably if you
    /// modify the underlying `FuturesUnordered`.
    fn borrow_mut_inner(&mut self) -> &mut FuturesOrdered<F>;

    /// Consume the `FuturesOrderedIter` and return the
    /// inner `FuturesOrdered`.
    fn into_inner(self) -> FuturesOrdered<F>
    where
        Self: Sized;

    /// Convenience method for the inner `FuturesOrdered`.
    fn len_inner(&self) -> usize {
        self.borrow_inner().len()
    }

    /// Convenience  method for the inner `FuturesOrdered`.
    fn is_empty_inner(&self) -> bool {
        self.borrow_inner().is_empty()
    }
}
