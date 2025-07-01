/* src/wsm/header.rs */

use std::convert::TryFrom;

// Final message flag for reserved field
pub const RESERVED_FINAL_FLAG: u8 = 0xFF;
// Opcode for fatal, unrecoverable errors
pub const OPCODE_ERROR_FATAL: u8 = 0xFF;


#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadType {
    Json = 0x01,
    Bincode = 0x02,
    Raw = 0x03,
    Base64 = 0x04,
    MsgPack = 0x05,
    Custom = 0xFF,
}

impl TryFrom<u8> for PayloadType {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            x if x == PayloadType::Json as u8 => Ok(PayloadType::Json),
            x if x == PayloadType::Bincode as u8 => Ok(PayloadType::Bincode),
            x if x == PayloadType::Raw as u8 => Ok(PayloadType::Raw),
            x if x == PayloadType::Base64 as u8 => Ok(PayloadType::Base64),
            x if x == PayloadType::MsgPack as u8 => Ok(PayloadType::MsgPack),
            x if x == PayloadType::Custom as u8 => Ok(PayloadType::Custom),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WsmHeader {
    pub opcode: u8,
    pub message_id: u8,
    pub payload_type: u8,
    pub reserved: u8,
    pub payload_len: u32,
}

impl WsmHeader {
    pub fn new(opcode: u8, message_id: u8, payload_type: PayloadType, payload_len: u32) -> Self {
        WsmHeader {
            opcode,
            message_id,
            payload_type: payload_type as u8,
            reserved: 0,
            payload_len,
        }
    }

    pub fn with_reserved(
        opcode: u8,
        message_id: u8,
        payload_type: PayloadType,
        payload_len: u32,
        reserved: u8,
    ) -> Self {
        WsmHeader {
            opcode,
            message_id,
            payload_type: payload_type as u8,
            reserved,
            payload_len,
        }
    }

    pub fn is_final(&self) -> bool {
        self.reserved == RESERVED_FINAL_FLAG
    }

    pub fn to_bytes(&self) -> [u8; 8] {
        let mut buf = [0u8; 8];
        buf[0] = self.opcode;
        buf[1] = self.message_id;
        buf[2] = self.payload_type;
        buf[3] = self.reserved;
        buf[4..8].copy_from_slice(&self.payload_len.to_le_bytes());
        buf
    }

    pub fn from_bytes(buf: &[u8; 8]) -> Self {
        let payload_len = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
        WsmHeader {
            opcode: buf[0],
            message_id: buf[1],
            payload_type: buf[2],
            reserved: buf[3],
            payload_len,
        }
    }
}