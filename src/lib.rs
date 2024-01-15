//! Rate-limited version of `FuturesUnordered`/`FuturesOrdered`
//!
//! This crate provides a rate-limited version of `FuturesUnordered`/`FuturesOrdered`.
//!
//! The default `FuturesUnordered` does not provide any means to limit how many
//! futures are polled at any given time. While you can use semaphores or any other
//! forms of resource locking, that won't stop the async runtime from potentially
//! polling (and thus, wasting CPU) too many futures at once. You can use channels, but
//! that feels cumbersome and means extra code and added overhead.
//!
//! This crate provides types that behave just like the usual `futures`-(un)ordered types,
//! except that you can specify a maximum number of futures that can be polled at any
//! given time. This allows you to create and _submit_ tons of futures without
//! overwhelming the async runtime.
//!
//! The logic is rather simple: we override the underlying' `poll_next()`,
//! and, when necessary, pop a new future from either an iterator or a queue (at your
//! choice). This means that we'll always be running **exactly** N futures (or less, if
//! the futures source has been exhausted).
//!
//! The wrapped types have been designed such that the checks (to pop for more futures)
//! are executed only when needed, thus making everything as efficient as it gets.
//!
//! Make sure to check out the docs for examples!
pub mod common;
pub mod ordered;
pub mod unordered;
