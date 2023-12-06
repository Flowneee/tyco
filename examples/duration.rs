use tyco::{context, FutureExt, TypedContext};

// Контекс №1
mod deadline {
    use std::time::{Duration, Instant};

    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    pub struct Deadline(Instant);

    impl Deadline {
        pub fn after(after: Duration) -> Self {
            Self(Instant::now() + after)
        }

        pub fn after_secs(after_secs: u64) -> Self {
            Self::after(Duration::from_secs(after_secs))
        }
    }

    // Магия тут
    context!(Deadline);
}

// Контекс №2
mod trace_id {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    pub struct TraceId(String);

    impl TraceId {
        pub fn new(v: impl ToString) -> Self {
            Self(v.to_string())
        }
    }

    // И тут
    context!(TraceId);
}

#[tokio::main]
async fn main() {
    let d = deadline::Deadline::after_secs(1);
    let t = trace_id::TraceId::new("1234");

    let _d_guard = d.clone().attach();

    let res = tokio::spawn(
        async { (deadline::Deadline::current(), trace_id::TraceId::current()) }
            // 2 способа проброосить в другую футуру
            .with_current::<deadline::Deadline>()
            .with(t.clone()),
    )
    .await
    .unwrap();

    assert_eq!(res, (Some(d), Some(t)))
}
