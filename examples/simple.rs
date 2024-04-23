use tyco::{context, FutureExt, TypedContext};

// Define context
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

    // Magic here
    context!(Deadline);
}

#[tokio::main]
async fn main() {
    let d = deadline::Deadline::after_secs(1);

    let _d_guard = d.clone().attach();

    let res = tokio::spawn(
        async { deadline::Deadline::current() }
            // 2 ways to pass context to spawned future
            .with_current::<deadline::Deadline>()
            .with(d.clone()),
    )
    .await
    .unwrap();

    assert_eq!(res, (Some(d)))
}
