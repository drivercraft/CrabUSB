#![cfg(test)]

use crab_usb::{BaseClass, DeviceInfo, USBHost};
use log::info;

#[tokio::test]
async fn test() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .is_test(true)
        .init();

    let mut host = USBHost::new_libusb();
    let ls = host.device_list().await.unwrap();

    let mut info: Option<DeviceInfo> = None;

    for device in ls {
        println!("Device: {:?}", device.descriptor().await.unwrap());

        for iface in device.interface_descriptors() {
            println!("  Interface: {iface:?}",);

            if iface.class == BaseClass::VIDEO {
                info = Some(device);
                break;
            }
        }
    }

    let mut info = info.unwrap();

    let mut device = info.open().await.unwrap();
    info!("Opened device: {device}");

    let config = device.current_configuration_descriptor().await.unwrap();

    for iface in &config.interfaces {
        let interface = device
            .claim_interface(iface.interface_number, 0)
            .await
            .unwrap();
    }
}
