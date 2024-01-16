# Futures-Ratelimit

Bounded versions of `FuturesUnordered`/`FuturesOrdered`

This crate provides bounded versions of `FuturesUnordered`/`FuturesOrdered`.

The usual futures' `FuturesUnordered`/`FuturesOrdered` structs don't provide any means to limit how many
futures are polled at any given time. While you can use semaphores or any other
forms of resource locking, that won't stop the async runtime from potentially
polling (and thus, wasting CPU) too many futures at once. You can use channels, but
that feels cumbersome and means extra code and added overhead.

This crate provides types that behave just like the usual `futures`-(un)ordered types,
except that you can specify a maximum number of futures that can be polled at any
given time. This allows you to create and _submit_ tons of futures without
overwhelming the async runtime. **100% drop-in replacements** for `FuturesOrdered` and
`FuturesUnordered` are also provided.

The logic behind this crate is rather simple: we _override_ the underlying' `poll_next()`,
and, when necessary, pop a new future from either an iterator or a queue (at your
choice). This means that we'll always be running **exactly** N futures (or less, if
the futures source has been exhausted).

The wrapped types have been designed such that the checks (to pop for more futures)
are executed only when needed, thus making everything as efficient as it gets.

Basic example:

```rust
use futures_ratelimit::ordered::FuturesOrderedBounded;
use futures_ratelimit::common::Passthrough;
use futures::StreamExt;

async fn dummy() -> u64{
    42
}

let mut fut_unordered = FuturesOrderedBounded::new(5);
for _ in 0..10 {
    fut_unordered.push_back(dummy());
}

// The internal FuturesOrderedBounded will have 5 futures at most.
tokio_test::block_on(async move{
    assert_eq!(fut_unordered.borrow_inner().len(), 5);
    while let Some(value) = fut_unordered.next().await {
        println!("{}", value);
        assert!(fut_unordered.borrow_inner().len() <= 5);
    }
    assert_eq!(fut_unordered.borrow_inner().len(), 0);
    assert!(fut_unordered.borrow_inner().is_empty());
});
```

Make sure to check out the docs for examples!

### LICENSE

MIT


### Notes

The name of this library is intentionally misleading. It was supposed to be
`futures-bounded`, but that name is already being used in crates.io by someone else and
`rate-limit` kinda conveys the same meaning.