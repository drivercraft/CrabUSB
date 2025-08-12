use core::time::Duration;

use futures::future::BoxFuture;
use trait_ffi::def_extern_trait;

#[def_extern_trait]
pub trait Kernel {
    fn sleep<'a>(duration: Duration) -> BoxFuture<'a, ()>;
    fn page_size() -> usize;
}
