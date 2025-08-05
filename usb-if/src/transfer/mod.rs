#[repr(u8)]
/// The direction of the data transfer.
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub enum Direction {
    /// Out (Write Data)
    Out = 0,
    /// In (Read Data)
    In = 1,
}

impl Direction {
    pub fn from_address(addr: u8) -> Direction {
        match addr & Self::MASK {
            0 => Self::Out,
            _ => Self::In,
        }
    }
    const MASK: u8 = 0x80;
}

#[derive(thiserror::Error, Debug)]
pub enum TransferError {}
