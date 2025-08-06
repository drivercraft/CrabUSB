use alloc::{boxed::Box, collections::btree_map::BTreeMap};
use usb_if::descriptor::{EndpointDescriptor, InterfaceDescriptor};

use crate::{
    endpoint::{direction, kind},
    err::USBError,
    host::xhci::{
        def::Dci,
        endpoint::{Endpoint, EndpointControl, EndpointRaw},
    },
};

pub struct Interface {
    desc: InterfaceDescriptor,
    ctrl_ep: EndpointControl,
    ep_map: BTreeMap<Dci, EndpointRaw>,
}

impl Interface {
    pub(crate) fn new(
        desc: InterfaceDescriptor,
        ep_map: BTreeMap<Dci, EndpointRaw>,
        ctrl_ep: EndpointControl,
    ) -> Self {
        Self {
            desc,
            ep_map,
            ctrl_ep,
        }
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

impl usb_if::host::Interface for Interface {
    // fn set_alt_setting(&mut self, _alt_setting: u8) -> Result<(), USBError> {
    //     todo!()
    // }

    // fn get_alt_setting(&self) -> Result<u8, USBError> {
    //     todo!()
    // }

    fn control_in<'a>(
        &mut self,
        setup: usb_if::host::ControlSetup,
        data: &'a mut [u8],
    ) -> usb_if::host::ResultTransfer<'a> {
        self.ctrl_ep.control_in(setup, data)
    }

    fn control_out<'a>(
        &mut self,
        setup: usb_if::host::ControlSetup,
        data: &'a [u8],
    ) -> usb_if::host::ResultTransfer<'a> {
        self.ctrl_ep.control_out(setup, data)
    }

    fn endpoint_bulk_in(
        &mut self,
        endpoint: u8,
    ) -> Result<Box<dyn usb_if::host::EndpointBulkIn>, USBError> {
        let ep = self.endpoint::<kind::Bulk, direction::In>(endpoint)?;
        Ok(Box::new(ep))
    }

    fn endpoint_bulk_out(
        &mut self,
        endpoint: u8,
    ) -> Result<Box<dyn usb_if::host::EndpointBulkOut>, USBError> {
        let ep = self.endpoint::<kind::Bulk, direction::Out>(endpoint)?;
        Ok(Box::new(ep))
    }

    fn endpoint_interrupt_in(
        &mut self,
        endpoint: u8,
    ) -> Result<Box<dyn usb_if::host::EndpointInterruptIn>, USBError> {
        let ep = self.endpoint::<kind::Interrupt, direction::In>(endpoint)?;
        Ok(Box::new(ep))
    }

    fn endpoint_interrupt_out(
        &mut self,
        endpoint: u8,
    ) -> Result<Box<dyn usb_if::host::EndpointInterruptOut>, USBError> {
        let ep = self.endpoint::<kind::Interrupt, direction::Out>(endpoint)?;
        Ok(Box::new(ep))
    }

    fn endpoint_iso_in(
        &mut self,
        endpoint: u8,
    ) -> Result<Box<dyn usb_if::host::EndpintIsoIn>, USBError> {
        let ep = self.endpoint::<kind::Isochronous, direction::In>(endpoint)?;
        Ok(Box::new(ep))
    }

    fn endpoint_iso_out(
        &mut self,
        endpoint: u8,
    ) -> Result<Box<dyn usb_if::host::EndpintIsoOut>, USBError> {
        let ep = self.endpoint::<kind::Isochronous, direction::Out>(endpoint)?;
        Ok(Box::new(ep))
    }
}
