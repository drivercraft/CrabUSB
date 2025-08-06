#![cfg(test)]

use crab_usb::Host;

#[tokio::test]
async fn test() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .is_test(true)
        .init();

    let mut host = Host::new_libusb();
    let ls = host.probe().await.unwrap();

    println!("Found {} devices", ls.len());

    for device in &ls {
        println!("Device: {:?}", device.descriptor());
    }
}
