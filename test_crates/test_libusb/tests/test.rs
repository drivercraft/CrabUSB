#![cfg(test)]

use std::{hint::spin_loop, thread};

use crab_usb::{Class, DeviceInfo, USBHost};
use log::info;

#[tokio::test]
async fn test() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .is_test(true)
        .init();

    let mut host = USBHost::new_libusb();
    let event_handler = host.event_handler();
    let ls = host.device_list().await.unwrap();

    thread::spawn(move || {
        while event_handler.handle_event() {
            spin_loop();
        }
    });

    let mut info: Option<DeviceInfo> = None;

    for device in ls {
        println!("{device}");

        for iface in device.interface_descriptors() {
            println!("  Interface: {:?}", iface.class());

            // if device.vendor_id() == 0x1a86 && device.product_id() == 0x7523 {
            //     info = Some(device);
            //     break;
            // }

            if matches!(iface.class(), Class::Video | Class::AudioVideo(_)) {
                info!("Found video interface: {iface:?}");
                info = Some(device);
                break;
            }
        }
    }
    let mut info = info.unwrap();

    let mut device = info.open().await.unwrap();
    info!("Opened device: {device}");

    if let Some(index) = device.descriptor.manufacturer_string_index {
        let s = device.string_descriptor(index.get()).await.unwrap();
        info!("Manufacturer: {s}");
    }

    let config = device.current_configuration_descriptor().await.unwrap();

    for iface in &config.interfaces {
        let iface = iface.first_alt_setting();

        info!("Interface: {iface:?}");
        let interface = device
            .claim_interface(iface.interface_number, 0)
            .await
            .unwrap();

        info!("  Claimed interface: {interface}");

        for ep in &iface.endpoints {
            info!("  Endpoint: {ep:?}");
            // if matches!(ep.direction, Direction::In)
            //     && matches!(ep.transfer_type, EndpointType::Isochronous)
            // {
            //     let mut endpoint = interface.endpoint_iso_in(ep.address).unwrap();
            //     let mut buf = vec![0u8; ep.max_packet_size as usize];
            //     let transfer = endpoint.submit(&mut buf, 1).unwrap();
            //     let n = transfer.await.unwrap();
            //     info!("    Read {n} bytes: {:x?}", &buf[..n]);
            // }

            // if matches!(ep.direction, Direction::In)
            //     && matches!(ep.transfer_type, EndpointType::Bulk)
            // {
            //     let mut endpoint = interface.endpoint_bulk_in(ep.address).unwrap();
            //     let mut buf = vec![0u8; ep.max_packet_size as usize];
            //     let transfer = endpoint.submit(&mut buf).unwrap();
            //     let n = transfer.await.unwrap();
            //     info!("    Wrote {n} bytes: {:x?}", &buf[..n]);
            // }
        }
    }

    drop(host);
}
