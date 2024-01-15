//! Bounded replacements for `FuturesUnordered`.

use std::borrow::BorrowMut;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::stream::{FuturesUnordered, Stream};
use futures::Future;
use pin_project::pin_project;

use crate::common::UnorderedPassthrough;

/// Bounded FuturesUnordered from an iterator.
///
/// This struct consumes an iterator where each item
/// is a future. By using an iterator, we need no additional state,
/// or additional queues to store the excess of futures.
///
/// This should be always preferred over `FuturesUnorderedBounded`,
/// as it's more performant and adds basically ZERO overhead on top
/// of the normal `FuturesUnordered`.
#[pin_project]
pub struct FuturesUnorderedIter<T, F>
where
    F: Future,
    T: Iterator<Item = F>,
{
    max_concurrent: usize,
    tasks: T,
    #[pin]
    pub running_tasks: FuturesUnordered<F>,
}

/// Drop-in replacement for `FuturesUnordered`.
///
/// This struct behaves exactly like the normal `FuturesUnordered`,
/// and allows a 100.0% drop-in replacement for `FuturesUnordered`.
///
/// Unlike `FuturesUnorderedIter`, this struct allows you to continuously
/// push futures into it, but it'll only poll N futures at any given time.
/// This struct internally uses a `Vec` to store the excess of futures.
#[pin_project]
pub struct FuturesUnorderedBounded<F>
where
    F: Future,
{
    max_concurrent: usize,
    queued_tasks: Vec<F>,
    #[pin]
    running_tasks: FuturesUnordered<F>,
}

impl<T, F> UnorderedPassthrough<F> for FuturesUnorderedIter<T, F>
where
    T: Iterator<Item = F>,
    F: Future,
{
    fn set_capacity(&mut self, max_concurrent: usize) {
        self.max_concurrent = max_concurrent;
    }

    fn borrow_inner(&self) -> &FuturesUnordered<F> {
        &self.running_tasks
    }

    fn borrow_mut_inner(&mut self) -> &mut FuturesUnordered<F> {
        self.running_tasks.borrow_mut()
    }

    fn into_inner(self) -> FuturesUnordered<F>
    where
        Self: Sized,
    {
        self.running_tasks
    }
}

impl<F> UnorderedPassthrough<F> for FuturesUnorderedBounded<F>
where
    F: Future,
{
    fn set_capacity(&mut self, max_concurrent: usize) {
        self.max_concurrent = max_concurrent;
    }

    fn borrow_inner(&self) -> &FuturesUnordered<F> {
        &self.running_tasks
    }

    fn borrow_mut_inner(&mut self) -> &mut FuturesUnordered<F> {
        self.running_tasks.borrow_mut()
    }

    fn into_inner(self) -> FuturesUnordered<F>
    where
        Self: Sized,
    {
        self.running_tasks
    }
}

impl<T, F> Stream for FuturesUnorderedIter<T, F>
where
    T: Iterator<Item = F>,
    F: Future,
{
    type Item = F::Output;
    /// Perform the same thing as `FuturesUnordered.poll_next()`,
    /// except we only poll `max_concurrent` futures at a time.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        match this.running_tasks.as_mut().poll_next(cx) {
            Poll::Ready(Some(value)) => {
                while this.running_tasks.len() < *this.max_concurrent {
                    match this.tasks.next() {
                        Some(future) => this.running_tasks.push(future),
                        _ => break,
                    }
                }
                Poll::Ready(Some(value))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<F> futures::stream::Stream for FuturesUnorderedBounded<F>
where
    F: Future,
{
    type Item = F::Output;

    /// We do the same thing here as in `FuturesUnorederedIter` , except
    /// we pop new futures from the internal queue.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        match this.running_tasks.as_mut().poll_next(cx) {
            Poll::Ready(Some(value)) => {
                while this.running_tasks.len() < *this.max_concurrent {
                    match this.queued_tasks.pop() {
                        Some(future) => this.running_tasks.push(future),
                        _ => break,
                    }
                }
                Poll::Ready(Some(value))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T, F> FuturesUnorderedIter<T, F>
where
    F: Future,
    T: Iterator<Item = F>,
{
    /// Creates a new rate-limited `FuturesUnordered` from an iterator.
    /// This option is as efficient as it gets since it completely avoids
    /// allocating a new vector to store the excess of futures.
    ///
    /// Since iterators are lazily evaluated, futures will be created on the
    /// fly as well.
    ///
    /// Panics if `max_concurrent` is 0.
    /// ```rust
    /// use futures_bounded::unordered::FuturesUnorderedIter;
    /// use futures_bounded::common::UnorderedPassthrough;
    /// use futures::StreamExt;
    ///
    /// async fn dummy() -> u64{
    ///     42
    /// }
    ///
    /// let tasks = (0..100).into_iter().map(|_| dummy());
    /// let mut fut_unordered = FuturesUnorderedIter::new(5, tasks);
    /// // The internal FuturesUnorderedBounded will have 5 futures at most.
    /// // All other remaining futures will be stored in the internal queue,
    /// // and will be consumed as needed.
    ///
    /// tokio_test::block_on(async move{
    ///     assert_eq!(fut_unordered.len_inner(), 5);
    ///
    ///     while let Some(value) = fut_unordered.next().await {
    ///         println!("{}", value);
    ///         assert!(fut_unordered.len_inner() <= 5);
    ///     }
    ///
    ///     assert_eq!(fut_unordered.len_inner(), 0);
    ///     assert!(fut_unordered.is_empty_inner());
    ///
    /// });
    ///
    /// ```
    pub fn new<I: IntoIterator<IntoIter = T>>(max_concurrent: usize, tasks: I) -> Self {
        let running_tasks = FuturesUnordered::new();
        assert!(max_concurrent > 0, "max_concurrent must be greater than 0");
        let mut tasks = tasks.into_iter();
        // Immediately put futures in the queue
        tasks
            .borrow_mut()
            .take(max_concurrent)
            .for_each(|future| running_tasks.push(future));

        Self {
            max_concurrent,
            tasks,
            running_tasks,
        }
    }
}

impl<F> FuturesUnorderedBounded<F>
where
    F: Future,
{
    /// Creates a new rate-limited `FuturesUnordered`. This struct
    /// is 100% compatible with the normal `FuturesUnordered`.
    ///
    /// Panics if `max_concurrent` is 0.
    /// ```rust
    /// use futures_bounded::unordered::FuturesUnorderedBounded;
    /// use futures_bounded::common::UnorderedPassthrough;
    /// use futures::StreamExt;
    ///
    /// async fn dummy() -> u64{
    ///     42
    /// }
    ///
    /// let mut fut_unordered = FuturesUnorderedBounded::new(5);
    /// for _ in 0..10 {
    ///     fut_unordered.push(dummy());
    /// }
    /// // The internal FuturesUnorderedBounded will have 5 futures at most.
    /// // All other remaining futures will be stored in the internal queue,
    /// // and will be consumed as needed.
    ///
    /// tokio_test::block_on(async move{
    ///     assert_eq!(fut_unordered.len_inner(), 5);
    ///
    ///     while let Some(value) = fut_unordered.next().await {
    ///         println!("{}", value);
    ///         assert!(fut_unordered.len_inner() <= 5);
    ///     }
    ///
    ///     assert_eq!(fut_unordered.len_inner(), 0);
    ///     assert!(fut_unordered.is_empty_inner());
    ///
    /// });
    ///
    /// ```
    pub fn new(max_concurrent: usize) -> Self {
        let running_tasks = FuturesUnordered::new();
        assert!(max_concurrent > 0, "max_concurrent must be greater than 0");
        Self {
            max_concurrent,
            queued_tasks: Vec::new(),
            running_tasks,
        }
    }

    /// Enqueues a new future into the `FuturesUnorderedBounded`.
    ///
    /// If more than `max_concurrent` futures are already in the
    /// internal queue, the excess of futures will be stored in another
    /// queue and will only get pulled as needed.
    pub fn push(&mut self, fut: F) {
        match self.len_inner() < self.max_concurrent {
            true => self.borrow_mut_inner().push(fut),
            false => self.queued_tasks.push(fut),
        }
    }

    /// Mutably borrow the internal queue task list.
    pub fn borrow_mut_queue(&mut self) -> &mut Vec<F> {
        &mut self.queued_tasks
    }

    /// Borrow the internal queue task list.
    pub fn borrow_queue(&self) -> &Vec<F> {
        &self.queued_tasks
    }

    /// Clear the internal queue.
    /// This will clear both the queue used to store the
    /// excess of futures and the internal queue as well.
    pub fn clear_queue(&mut self) {
        self.queued_tasks.clear();
        self.borrow_mut_inner().clear();
    }

    /// Return the length of the inner queue.
    pub fn len_queue(&self) -> usize {
        self.queued_tasks.len()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cmp,
        sync::{atomic::AtomicU8, Arc},
        time::Duration,
    };

    use futures::StreamExt;

    use super::*;

    fn create_tasks() -> Vec<impl Future<Output = u64>> {
        const MAX_FUTURES: u64 = 100;
        let tasks = (0..MAX_FUTURES).map(|i| dummy(i)).collect::<Vec<_>>();
        tasks
    }

    /// Dummy function that receives and
    /// returns a `u64` value.
    async fn dummy(val: u64) -> u64 {
        val
    }

    /// Dummy function that increments and decrements a counter.
    /// This is useful to check if more than N tasks are running concurrently.
    async fn dummy_checked(val: Arc<AtomicU8>) -> u8 {
        val.fetch_add(1, std::sync::atomic::Ordering::Acquire);
        let max = val.load(std::sync::atomic::Ordering::Relaxed);
        tokio::time::sleep(Duration::from_nanos(100)).await;
        val.fetch_sub(1, std::sync::atomic::Ordering::Release);
        max
    }

    #[tokio::test]
    async fn test_futures_unordered_iter() {
        let tasks = create_tasks();
        let max_concurrent: u64 = 10;

        let mut fut_iter = FuturesUnorderedIter::new(max_concurrent as usize, tasks);

        let mut counter = 0;
        while let Some(result) = fut_iter.next().await {
            assert_eq!(result, counter);
            counter += 1;
            // Inner struct should always have less than `max_concurrent` futures.
            assert!(fut_iter.len_inner() <= max_concurrent as usize);
        }
    }

    #[tokio::test]
    async fn test_futures_unordered() {
        let mut fut_iter = FuturesUnorderedBounded::new(10);
        for i in 0..100 {
            fut_iter.push(dummy(i as u64));
        }
        // Run poll just 1 time
        let result = fut_iter.next().await;

        assert!(result.is_some());
        assert_eq!(fut_iter.len_queue(), 89); // we consumed 1 future

        // We should have 10 enqueued futures
        assert_eq!(fut_iter.len_inner(), 10);
        let _result = fut_iter.next().await;
        // 2 consumed futures at this point
        assert_eq!(fut_iter.len_queue(), 88);
        // We should (still) have 10 enqueued futures
        assert_eq!(fut_iter.len_inner(), 10);
        // Test clear queue
        fut_iter.clear_queue();
        assert_eq!(fut_iter.len_queue(), 0);
        assert!(fut_iter.is_empty_inner());
        assert_eq!(fut_iter.borrow_inner().len(), 0);

        let result = fut_iter.next().await;
        assert!(result.is_none());
        // Test push logic
        for i in 0..100 {
            fut_iter.push(dummy(i as u64));
        }
        assert_eq!(fut_iter.len_queue(), 90);
        assert_eq!(fut_iter.len_inner(), 10);

        let result = fut_iter.next().await;
        assert!(result.is_some());
        // we consumed 1 future
        assert_eq!(fut_iter.len_queue(), 89);
        // We should have 10 enqueued futures
        assert_eq!(fut_iter.len_inner(), 10);
        // Test borrow* methods
        assert_eq!(fut_iter.borrow_queue().len(), 89);
        assert_eq!(fut_iter.borrow_mut_queue().len(), 89);

        assert_eq!(fut_iter.into_inner().len(), 10);
    }

    #[tokio::test]
    async fn test_max_concurrent() {
        // Test from iter
        let test_value = Arc::new(AtomicU8::new(0));
        let tasks = (0..50)
            .map(|_| dummy_checked(Arc::clone(&test_value)))
            .collect::<Vec<_>>();

        let mut fut_iter = FuturesUnorderedIter::new(10, tasks);
        let mut max_so_far = 0;
        let mut count = 0;
        while let Some(max_res) = fut_iter.next().await {
            max_so_far = cmp::max(max_so_far, max_res);
            // Don't consume all the futures - we want to change the limit at runtime
            if count > 10 {
                fut_iter.set_capacity(20);
                break;
            }
            count += 1;
        }
        assert_eq!(max_so_far, 10);
        // Ensure the new capacity limit was applied
        let max = fut_iter.collect::<Vec<_>>().await.into_iter().max();
        assert_eq!(max, Some(20));

        // Test bounded one
        let test_value = Arc::new(AtomicU8::new(0));
        let mut fut_iter = FuturesUnorderedBounded::new(10);
        for _ in 0..50 {
            fut_iter.push(dummy_checked(Arc::clone(&test_value)));
        }
        let mut max_so_far = 0;
        while let Some(max_res) = fut_iter.next().await {
            max_so_far = cmp::max(max_so_far, max_res);
        }
        assert_eq!(max_so_far, 10);
        // Change capacity
        fut_iter.set_capacity(20);
        for _ in 0..50 {
            fut_iter.push(dummy_checked(Arc::clone(&test_value)));
        }
        while let Some(max_res) = fut_iter.next().await {
            max_so_far = cmp::max(max_so_far, max_res);
        }
        assert_eq!(max_so_far, 20);
    }

    #[tokio::test]
    async fn test_iter_inner() {
        let max_concurrent: u64 = 10;
        let tasks = (0..100).map(|i| dummy(i)).collect::<Vec<_>>();

        let mut fut_iter = FuturesUnorderedIter::new(max_concurrent as usize, tasks);

        assert_eq!(fut_iter.len_inner(), 10);
        assert_eq!(fut_iter.is_empty_inner(), false);
        {
            let inner = fut_iter.borrow_inner();
            assert_eq!(inner.len(), 10);
        }
        {
            let inner = fut_iter.borrow_mut_inner();
            assert_eq!(inner.len(), 10);
        }
        // Bypass control mechanism
        fut_iter.push_inner(dummy(123));
        assert_eq!(fut_iter.len_inner(), 11);

        // Clear internal struct
        fut_iter.clear_inner();
        assert_eq!(fut_iter.len_inner(), 0);

        let inner = fut_iter.into_inner();
        assert_eq!(inner.len(), 0);
    }
}
