use core::time::Duration;

use trait_ffi::def_extern_trait;

#[def_extern_trait]
pub trait Kernel {
    fn page_size() -> usize;
    fn delay(duration: Duration);
}



pub struct SpinWhile<F>
where
    F: Fn() -> bool,
{
    pub condition: F,
}

impl<F> SpinWhile<F>
where
    F: Fn() -> bool,
{
    #[must_use]
    pub fn new(condition: F) -> Self {
        Self { condition }
    }
}

impl<F> core::future::Future for SpinWhile<F>
where
    F: Fn() -> bool,
{
    type Output = ();

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        if (self.condition)() {
            cx.waker().wake_by_ref();
            core::task::Poll::Pending
        } else {
            core::task::Poll::Ready(())
        }
    }
}
