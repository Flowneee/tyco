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

pub trait TypedContext: Clone + 'static {
    const TLS: LocalKey<RefCell<Option<Cow<'static, Self>>>>;

    fn current() -> Option<Self> {
        Self::TLS.with(|v| v.borrow().clone()).map(Cow::into_owned)
    }

    fn attach(self) -> ContextGuard<Self> {
        let previous_value = Self::TLS
            .try_with(|current| current.replace(Some(Cow::Owned(self))))
            .ok();

        ContextGuard {
            previous_value,
            _marker: PhantomData,
        }
    }

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

pub trait FutureExt: Sized {
    fn with<T>(self, value: T) -> WithContext<Self, T> {
        WithContext {
            inner: self,
            value: Some(value),
        }
    }

    fn with_opt<T>(self, value: Option<T>) -> WithContext<Self, T> {
        WithContext {
            inner: self,
            value: value.into(),
        }
    }

    fn with_current<T: TypedContext>(self) -> WithContext<Self, T> {
        self.with_opt(T::current())
    }
}

impl<T: Sized + Future<Output = O>, O> FutureExt for T {}

#[macro_export]
macro_rules! context {
    ($name:ident) => {
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

// mod v1 {
//     use std::time::{Duration, Instant};

//     macro_rules! context {
//         ($name:ident, $mod_name:ident) => {
//             mod $mod_name {
//                 use std::{
//                     borrow::Cow,
//                     cell::RefCell,
//                     future::Future,
//                     marker::PhantomData,
//                     pin::Pin,
//                     task::{Context, Poll},
//                 };

//                 use pin_project_lite::pin_project;

//                 use super::$name;

//                 thread_local! {
//                     static CURRENT_CONTEXT_VALUE: RefCell<Option<Cow<'static, $name>>> =
//                         RefCell::new(None);
//                 }

//                 impl $name {
//                     pub fn current() -> Option<Self> {
//                         CURRENT_CONTEXT_VALUE
//                             .with(|v| v.borrow().clone())
//                             .map(Cow::into_owned)
//                     }

//                     pub fn attach(self) -> ContextGuard {
//                         let previous_value = CURRENT_CONTEXT_VALUE
//                             .try_with(|current| current.replace(Some(Cow::Owned(self))))
//                             .ok();

//                         ContextGuard {
//                             previous_value,
//                             _marker: PhantomData,
//                         }
//                     }

//                     pub unsafe fn attach_ref(&self) -> ContextRefGuard {
//                         let static_ref: &'static Self = unsafe { &*(self as *const Self) };
//                         let previous_value = CURRENT_CONTEXT_VALUE
//                             .try_with(|current| current.replace(Some(Cow::Borrowed(static_ref))))
//                             .ok();

//                         ContextRefGuard {
//                             previous_value,
//                             _marker: PhantomData,
//                         }
//                     }
//                 }

//                 pub struct ContextGuard {
//                     previous_value: Option<Option<Cow<'static, $name>>>,
//                     _marker: PhantomData<*const ()>,
//                 }

//                 impl Drop for ContextGuard {
//                     fn drop(&mut self) {
//                         if let Some(previous_value) = self.previous_value.take() {
//                             let _ = CURRENT_CONTEXT_VALUE
//                                 .try_with(|current| current.replace(previous_value));
//                         }
//                     }
//                 }

//                 pub struct ContextRefGuard<'a> {
//                     previous_value: Option<Option<Cow<'static, $name>>>,
//                     _marker: PhantomData<&'a ()>,
//                 }

//                 impl<'a> Drop for ContextRefGuard<'a> {
//                     fn drop(&mut self) {
//                         if let Some(previous_value) = self.previous_value.take() {
//                             let _ = CURRENT_CONTEXT_VALUE
//                                 .try_with(|current| current.replace(previous_value));
//                         }
//                     }
//                 }

//                 pin_project! {
//                     #[derive(Clone, Debug)]
//                     pub struct WithContext<T> {
//                         #[pin]
//                         inner: T,
//                         value: Option<$name>,
//                     }

//                 }

//                 impl<T: Sized + Future<Output = O>, O> FutureExt for T {}

//                 impl<T: Future> Future for WithContext<T> {
//                     type Output = T::Output;

//                     fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
//                         let this = self.project();

//                         if let Some(ref x) = this.value {
//                             let _guard = unsafe { x.attach_ref() };
//                             this.inner.poll(cx)
//                         } else {
//                             this.inner.poll(cx)
//                         }
//                     }
//                 }

//                 pub trait FutureExt: Sized {
//                     fn with(self, value: $name) -> WithContext<Self> {
//                         WithContext {
//                             inner: self,
//                             value: Some(value),
//                         }
//                     }

//                     fn with_opt(self, value: impl Into<Option<$name>>) -> WithContext<Self> {
//                         WithContext {
//                             inner: self,
//                             value: value.into(),
//                         }
//                     }

//                     fn with_current(self) -> WithContext<Self> {
//                         self.with_opt($name::current())
//                     }
//                 }
//             }
//         };
//     }

//     #[derive(Clone, Debug, PartialEq)]
//     struct Deadline(Instant);

//     impl Deadline {
//         pub fn after(after: Duration) -> Self {
//             Self(Instant::now() + after)
//         }

//         pub fn after_secs(after_secs: u64) -> Self {
//             Self(Instant::now() + Duration::from_secs(after_secs))
//         }
//     }

//     context!(Deadline, deadline);

//     #[cfg(test)]
//     mod ui_test {
//         use super::{deadline::FutureExt, Deadline};

//         #[test]
//         fn both_attach() {
//             let x1 = Deadline::after_secs(1);
//             let _x1_guard = x1.clone().attach();

//             assert_eq!(Deadline::current().unwrap(), x1);

//             let x2 = Deadline::after_secs(2);
//             let x2_guard = unsafe { x2.attach_ref() };

//             assert_eq!(Deadline::current().unwrap(), x2);

//             drop(x2_guard);
//             drop(x2);

//             assert_eq!(Deadline::current().unwrap(), x1);
//         }

//         #[tokio::test]
//         async fn get_across_spawn() {
//             let x = Deadline::after_secs(1);

//             assert_eq!(
//                 tokio::spawn(async { Deadline::current() }.with(x.clone()))
//                     .await
//                     .unwrap(),
//                 Some(x)
//             )
//         }
//     }
// }
