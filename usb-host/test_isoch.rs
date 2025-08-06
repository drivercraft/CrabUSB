// 临时测试文件，用于探索Isoch TRB的API
use xhci::ring::trb::transfer::Isoch;

fn main() {
    let mut isoch = Isoch::new();

    // 探索可用的方法
    isoch
        .set_data_buffer_pointer(0)
        .set_trb_transfer_length(0)
        .set_interrupter_target(0)
        .set_interrupt_on_completion();

    // 尝试其他可能的方法
    // isoch.set_transfer_burst_count();
    // isoch.set_transfer_last_burst_packet_count();
    // isoch.set_frame_id();
    // isoch.set_start_isoch_asap();
}
