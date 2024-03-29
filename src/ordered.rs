//! Bounded replacements for `FuturesOrdered`.

use std::borrow::BorrowMut;
use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::stream::{FuturesOrdered, Stream};
use futures::Future;
use pin_project::pin_project;

use crate::common::Passthrough;

/// Bounded FuturesOrdered from an iterator.
///
/// This struct consumes an iterator where each item
/// is a future. By using an iterator, we need no additional state,
/// or additional queues to store the excess of futures.
///
/// This should be always preferred over `FuturesOrderedBounded`,
/// as it's more performant and adds basically ZERO overhead on top
/// of the normal `FuturesOrdered`.
#[pin_project]
pub struct FuturesOrderedIter<I, F>
where
    F: Future,
    I: Iterator<Item = F>,
{
    max_concurrent: usize,
    tasks: I,
    #[pin]
    running_tasks: FuturesOrdered<F>,
}

/// Drop-in replacement for `FuturesOrdered`.
///
/// This struct behaves exactly like the normal `FuturesOrdered`.
///
/// Unlike `FuturesOrderedIter`, this struct allows you to continuously
/// push futures into it, but it'll only poll N futures at any given time.
/// This struct internally uses a `Vec` to store the excess of futures.
#[pin_project]
pub struct FuturesOrderedBounded<F>
where
    F: Future,
{
    max_concurrent: usize,
    queued_tasks: VecDeque<F>,
    #[pin]
    running_tasks: FuturesOrdered<F>,
}

impl<I, F> Passthrough<F> for FuturesOrderedIter<I, F>
where
    I: Iterator<Item = F>,
    F: Future,
{
    type FuturesHolder = FuturesOrdered<F>;

    fn set_max_concurrent(&mut self, max_concurrent: usize) {
        self.max_concurrent = max_concurrent;
    }

    fn borrow_inner(&self) -> &FuturesOrdered<F> {
        &self.running_tasks
    }

    fn borrow_mut_inner(&mut self) -> &mut FuturesOrdered<F> {
        self.running_tasks.borrow_mut()
    }

    fn into_inner(self) -> FuturesOrdered<F>
    where
        Self: Sized,
    {
        self.running_tasks
    }
}

impl<F> Passthrough<F> for FuturesOrderedBounded<F>
where
    F: Future,
{
    type FuturesHolder = FuturesOrdered<F>;

    fn set_max_concurrent(&mut self, max_concurrent: usize) {
        self.max_concurrent = max_concurrent;
    }

    fn borrow_inner(&self) -> &FuturesOrdered<F> {
        &self.running_tasks
    }

    fn borrow_mut_inner(&mut self) -> &mut FuturesOrdered<F> {
        self.running_tasks.borrow_mut()
    }

    fn into_inner(self) -> FuturesOrdered<F>
    where
        Self: Sized,
    {
        self.running_tasks
    }
}

impl<T, F> Stream for FuturesOrderedIter<T, F>
where
    T: Iterator<Item = F>,
    F: Future,
{
    type Item = F::Output;

    /// Calls the `poll()` method on the underlying `FuturesOrdered`.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        match this.running_tasks.as_mut().poll_next(cx) {
            Poll::Ready(Some(value)) => {
                while this.running_tasks.len() < *this.max_concurrent {
                    match this.tasks.next() {
                        Some(future) => this.running_tasks.push_back(future),
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

impl<F> futures::stream::Stream for FuturesOrderedBounded<F>
where
    F: Future,
{
    type Item = F::Output;

    /// Calls the `poll()` method on the underlying `FuturesOrdered`.
    ///
    /// **Important note**: The poll implementation of `FuturesOrderedBounded`
    /// uses an internal `VecDeque` to store the excess of futures. The `poll()`
    /// method  uses `pop_front` to get more futures. There's no way
    /// to change this behavior, although this could change in the future.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        match this.running_tasks.as_mut().poll_next(cx) {
            Poll::Ready(Some(value)) => {
                while this.running_tasks.len() < *this.max_concurrent {
                    match this.queued_tasks.pop_front() {
                        Some(future) => this.running_tasks.push_back(future),
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

impl<T, F> FuturesOrderedIter<T, F>
where
    F: Future,
    T: Iterator<Item = F>,
{
    /// Creates a new bounded `FuturesOrdered` from an iterator.
    /// This option is as efficient as it gets since it completely avoids
    /// allocating a new vector to store the excess of futures.
    ///
    /// Since iterators are lazily evaluated, futures will be created on the
    /// fly as well.
    ///
    /// Panics if `max_concurrent` is 0.
    /// ```rust
    /// use futures_ratelimit::ordered::FuturesOrderedIter;
    /// use futures_ratelimit::common::Passthrough;
    /// use futures::StreamExt;
    ///
    /// async fn dummy() -> u64{
    ///     42
    /// }
    ///
    /// let tasks = (0..100).into_iter().map(|_| dummy());
    /// let mut fut_unordered = FuturesOrderedIter::new(5, tasks);
    /// // The internal FuturesOrdered will have 5 futures at most.
    ///
    /// tokio_test::block_on(async move{
    ///     assert_eq!(fut_unordered.borrow_inner().len(), 5);
    ///
    ///     while let Some(value) = fut_unordered.next().await {
    ///         println!("{}", value);
    ///         assert!(fut_unordered.borrow_inner().len() <= 5);
    ///     }
    ///
    ///     assert_eq!(fut_unordered.borrow_inner().len(), 0);
    ///     assert!(fut_unordered.borrow_inner().is_empty());
    ///
    /// });
    ///
    /// ```
    pub fn new<I: IntoIterator<IntoIter = T>>(max_concurrent: usize, tasks: I) -> Self {
        let mut running_tasks = FuturesOrdered::new();
        assert!(max_concurrent > 0, "max_concurrent must be greater than 0");
        let mut tasks = tasks.into_iter();
        // Immediately put futures in the queue
        tasks
            .borrow_mut()
            .take(max_concurrent)
            .for_each(|future| running_tasks.push_back(future));

        Self {
            max_concurrent,
            tasks,
            running_tasks,
        }
    }
}

impl<F> FuturesOrderedBounded<F>
where
    F: Future,
{
    /// Creates a new bounded `FuturesOrdered`. This struct
    /// is 100% compatible with the normal `FuturesOrdered`.
    ///
    /// Panics if `max_concurrent` is 0.
    /// ```rust
    /// use futures_ratelimit::ordered::FuturesOrderedBounded;
    /// use futures_ratelimit::common::Passthrough;
    /// use futures::StreamExt;
    ///
    /// async fn dummy() -> u64{
    ///     42
    /// }
    ///
    /// let mut fut_unordered = FuturesOrderedBounded::new(5);
    /// for _ in 0..10 {
    ///     fut_unordered.push_back(dummy());
    /// }
    ///
    /// // The internal FuturesOrderedBounded will have 5 futures at most.
    ///
    /// tokio_test::block_on(async move{
    ///     assert_eq!(fut_unordered.borrow_inner().len(), 5);
    ///
    ///     while let Some(value) = fut_unordered.next().await {
    ///         println!("{}", value);
    ///         assert!(fut_unordered.borrow_inner().len() <= 5);
    ///     }
    ///
    ///     assert_eq!(fut_unordered.borrow_inner().len(), 0);
    ///     assert!(fut_unordered.borrow_inner().is_empty());
    ///
    /// });
    ///
    /// ```
    pub fn new(max_concurrent: usize) -> Self {
        let running_tasks = FuturesOrdered::new();
        assert!(max_concurrent > 0, "max_concurrent must be greater than 0");
        Self {
            max_concurrent,
            queued_tasks: VecDeque::new(),
            running_tasks,
        }
    }

    /// Appends an element to the back of the deque.
    ///
    /// If more than `max_concurrent` futures are already in the
    /// internal queue, the excess of futures will be stored in another
    /// queue and will only get pulled as needed.
    pub fn push_back(&mut self, fut: F) {
        match self.borrow_inner().len() < self.max_concurrent {
            true => self.borrow_mut_inner().push_back(fut),
            false => self.queued_tasks.push_back(fut),
        }
    }

    /// Appends an element to the front of the deque.
    ///
    /// If more than `max_concurrent` futures are already in the
    /// internal queue, the excess of futures will be stored in another
    /// queue and will only get pulled as needed.
    pub fn push_front(&mut self, fut: F) {
        match self.borrow_inner().len() < self.max_concurrent {
            true => self.borrow_mut_inner().push_front(fut),
            false => self.queued_tasks.push_front(fut),
        }
    }

    /// Mutably borrow the internal queue task list.
    pub fn borrow_mut_queue(&mut self) -> &mut VecDeque<F> {
        &mut self.queued_tasks
    }

    /// Borrow the internal queue task list.
    pub fn borrow_queue(&self) -> &VecDeque<F> {
        &self.queued_tasks
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

    async fn dummy_sleep(sleep_millis: u64) -> u64 {
        tokio::time::sleep(Duration::from_millis(sleep_millis)).await;
        sleep_millis
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
    async fn test_futures_ordered_iter() {
        let tasks = create_tasks();
        let max_concurrent: u64 = 10;

        let mut fut_iter = FuturesOrderedIter::new(max_concurrent as usize, tasks);

        let mut counter = 0;
        while let Some(result) = fut_iter.next().await {
            assert_eq!(result, counter);
            counter += 1;
            // Inner struct should always have less than `max_concurrent` futures.
            assert!(fut_iter.borrow_inner().len() <= max_concurrent as usize);
        }
    }

    #[tokio::test]
    async fn test_futures_ordered() {
        let mut fut_iter = FuturesOrderedBounded::new(10);
        for i in 0..100 {
            fut_iter.push_back(dummy(i as u64));
        }
        // Run poll just 1 time
        let result = fut_iter.next().await;

        assert!(result.is_some());
        assert_eq!(fut_iter.borrow_queue().len(), 89); // we consumed 1 future

        // We should have 10 enqueued futures
        assert_eq!(fut_iter.borrow_inner().len(), 10);
        let _result = fut_iter.next().await;
        // 2 consumed futures at this point
        assert_eq!(fut_iter.borrow_queue().len(), 88);
        // We should (still) have 10 enqueued futures
        assert_eq!(fut_iter.borrow_inner().len(), 10);
        // Test clear queue
        fut_iter.borrow_mut_queue().clear();
        assert_eq!(fut_iter.borrow_queue().len(), 0);

        // Consume all the futures
        while let Some(_) = fut_iter.next().await {}

        let result = fut_iter.next().await;
        assert!(result.is_none());
        // Test push logic
        for i in 0..100 {
            fut_iter.push_back(dummy(i as u64));
        }
        assert_eq!(fut_iter.borrow_queue().len(), 90);
        assert_eq!(fut_iter.borrow_inner().len(), 10);

        let result = fut_iter.next().await;
        assert!(result.is_some());
        // we consumed 1 future
        assert_eq!(fut_iter.borrow_queue().len(), 89);
        // We should have 10 enqueued futures
        assert_eq!(fut_iter.borrow_inner().len(), 10);
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

        let mut fut_iter = FuturesOrderedIter::new(10, tasks);
        let mut max_so_far = 0;
        let mut count = 0;
        while let Some(max_res) = fut_iter.next().await {
            max_so_far = cmp::max(max_so_far, max_res);
            // Don't consume all the futures - we want to change the limit at runtime
            if count > 10 {
                fut_iter.set_max_concurrent(20);
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
        let mut fut_iter = FuturesOrderedBounded::new(10);
        for _ in 0..50 {
            fut_iter.push_back(dummy_checked(Arc::clone(&test_value)));
        }
        let mut max_so_far = 0;
        while let Some(max_res) = fut_iter.next().await {
            max_so_far = cmp::max(max_so_far, max_res);
        }
        assert_eq!(max_so_far, 10);
        // Change capacity
        fut_iter.set_max_concurrent(20);
        for _ in 0..50 {
            fut_iter.push_back(dummy_checked(Arc::clone(&test_value)));
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

        let mut fut_iter = FuturesOrderedIter::new(max_concurrent as usize, tasks);

        assert_eq!(fut_iter.borrow_inner().len(), 10);
        assert_eq!(fut_iter.borrow_inner().is_empty(), false);
        {
            let inner = fut_iter.borrow_inner();
            assert_eq!(inner.len(), 10);
        }
        {
            let inner = fut_iter.borrow_mut_inner();
            assert_eq!(inner.len(), 10);
        }
        // Bypass control mechanism
        fut_iter.borrow_mut_inner().push_back(dummy(123));
        assert_eq!(fut_iter.borrow_inner().len(), 11);

        let inner = fut_iter.into_inner();
        assert_eq!(inner.len(), 11);
    }

    // Test with functions that actually take a little bit more time to ensure the inner
    // FuturesOrdered works as expected
    #[tokio::test]
    async fn test_push_back_front() {
        let max_concurrent = 10;

        let mut fut_iter = FuturesOrderedBounded::new(max_concurrent);
        fut_iter.push_back(dummy(1));
        fut_iter.push_front(dummy(2));
        fut_iter.push_back(dummy(3));

        assert_eq!(fut_iter.next().await, Some(2));
        assert_eq!(fut_iter.next().await, Some(1));
        assert_eq!(fut_iter.next().await, Some(3));

        let mut fut_iter = FuturesOrderedBounded::new(max_concurrent);
        fut_iter.push_back(dummy_sleep(1));
        fut_iter.push_back(dummy_sleep(10));
        fut_iter.push_back(dummy_sleep(2));
        assert_eq!(fut_iter.next().await, Some(1));
        assert_eq!(fut_iter.next().await, Some(10));
        assert_eq!(fut_iter.next().await, Some(2));

        let futures = (1..4).rev().map(dummy_sleep).collect::<Vec<_>>();
        let mut fut_iter = FuturesOrderedIter::new(max_concurrent, futures);
        assert_eq!(fut_iter.next().await, Some(3));
        assert_eq!(fut_iter.next().await, Some(2));
        assert_eq!(fut_iter.next().await, Some(1));
    }
}
