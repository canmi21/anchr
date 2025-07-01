/* src/wsm/header.rs */

/**
 * @file header.rs
 * @brief WSM Rev3 protocol 8-byte header definition and builder
 * @copyright Copyright (C) 2025 Canmi, all rights reserved.
 */

use std::convert::TryFrom;

// Operation code for identifying the protocol message type.
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum OpCode {
    Ping = 0x01,
    Auth = 0x02,
    Data = 0x03,
    Echo = 0x04,
    /* 0x01 - 0x10 are reserved for wsm std, other can be used by custom endpoints */
    Custom = 0xFF,
}

impl TryFrom<&str> for OpCode {
    type Error = ();

    fn try_from(name: &str) -> Result<Self, Self::Error> {
        match name.to_lowercase().as_str() {
            "ping" => Ok(OpCode::Ping),
            "auth" => Ok(OpCode::Auth),
            "data" => Ok(OpCode::Data),
            "echo" => Ok(OpCode::Echo),
            "custom" => Ok(OpCode::Custom),
            _ => Err(()),
        }
    }
}

// Type of the payload following the header
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum PayloadType {
    Json = 0x01,
    Bincode = 0x02,
    Raw = 0x03,
    Custom = 0xFF,
}

impl TryFrom<&str> for PayloadType {
    type Error = ();

    fn try_from(name: &str) -> Result<Self, Self::Error> {
        match name.to_lowercase().as_str() {
            "json" => Ok(PayloadType::Json),
            "bincode" => Ok(PayloadType::Bincode),
            "raw" => Ok(PayloadType::Raw),
            "custom" => Ok(PayloadType::Custom),
            _ => Err(()),
        }
    }
}

// Represents a full 8-byte header
#[derive(Debug, Clone, Copy)]
pub struct WsmHeader {
    pub opcode: u8,
    pub message_id: u8,
    pub payload_type: u8,
    pub reserved: u8,
    pub payload_len: u32,
}

impl WsmHeader {
    pub fn new(opcode: OpCode, message_id: u8, payload_type: PayloadType, payload_len: u32) -> Self {
        WsmHeader {
            opcode: opcode as u8,
            message_id,
            payload_type: payload_type as u8,
            reserved: 0,
            payload_len,
        }
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

    pub fn from_bytes(buf: [u8; 8]) -> Self {
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
