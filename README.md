# Reproducer for a flume notification bug

Flume notifications are currently not cancel safe. flume::Receiver::recv_async
will not always get a notification when it is cancelled. So you might have
a situation where you send into the queue, and recv_async blocks until some
other future waker "Fixes" it. Or forever.

To reproduce, run:

```
cargo build --release && for i in {1..100000}; do ./target/release/flume-bug-reproducer; done
```

It will hang after a few iterations when using flume, but will run forever when
using async_channel.