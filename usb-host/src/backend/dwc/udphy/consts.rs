pub const fn genmask(high: u32, low: u32) -> u64 {
    assert!(high < 64 && low < 64);
    assert!(high >= low);
    (u64::MAX << low) & (u64::MAX >> (63 - high))
}

pub const CMN_LANE_MUX_AND_EN_OFFSET: u32 = 0x0288;

pub const CMN_DP_LANE_MUX_ALL: u32 = genmask(7, 4) as u32;
pub const CMN_DP_LANE_EN_ALL: u32 = genmask(3, 0) as u32;

pub const PHY_LANE_MUX_USB: u32 = 0;
pub const PHY_LANE_MUX_DP: u32 = 1;