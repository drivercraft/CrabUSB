use alloc::collections::btree_map::BTreeMap;
use usb_if::descriptor::{EndpointDescriptor, InterfaceDescriptor};

use crate::{
    endpoint::{direction, kind},
    err::USBError,
    xhci::{
        def::Dci,
        endpoint::{Endpoint, EndpointRaw},
    },
};

pub struct Interface {
    desc: InterfaceDescriptor,
    ep_map: BTreeMap<Dci, EndpointRaw>,
}

impl Interface {
    pub(crate) fn new(desc: InterfaceDescriptor, ep_map: BTreeMap<Dci, EndpointRaw>) -> Self {
        Self { desc, ep_map }
    }

    pub fn endpoint<T: kind::Sealed, D: direction::Sealed>(
        &mut self,
        address: u8,
    ) -> Result<Endpoint<T, D>, USBError> {
        let desc = self.find_ep_desc(address)?.clone();
        let dci = desc.dci().into();
        let ep_raw = self.ep_map.remove(&dci).ok_or(USBError::NotFound)?;
        Endpoint::new(desc, ep_raw)
    }

    fn find_ep_desc(&self, address: u8) -> Result<&EndpointDescriptor, USBError> {
        self.desc
            .endpoints
            .iter()
            .find(|ep| ep.address == address)
            .ok_or(USBError::NotFound)
    }
}
