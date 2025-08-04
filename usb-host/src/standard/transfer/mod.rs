pub mod control;

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
    pub(crate) fn from_address(addr: u8) -> Direction {
        match addr & Self::MASK {
            0 => Self::Out,
            _ => Self::In,
        }
    }
    pub(crate) const MASK: u8 = 0x80;
}
