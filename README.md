# io-uring based Runtime

An experimental asynchronous runtime based on `io-uring`.

It attempts to provide a safe and easy-to-use API, but it is still pending.


# What is missing?

## AsyncRead and AsyncWrite

This is probably the most difficult point in designing the Proactor API.
The current `Async{Read,Write}` is designed for Reactor,
which makes it not compatible with `io-uring`.

In Proactor Pattern, buffers are written asynchronously,
and often fail to provide a reliable cancellation interface,
which means that pass buffer references may be unsound in some extreme cases.
this is a known issue with some `io-uring` crate in Rust. see
[1](https://github.com/slp/io-uring/issues/1)
[2](https://github.com/spacejam/rio/issues/1)
[3](https://github.com/spacejam/rio/issues/11)
[4](https://github.com/spacejam/rio/issues/12).

The only reliable way is to let Proactor (kernel) hold buffer's ownership.

## Buffer management

Designing a generic `Async{Read,Write}` is hard.
Since Rust does not yet support the async trait, our low-level API must still use poll.

We hope to use it like today's `Async{Read,Write}`,
which means it needs to be zero-overhead and avoid unsafe.
another point is trait object, which means we can't use too many generics.

There are roughly three style about this API design.

1. allocator + `poll_read()`

```rust
trait AsyncRead {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Bytes>;
}
```

  * A customizable buffer allocator is responsible for alloc all buffers,
    and `poll_read` will simply return completed buffer.
  * It looks like `Stream`/`Sink`, which means we don't need a new Async IO API.
  * It can be well combined with the io-uring buffer select feature.
  * Users cannot use their own buffers.
  * Users cannot specify buffer length.
  * The design of allocator will be another difficulty.

2. Explicitly pass in buffers

```rust
trait AsyncRead {
    fn submit(&mut self, buf: BytesMut);
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<BytesMut>;
}
```

* Users can use their own buffer.
* But cannot use io-uring's buffer select feature.

3. Compromise scheme

```rust
trait Buffer {
    ...
}

trait AsyncRead {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut dyn Buffer) -> Poll<BytesMut>;
}
```

* Use a special buffer to be compatible with both
* It usually looks a bit strange

We have not decided on the final API,
if you are interested, you can discuss it in
[ritsu issue](https://github.com/quininer/ritsu/issues/2) or
[tokio issue](https://github.com/tokio-rs/tokio/issues/2411).

## Thread model

Currently ritsu is single-threaded, just because it was single-threaded at first.

I suspect that it may not be very beneficial to support multi-threading.
maybe correct model is similar to today's reactor,
each thread (or CPU?) has its own proactor.

Currently we can use a channel to pass entry. like [tokio-ritsu](./tokio-ritsu) crate.

## Cancellation future

The cancellation of `io-uring` is very different from the usual poll-based API.
It has a separate `ASYNC_CANCEL` opcode,
which may mean that the API needs to be designed new for it.

There has not been any discussion on this so far.
