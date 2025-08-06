#![cfg(test)]

use crab_usb::USBHost;

#[tokio::test]
async fn test() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .is_test(true)
        .init();

    let mut host = USBHost::new_libusb();
    let mut ls = host.device_list().await.unwrap();

    for device in ls {
        println!("Device: {:?}", device.descriptor().await.unwrap());

        for iface in device.interface_descriptors() {
            println!("  Interface: {iface:?}",);
        }
    }
}
