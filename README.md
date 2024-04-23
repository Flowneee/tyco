# `tyco` - crate for creating scoped TYped COntexts.

This crate allows you to define and store typed information in current execution context (sync or async) and
pass it down call stack without explicitely speicfying it in function arguments. This is helpful for implementing
some additional functionality without main business logic messing up your main business logic code.

It is inspired by and similar to `opentelemetry::Context`, how loggers stored id log/slog/tracing, using
TLS for storing and accessing values. Unlike `opentelemetry::Context`, which is basically `HashMap<TypeId, Any>`,
current crate allows to work any type, which may (or may not) result in more efficient and clean code.

## Example

Basic example of how to define context, which will contain some tracing identifier, which can be used for things
like distinguishing HTTP requests.

```rust 
use tyco::{context, FutureExt, TypedContext};

mod trace_id {
    use super::*;

    #[derive(Clone, Default, Debug, PartialEq)]
    pub struct TraceId(String);

    tyco::context!(TraceId);
}

let t = trace_id::TraceId::default();

let spawned_fut = tokio::spawn(
    async { println!("{:?}", trace_id::TraceId::current()) }
    // 2 ways to pass context to spawned future
    .with_current::<trace_id::TraceId>()
    .with(t.clone()),
);
```

Also check more complex [examples](/examples) (on how to use it with HTTP server/client for example).
