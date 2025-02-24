use num_enum::{FromPrimitive, IntoPrimitive};

use super::Direction;

#[derive(Debug, Clone)]
pub struct ControlTransfer {
    pub request_type: RequestType,
    pub request: Request,
    pub index: u16,
    pub value: u16,
    /// (ptr, len)
    pub data: Option<(usize, u16)>,
}

#[derive(Debug, Clone, FromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Request {
    GetStatus = 0,
    ClearFeature = 1,
    SetFeature = 3,
    SetAddress = 5,
    GetDescriptor = 6,
    SetDescriptor = 7,
    GetConfiguration = 8,
    SetConfiguration = 9,
    GetInterface = 10,
    SetInterface = 11,
    SynchFrame = 12,
    SetEncryption = 13,
    GetEncryption = 14,
    SetHandshake = 15,
    GetHandshake = 16,
    SetConnection = 17,
    SetSecurityData = 18,
    GetSecurityData = 19,
    SetWusbData = 20,
    LoopbackDataWrite = 21,
    LoopbackDataRead = 22,
    SetInterfaceDs = 23,
    GetFwStatus = 26,
    SetFwStatus = 27,
    SetSel = 48,
    SetIsochDelay = 49,
    #[num_enum(catch_all)]
    Other(u8),
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct RequestType {
    pub direction: Direction,
    pub transfer_type: TransferType,
    pub recipient: Recipient,
}

impl RequestType {
    pub const fn new(
        direction: Direction,
        transfer_type: TransferType,
        recipient: Recipient,
    ) -> RequestType {
        RequestType {
            direction,
            transfer_type,
            recipient,
        }
    }
}

impl From<RequestType> for u8 {
    fn from(value: RequestType) -> Self {
        ((value.direction as u8) << 7) | ((value.transfer_type as u8) << 5) | value.recipient as u8
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum TransferType {
    Standard = 0,
    Class = 1,
    Vendor = 2,
    Reserved = 3,
}

#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum Recipient {
    Device = 0,
    Interface = 1,
    Endpoint = 2,
    Other = 3,
}
