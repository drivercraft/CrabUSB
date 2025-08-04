use xhci::ring::trb::transfer;

use crate::standard::transfer::Direction;

define_int_type!(SlotId, u8);

impl SlotId {
    pub fn as_u8(&self) -> u8 {
        self.0
    }

    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }
}

define_int_type!(Dci, u8);

impl Dci {
    pub const CTRL: Self = Self(1);

    pub fn as_u8(&self) -> u8 {
        self.0
    }

    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }
}

impl From<Direction> for transfer::TransferType {
    fn from(value: Direction) -> Self {
        match value {
            Direction::In => transfer::TransferType::In,
            Direction::Out => transfer::TransferType::Out,
        }
    }
}
impl From<Direction> for transfer::Direction {
    fn from(value: Direction) -> Self {
        match value {
            Direction::In => transfer::Direction::In,
            Direction::Out => transfer::Direction::Out,
        }
    }
}
