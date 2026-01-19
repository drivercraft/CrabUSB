# Hub 功能集成指南

## 当前状态

### ✅ 已完成（不破坏现有功能）

1. **核心数据结构** (`usb-host/src/hub/`)
   - `HubManager`: Hub 设备管理器
   - `HubDevice`: Hub 设备表示
   - `Port`: 端口状态管理
   - `HubEventHandler`: 事件处理系统

2. **xHCI Hub 支持** (`usb-host/src/backend/xhci/`)
   - `XhciRootHub`: xHCI Root Hub 实现
   - `XhciPort`: xHCI 端口实现

3. **测试项目** (`test_crates/test_hub/`)
   - 完整的测试框架
   - QEMU 配置文件

### ⚠️ 待集成（需要谨慎处理）

1. 将 `HubManager` 集成到 `Xhci` 主机控制器
2. 将 xHCI 端口事件连接到 Hub 事件处理
3. 实现完整的设备枚举流程

## 渐进式集成策略

### 阶段 1: 功能验证（当前）

**目标**: 确保 Hub 数据结构编译通过，现有测试不受影响

```bash
# 验证现有测试
cargo test -p crab-usb --test test --target aarch64-unknown-none-softfloat

# 验证 Hub 模块编译
cargo check -p crab-usb --target aarch64-unknown-none-softfloat
```

**状态**: ✅ 完成

### 阶段 2: 独立 Hub 测试

**目标**: 创建独立的 Hub 功能测试，不修改现有 Xhci 逻辑

**步骤**:

1. 创建独立的 Hub 测试（`test_hub`）
2. 使用 `XhciRootHub` 的独立实例
3. 不修改 `Xhci` 主机控制器

**示例**:

```rust
// test_crates/test_hub/tests/test.rs
#[test]
async fn test_root_hub_creation() {
    // 直接创建 XhciRootHub，不通过 Xhci
    let reg = /* 获取寄存器 */;
    let root_hub = XhciRootHub::new(reg, dma_mask)?;

    assert_eq!(root_hub.num_ports(), 4);
}
```

### 阶段 3: 可选集成

**目标**: 在 `Xhci` 中添加可选的 Hub 支持

**原则**:
- 使用 `#[cfg(feature = "hub")]` 条件编译
- 现有功能不受影响
- 用户可以选择启用 Hub 功能

**示例**:

```rust
// usb-host/src/backend/xhci/host.rs
pub struct Xhci {
    // ... 现有字段 ...

    #[cfg(feature = "hub")]
    hub_manager: Option<HubManager>,
}

impl Xhci {
    pub fn new(mmio: Mmio, dma_mask: usize) -> Result<Self> {
        // ... 现有代码 ...

        Ok(Xhci {
            // ... 现有字段 ...

            #[cfg(feature = "hub")]
            hub_manager: None,
        })
    }
}
```

### 阶段 4: 完整集成

**目标**: 完整集成 Hub 功能到主流程

**前提条件**:
- 所有现有测试通过
- Hub 功能测试完整
- 性能影响可接受

**步骤**:
1. 在 `reset_ports()` 中初始化 Root Hub
2. 在 `_probe_devices()` 中使用 HubManager
3. 在 `EventHandler::handle_event()` 中处理端口事件

## 安全检查清单

每次修改后，必须执行：

```bash
# 1. 编译检查
cargo check -p crab-usb --target aarch64-unknown-none-softfloat

# 2. 运行原有测试
cargo test -p crab-usb --test test --target aarch64-unknown-none-softfloat

# 3. 代码格式化
cargo fmt --all

# 4. （可选）运行 Hub 测试
cargo test -p test_hub --test test --target aarch64-unknown-none-softfloat
```

## 当前架构

```
usb-host/
├── src/
│   ├── backend/xhci/
│   │   ├── host.rs         (Xhci 主机控制器 - 不修改)
│   │   ├── hub.rs          (XhciRootHub - 独立实现)
│   │   ├── port.rs         (XhciPort - 独立实现)
│   │   └── event.rs        (事件处理 - 不修改)
│   └── hub/
│       ├── mod.rs          (Hub 模块导出)
│       ├── manager.rs      (HubManager - 独立管理器)
│       └── event.rs        (HubEventHandler - 事件系统)
└── tests/
    └── (现有测试 - 不修改)
```

## 下一步

1. ✅ 验证当前代码编译通过
2. ⏳ 实现 `test_hub` 独立测试
3. ⏳ 添加 feature gate 保护 Hub 集成
4. ⏳ 完整集成（在确保安全的前提下）

## 回滚策略

如果集成导致测试失败：

```bash
# 回滚到上一个工作版本
git reset --hard HEAD~1

# 或者恢复到特定提交
git checkout <commit-hash>
```

## 注意事项

1. **不要修改核心路径**: `Xhci::_probe_devices()` 等核心方法保持不变
2. **使用类型安全**: 所有新功能使用 Rust 类型系统保证安全
3. **保持向后兼容**: 现有用户代码不受影响
4. **渐进式验证**: 每次修改后立即运行测试
