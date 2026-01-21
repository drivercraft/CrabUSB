fn main() {
    bare_test_macros::build_test_setup!();

    println!("cargo::rustc-check-cfg=cfg(libusb)");

    let target = std::env::var("TARGET").unwrap_or_default();

    if std::env::var("CARGO_FEATURE_LIBUSB").is_ok()
        && (target.contains("windows") || target.contains("linux") || target.contains("apple"))
    {
        println!("cargo::rustc-cfg=libusb");
    }
}
