use core::{
    cell::UnsafeCell,
    hint::spin_loop,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use alloc::sync::Arc;
use log::{debug, info, trace};
use mbarrier::wmb;
use xhci::{
    registers::doorbell,
    ring::trb::{
        command,
        event::{Allowed, CommandCompletion, CompletionCode, TransferEvent},
    },
};

use crate::{
    BusAddr, PortId,
    err::USBError,
    sleep,
    wait::{WaitMap, Waiter},
    xhci::{
        XhciRegisters,
        context::{DeviceContextList, ScratchpadBufferArray},
        def::SlotId,
        device::Device,
        event::EventRing,
        reg::DisableIrqGuard,
        ring::{Ring, TrbData},
    },
};

pub struct Root {
    reg: XhciRegisters,
    pub event_ring: EventRing,
    pub dev_list: DeviceContextList,
    pub cmd: Ring,
    pub scratchpad_buf_arr: Option<ScratchpadBufferArray>,

    wait_transfer: WaitMap<TransferEvent>,
    wait_cmd: WaitMap<CommandCompletion>,
}

impl Root {
    pub fn new(max_slots: usize, reg: XhciRegisters) -> Result<Self, USBError> {
        let cmd = Ring::new_with_len(
            0x1000 / size_of::<TrbData>(),
            true,
            dma_api::Direction::Bidirectional,
        )?;
        let event_ring = EventRing::new()?;

        let cmd_wait = WaitMap::new(cmd.trb_bus_addr_list().map(|a| a.raw()));

        Ok(Self {
            dev_list: DeviceContextList::new(max_slots)?,
            cmd,
            event_ring,
            scratchpad_buf_arr: None,
            reg,
            wait_transfer: WaitMap::empty(),
            wait_cmd: cmd_wait,
        })
    }

    pub fn init(&mut self) -> Result<(), USBError> {
        self.disable_irq();
        // Program the Device Context Base Address Array Pointer (DCBAAP)
        // register (5.4.6) with a 64-bit address pointing to where the Device
        // Context Base Address Array is located.
        self.setup_dcbaap();
        // Define the Command Ring Dequeue Pointer by programming the
        // Command Ring Control Register (5.4.5) with a 64-bit address pointing to
        // the starting address of the first TRB of the Command Ring.
        self.set_cmd_ring()?;
        self.init_irq()?;
        self.setup_scratchpads()?;
        // At this point, the host controller is up and running and the Root Hub ports
        // (5.4.8) will begin reporting device connects, etc., and system software may begin
        // enumerating devices. System software may follow the procedures described in
        // section 4.3, to enumerate attached devices.
        self.start();
        Ok(())
    }

    fn setup_dcbaap(&mut self) {
        let dcbaa_addr = self.dev_list.dcbaa.bus_addr();
        debug!("DCBAAP: {dcbaa_addr:X}");
        self.reg.operational.dcbaap.update_volatile(|r| {
            r.set(dcbaa_addr);
        });
    }

    fn set_cmd_ring(&mut self) -> Result<(), USBError> {
        let crcr = self.cmd.trbs.bus_addr();
        let cycle = self.cmd.cycle;

        debug!("CRCR: {crcr:X}");
        self.reg.operational.crcr.update_volatile(|r| {
            r.set_command_ring_pointer(crcr);
            if cycle {
                r.set_ring_cycle_state();
            } else {
                r.clear_ring_cycle_state();
            }
        });

        Ok(())
    }

    fn disable_irq(&mut self) {
        debug!("Disable interrupts");
        self.reg.operational.usbcmd.update_volatile(|r| {
            r.clear_interrupter_enable();
        });
    }
    pub fn enable_irq(&mut self) {
        debug!("Enable interrupts");
        self.reg.operational.usbcmd.update_volatile(|r| {
            r.set_interrupter_enable();
        });
    }

    fn init_irq(&mut self) -> Result<(), USBError> {
        let erstz = self.event_ring.len();
        let erdp = self.event_ring.erdp();
        let erstba = self.event_ring.erstba();

        {
            let mut ir0 = self.reg.interrupter_register_set.interrupter_mut(0);

            debug!("ERDP: {erdp:x}");

            ir0.erdp.update_volatile(|r| {
                r.set_event_ring_dequeue_pointer(erdp);
                r.set_dequeue_erst_segment_index(0);
                r.clear_event_handler_busy();
            });

            debug!("ERSTZ: {erstz:x}");
            ir0.erstsz.update_volatile(|r| r.set(erstz as _));
            debug!("ERSTBA: {erstba:X}");
            ir0.erstba.update_volatile(|r| {
                r.set(erstba);
            });

            ir0.imod.update_volatile(|im| {
                im.set_interrupt_moderation_interval(0x1F);
                im.set_interrupt_moderation_counter(0);
            });
        }

        {
            debug!("Enabling primary interrupter.");
            self.reg
                .interrupter_register_set
                .interrupter_mut(0)
                .iman
                .update_volatile(|im| {
                    im.set_interrupt_enable();
                    im.clear_interrupt_pending();
                });
        }

        /* Set the HCD state before we enable the irqs */
        self.reg.operational.usbcmd.update_volatile(|r| {
            r.set_host_system_error_enable();
            r.set_enable_wrap_event();
        });
        Ok(())
    }

    fn setup_scratchpads(&mut self) -> Result<(), USBError> {
        let scratchpad_buf_arr = {
            let buf_count = {
                let count = self
                    .reg
                    .capability
                    .hcsparams2
                    .read_volatile()
                    .max_scratchpad_buffers();
                debug!("Scratch buf count: {count}");
                count
            };
            if buf_count == 0 {
                return Ok(());
            }
            let scratchpad_buf_arr = ScratchpadBufferArray::new(buf_count as _)?;

            let bus_addr = scratchpad_buf_arr.bus_addr();

            self.dev_list.dcbaa.set(0, bus_addr);

            debug!("Setting up {buf_count} scratchpads, at {bus_addr:#0x}");
            scratchpad_buf_arr
        };

        self.scratchpad_buf_arr = Some(scratchpad_buf_arr);

        Ok(())
    }

    fn start(&mut self) {
        self.reg.operational.usbcmd.update_volatile(|r| {
            r.set_run_stop();
        });
        debug!("Start run");
    }

    pub fn handle_event(&mut self) {
        let erdp = {
            self.clean_events();
            self.event_ring.erdp()
        };
        {
            let mut irq = self.reg.interrupter_register_set.interrupter_mut(0);

            irq.erdp.update_volatile(|r| {
                r.set_event_ring_dequeue_pointer(erdp);
                r.clear_event_handler_busy();
            });

            irq.iman.update_volatile(|r| {
                r.clear_interrupt_pending();
            });
        }
    }

    fn clean_events(&mut self) -> usize {
        let mut count = 0;

        while let Some(allowed) = self.event_ring.next() {
            unsafe {
                match allowed {
                    Allowed::CommandCompletion(c) => {
                        let addr = c.command_trb_pointer();
                        trace!("[Command] << {allowed:?} @{addr:X}");
                        self.wait_cmd.set_result(addr, c);
                    }
                    Allowed::PortStatusChange(st) => {
                        debug!("port change: {}", st.port_id());
                    }
                    Allowed::TransferEvent(c) => {
                        let addr = c.trb_pointer();
                        trace!("[Transfer] << {allowed:?} @{addr:X}");
                        debug!("transfer event: {c:?}");

                        self.wait_transfer.set_result(c.trb_pointer(), c)
                    }
                    _ => {
                        debug!("unhandled event {allowed:?}");
                    }
                }
            }
            count += 1;
        }

        count
    }

    pub fn reset_ports(&mut self) {
        let regs = &mut self.reg;
        let port_len = regs.port_register_set.len();

        for i in 0..port_len {
            debug!("Port {i} start reset",);
            regs.port_register_set.update_volatile_at(i, |port| {
                port.portsc.set_0_port_enabled_disabled();
                port.portsc.set_port_reset();
            });
        }
        for i in 0..port_len {
            while regs
                .port_register_set
                .read_volatile_at(i)
                .portsc
                .port_reset()
            {
                spin_loop();
            }
        }
    }

    pub fn cmd_request(&mut self, trb: command::Allowed) -> Waiter<CommandCompletion> {
        let trb_addr = self.cmd.enque_command(trb);
        wmb();
        self.reg
            .doorbell
            .write_volatile_at(0, doorbell::Register::default());

        self.wait_cmd.try_wait_for_result(trb_addr.raw()).unwrap()
    }

    pub(crate) fn litsen_transfer(&mut self, ring: &Ring) {
        self.wait_transfer
            .append(ring.trb_bus_addr_list().map(|a| a.raw()));
    }

    pub fn port_speed(&self, port: PortId) -> u8 {
        self.reg
            .port_register_set
            .read_volatile_at(port.raw() - 1)
            .portsc
            .port_speed()
    }

    /// 解析端口速度 ID 为实际速度 (Mbps)
    #[allow(dead_code)]
    pub fn parse_port_speed_to_mbps(&self, port: PortId) -> Option<u32> {
        let speed_id = self.port_speed(port);
        if speed_id == 0 {
            return None; // PSIV 值 0 是保留的
        }

        // TODO: 从 xHCI Extended Capabilities 中查找对应的 PSI
        // 现在先返回常见速度的硬编码值
        match speed_id {
            1 => Some(12),    // USB 1.x Full Speed (12 Mbps)
            2 => Some(480),   // USB 2.0 High Speed (480 Mbps)
            3 => Some(5000),  // USB 3.0 SuperSpeed (5 Gbps = 5000 Mbps)
            4 => Some(10000), // USB 3.1 SuperSpeedPlus (10 Gbps)
            5 => Some(20000), // USB 3.2 SuperSpeedPlus (20 Gbps)
            _ => None,
        }
    }

    /// 获取端口速度的描述字符串
    #[allow(dead_code)]
    pub fn port_speed_description(&self, port: PortId) -> &'static str {
        let speed_id = self.port_speed(port);
        match speed_id {
            0 => "Unknown",
            1 => "Full Speed (12 Mbps)",
            2 => "High Speed (480 Mbps)",
            3 => "SuperSpeed (5 Gbps)",
            4 => "SuperSpeedPlus (10 Gbps)",
            5 => "SuperSpeedPlus (20 Gbps)",
            _ => "Reserved/Unknown",
        }
    }

    /// 根据端口速度计算默认控制端点的最大包大小
    /// 基于 USB 规范和 xHCI 规范 4.3.3 Address Assignment
    #[allow(dead_code)]
    pub fn calculate_default_max_packet_size(&self, port: PortId) -> u16 {
        let speed_id = self.port_speed(port);
        match speed_id {
            0 => 8,  // Unknown/Reserved - 使用最小值
            1 => 64, // Full Speed (USB 1.1): 8, 16, 32, 或 64 字节
            // 规范建议初始化时使用 64，后续通过设备描述符确认实际值
            2 => 64,  // High Speed (USB 2.0): 固定 64 字节
            3 => 512, // SuperSpeed (USB 3.0): 固定 512 字节
            4 => 512, // SuperSpeedPlus (USB 3.1): 固定 512 字节
            5 => 512, // SuperSpeedPlus (USB 3.2): 固定 512 字节
            _ => 8,   // 其他/未知速度 - 使用最保守的值
        }
    }

    /// 获取特定速度的详细包大小信息
    #[allow(dead_code)]
    pub fn get_packet_size_info(&self, port: PortId) -> (u16, &'static str) {
        let speed_id = self.port_speed(port);
        match speed_id {
            0 => (8, "Unknown speed, using minimum"),
            1 => (64, "Full Speed: 8/16/32/64 bytes possible, using 64"),
            2 => (64, "High Speed: fixed 64 bytes"),
            3 => (512, "SuperSpeed: fixed 512 bytes"),
            4 => (512, "SuperSpeedPlus 10G: fixed 512 bytes"),
            5 => (512, "SuperSpeedPlus 20G: fixed 512 bytes"),
            _ => (8, "Reserved/Unknown speed, using minimum"),
        }
    }

    // pub fn is_64_byte(&self) -> bool {
    //     self.reg
    //         .capability
    //         .hccparams1
    //         .read_volatile()
    //         .addressing_capability()
    // }
}

#[derive(Clone)]
pub(crate) struct RootHub {
    inner: Arc<MutexRoot>,
}

impl RootHub {
    pub fn new(max_slots: usize, reg: XhciRegisters) -> Result<Self, USBError> {
        Ok(Self {
            inner: Arc::new(MutexRoot::new(Root::new(max_slots, reg)?)),
        })
    }

    pub fn try_lock(&self) -> Option<MutexGuard<'_>> {
        self.inner.try_lock()
    }

    pub fn lock(&self) -> MutexGuard<'_> {
        loop {
            if let Some(g) = self.inner.try_lock() {
                return g;
            }
        }
    }

    pub fn init(&self) -> Result<(), USBError> {
        self.try_lock().unwrap().init()
    }

    #[allow(clippy::mut_from_ref)]
    pub unsafe fn force_use(&self) -> &mut Root {
        unsafe { self.inner.force_use() }
    }

    pub async fn wait_for_running(&self) {
        let mut reg = { self.lock().reg.clone() };
        trace!("Waiting for HC to run");
        while reg.operational.usbsts.read_volatile().hc_halted() {
            sleep(Duration::from_millis(10)).await;
            trace!("Waiting for HC to run");
        }

        info!("Running");

        reg.doorbell
            .write_volatile_at(0, doorbell::Register::default());
    }

    pub async fn post_cmd(&self, trb: command::Allowed) -> Result<CommandCompletion, USBError> {
        let fur = self.lock().cmd_request(trb);
        let res = fur.await;
        match res.completion_code() {
            Ok(code) => {
                if matches!(code, CompletionCode::Success) {
                    Ok(res)
                } else {
                    Err(USBError::TransferEventError(code))
                }
            }
            Err(_e) => Err(USBError::Unknown),
        }
    }

    pub unsafe fn reg(&self) -> XhciRegisters {
        unsafe { self.force_use().reg.clone() }
    }

    async fn device_slot_assignment(&self) -> Result<SlotId, USBError> {
        // enable slot
        let result = self
            .post_cmd(command::Allowed::EnableSlot(command::EnableSlot::default()))
            .await?;

        let slot_id = result.slot_id();
        trace!("assigned slot id: {slot_id}");
        Ok(slot_id.into())
    }

    pub async fn new_device(&self, port_idx: usize) -> Result<Device, USBError> {
        debug!("New device on port {port_idx}");
        let slot_id = self.device_slot_assignment().await?;
        debug!("Slot {slot_id} assigned");
        let mut device = {
            let mut root = self.lock();
            let is_64 = root
                .reg
                .capability
                .hccparams1
                .read_volatile()
                .context_size();
            debug!(
                "Creating new context for slot {slot_id}, {}",
                if is_64 { "64-bit" } else { "32-bit" }
            );
            let ctx = root.dev_list.new_ctx(slot_id, is_64)?;
            let device = Device::new(slot_id, self, ctx, (port_idx + 1).into())?;
            root.litsen_transfer(&device.ctrl_ep.ring);
            device
        };

        device.init().await?;
        Ok(device)
    }

    pub(crate) unsafe fn try_wait_for_transfer(
        &self,
        addr: BusAddr,
    ) -> Option<Waiter<TransferEvent>> {
        let inner = unsafe { self.force_use() };
        inner.wait_transfer.try_wait_for_result(addr.raw())
    }
}

pub struct MutexRoot {
    inner: UnsafeCell<Root>,
    lock: AtomicBool,
}

unsafe impl Send for MutexRoot {}
unsafe impl Sync for MutexRoot {}

impl MutexRoot {
    pub fn new(inner: Root) -> Self {
        Self {
            inner: UnsafeCell::new(inner),
            lock: AtomicBool::new(false),
        }
    }

    pub fn try_lock(&self) -> Option<MutexGuard<'_>> {
        if self
            .lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            let inner = unsafe { &mut *self.inner.get() };
            let irq_guard = inner.reg.disable_irq_guard();
            Some(MutexGuard {
                inner: self,
                _irq_guard: irq_guard,
            })
        } else {
            None
        }
    }

    #[allow(clippy::mut_from_ref)]
    pub unsafe fn force_use(&self) -> &mut Root {
        let inner = self.inner.get();
        unsafe { &mut *inner }
    }
}

pub struct MutexGuard<'a> {
    inner: &'a MutexRoot,
    _irq_guard: DisableIrqGuard,
}

impl Deref for MutexGuard<'_> {
    type Target = Root;

    fn deref(&self) -> &Self::Target {
        unsafe { self.inner.force_use() }
    }
}

impl DerefMut for MutexGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.inner.force_use() }
    }
}

impl Drop for MutexGuard<'_> {
    fn drop(&mut self) {
        self.inner.lock.store(false, Ordering::Release);
    }
}
