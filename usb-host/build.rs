fn main() {
    println!("cargo::rustc-check-cfg=cfg(umod)");
    println!("cargo::rustc-check-cfg=cfg(kmod)");

    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if os == "none" {
        println!("cargo::rustc-cfg=kmod");
    } else {
        println!("cargo::rustc-cfg=umod");
    }
}
