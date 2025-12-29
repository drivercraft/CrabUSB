use usb_if::{err::TransferError, host::ControlSetup};
use xhci::ring::trb::transfer::Direction;

pub trait EndpointKernel {
    fn transfer(
        &mut self,
        param: ControlSetup,
        dir: Direction,
        buff: Option<(usize, usize)>,
    ) -> impl Future<Output = Result<usize, TransferError>> + Send;


}

