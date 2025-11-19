# Test UVC over xHCI

```shell
cargo test --package test_xhci_uvc --test test --target aarch64-unknown-none-softfloat -- --show-output uboot | tee target/uvc.log

cargo run -p uvc-frame-parser -- -l target/uvc.log -o target/output
```
