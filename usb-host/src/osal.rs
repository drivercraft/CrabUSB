use core::time::Duration;

use futures::future::BoxFuture;
use trait_ffi::def_extern_trait;

#[def_extern_trait]
pub trait Kernel {
    fn page_size() -> usize;
}
