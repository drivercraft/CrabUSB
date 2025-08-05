#![cfg(test)]

use crab_usb::USBHost;

#[tokio::test]
async fn test() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .is_test(true)
        .init();

    let mut host = USBHost::new_libusb();
    let ls = host.probe().await.unwrap();
    println!("Found {} devices", ls.len());
}
