//! # `tyco` - crate for creating scoped TYped COntexts.
//!
//! This crate allows you to define and store typed information in current execution context (sync or async) and
//! pass it down call stack without explicitely speicfying it in function arguments. This is helpful for implementing
//! some additional functionality without main business logic messing up your main business logic code.
//!
//! It is inspired by and similar to `opentelemetry::Context`, how loggers stored id log/slog/tracing, using
//! TLS for storing and accessing values. Unlike `opentelemetry::Context`, which is basically `HashMap<TypeId, Any>`,
//! current crate allows to work any type, which may (or may not) result in more efficient and clean code.
//!
//! # Example
//!
//! Basic example of how to define context, which will contain some tracing identifier, which can be used for things
//! like distinguishing HTTP requests.
//!
//! ```no_run
//! use tyco::{context, FutureExt, TypedContext};
//!
//! mod trace_id {
//!     use super::*;
//!
//!     #[derive(Clone, Default, Debug, PartialEq)]
//!     pub struct TraceId(String);
//!
//!     tyco::context!(TraceId);
//! }
//!
//! let t = trace_id::TraceId::default();
//!
//! let spawned_fut = tokio::spawn(
//!     async { println!("{:?}", trace_id::TraceId::current()) }
//!     // 2 ways to pass context to spawned future
//!     .with_current::<trace_id::TraceId>()
//!     .with(t.clone()),
//! );
//! ```

use std::{
    borrow::Cow,
    cell::RefCell,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
    thread::LocalKey,
};

use pin_project_lite::pin_project;

/// Trait for interaction with typed contexts.
pub trait TypedContext: Clone + 'static {
    const TLS: LocalKey<RefCell<Option<Cow<'static, Self>>>>;

    /// Get clone of current value of the context.
    ///
    /// `None` is returned if no value set.
    fn current() -> Option<Self> {
        Self::TLS.with(|v| v.borrow().clone()).map(Cow::into_owned)
    }

    /// Set value as the current context.
    ///
    /// It will live as long as returned guard is alive. Previous value is stored
    /// inside guard and will be restored on drop.
    fn attach(self) -> ContextGuard<Self> {
        let previous_value = Self::TLS
            .try_with(|current| current.replace(Some(Cow::Owned(self))))
            .ok();

        ContextGuard {
            previous_value,
            _marker: PhantomData,
        }
    }

    /// Set reference to a value as current context.
    ///
    /// This function is mainly used for [`FutureExt`] implementation and should
    /// not be used by user (or used with great caution !!!).
    unsafe fn attach_ref(&self) -> ContextRefGuard<Self> {
        let static_ref: &'static Self = unsafe { &*(self as *const Self) };
        let previous_value = Self::TLS
            .try_with(|current| current.replace(Some(Cow::Borrowed(static_ref))))
            .ok();

        ContextRefGuard {
            previous_value,
            _marker: PhantomData,
        }
    }
}

/// Guard, created with [`TypedContext::attach`], keeping value as current context.
///
/// On drop it will restore previous value.
pub struct ContextGuard<T: TypedContext> {
    previous_value: Option<Option<Cow<'static, T>>>,
    _marker: PhantomData<*const ()>,
}

impl<T: TypedContext> Drop for ContextGuard<T> {
    fn drop(&mut self) {
        if let Some(previous_value) = self.previous_value.take() {
            let _ = T::TLS.try_with(|current| current.replace(previous_value));
        }
    }
}

/// Guard, created with [`TypedContext::attach_ref`], keeping value as current context.
///
/// On drop it will restore previous value.
pub struct ContextRefGuard<'a, T: TypedContext> {
    previous_value: Option<Option<Cow<'static, T>>>,
    _marker: PhantomData<&'a ()>,
}

impl<'a, T: TypedContext> Drop for ContextRefGuard<'a, T> {
    fn drop(&mut self) {
        if let Some(previous_value) = self.previous_value.take() {
            let _ = T::TLS.try_with(|current| current.replace(previous_value));
        }
    }
}

pin_project! {
    /// Wrapper for a future, responsible for managing its context.
    #[derive(Clone, Debug)]
    pub struct WithContext<F, T> {
        #[pin]
        inner: F,
        value: Option<T>,
    }
}

impl<F: Future, T: TypedContext> Future for WithContext<F, T> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        if let Some(ref x) = this.value {
            let _guard = unsafe { x.attach_ref() };
            this.inner.poll(cx)
        } else {
            this.inner.poll(cx)
        }
    }
}

/// Extension trait allowing to attach context to futures.
pub trait FutureExt: Sized {
    /// Set value as context for future.
    fn with<T>(self, value: T) -> WithContext<Self, T> {
        WithContext {
            inner: self,
            value: Some(value),
        }
    }

    /// Set optional value as context for future.
    ///
    /// Primarily used with return value of [`TypedContext::current`].
    fn with_opt<T>(self, value: Option<T>) -> WithContext<Self, T> {
        WithContext {
            inner: self,
            value: value.into(),
        }
    }

    /// Take current context and set is as context for a future.
    ///
    /// Basically it is `self.with_opt(T::current())`.
    fn with_current<T: TypedContext>(self) -> WithContext<Self, T> {
        self.with_opt(T::current())
    }
}

impl<T: Sized + Future<Output = O>, O> FutureExt for T {}

/// Macro for implementing typed context.
///
/// This macro will generate impmenetation of [`TypedContext`] trait
/// for your type alongside with necessary TLS definitions. Macro accept
/// single argument - path to type.
///
/// # Note
///
/// Macro can be used only once in one module, because it have 'static' names for TLS variable. This is
/// done to keep this macro declarative.
///
/// # Example:
///
/// ```no_run
/// use tyco::{context, FutureExt, TypedContext};
///
/// mod trace_id {
///     use super::*;
///
///     #[derive(Clone, Default, Debug, PartialEq)]
///     pub struct TraceId(String);
///
///     tyco::context!(TraceId);
/// }
#[macro_export]
macro_rules! context {
    ($name:path) => {
        thread_local! {
            static CURRENT_CONTEXT_VALUE: std::cell::RefCell<Option<std::borrow::Cow<'static, $name>>> =
                std::cell::RefCell::new(None);
        }

        impl $crate::TypedContext for $name {
            const TLS: std::thread::LocalKey<std::cell::RefCell<Option<std::borrow::Cow<'static, Self>>>> =
                CURRENT_CONTEXT_VALUE;
        }
    };
}

#[cfg(test)]
mod ui_test {
    use std::time::{Duration, Instant};

    use super::{FutureExt, TypedContext};

    #[derive(Clone, Debug, PartialEq)]
    struct Deadline(Instant);

    impl Deadline {
        pub fn after(after: Duration) -> Self {
            Self(Instant::now() + after)
        }

        pub fn after_secs(after_secs: u64) -> Self {
            Self::after(Duration::from_secs(after_secs))
        }
    }

    context!(Deadline);

    #[test]
    fn both_attach() {
        let x1 = Deadline::after_secs(1);
        let _x1_guard = x1.clone().attach();

        assert_eq!(Deadline::current().unwrap(), x1);

        let x2 = Deadline::after_secs(2);
        let x2_guard = unsafe { x2.attach_ref() };

        assert_eq!(Deadline::current().unwrap(), x2);

        drop(x2_guard);
        drop(x2);

        assert_eq!(Deadline::current().unwrap(), x1);
    }

    #[tokio::test]
    async fn get_across_spawn() {
        let x = Deadline::after_secs(1);

        assert_eq!(
            tokio::spawn(async { Deadline::current() }.with(x.clone()))
                .await
                .unwrap(),
            Some(x)
        )
    }
}
