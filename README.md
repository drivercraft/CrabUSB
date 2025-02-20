# CrabUSB

Rust 实现的 USB-Host 驱动

## 测试

运行Qemu测例

```bash
cargo install ostool
cargo test --test test -- --show-output
```

真机使用uboot运行测例

```bash
cargo test --test test -- --show-output --uboot
```

## 参考资料
