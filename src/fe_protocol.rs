//! Helper and constants for FE protocol (single message format: 8-byte request_id, session_id, ticket_id).
//!
//! # REQUEST byte layout (i64 IDs: header 24 bytes)
//!
//! ## CONNECT (2.3.1.7.1) – 44 bytes
//! | Field        | Offset  |
//! |--------------|---------|
//! | msg_len      | 0..4    |
//! | command_id   | 4..8    |
//! | version_id   | 8..12   |
//! | request_id   | 12..20  |
//! | username     | 20..30  |
//! | password     | 30..40  |
//! | timeout      | 40..44  |
//!
//! ## CONNECT_RESP (2.3.1.7.2) – 32 bytes: msg_len, command_id, version_id, request_id, session_id, status.
//!
//! ## SHAKE (0C) 2.3.1.7.3 – 28 bytes
//! | Field      | Offset  |
//! |------------|---------|
//! | msg_len    | 0..4    |
//! | command_id | 4..8    |
//! | version_id | 8..12   |
//! | request_id | 12..20  |
//! | session_id | 20..28  |
//!
//! ## TERMINATE (0E) 2.3.1.7.11 – 28 bytes
//! | Field      | Offset  |
//! |------------|---------|
//! | msg_len    | 0..4    |
//! | command_id | 4..8    |
//! | version_id | 8..12   |
//! | request_id | 12..20  |
//! | session_id | 20..28  |
//!
//! ## CHECKIN (106 bytes), COMMIT (98 bytes), ROLLBACK (90 bytes)
//! Header 0..24 (msg_len, command_id, request_id, session_id); then command-specific body.

use crate::constants::fe;
use std::io;
use tokio::io::AsyncWriteExt;

// ---------------------------------------------------------------------------
// Constants: FE request message length (single format: 8-byte request_id/session_id/ticket_id)
// ---------------------------------------------------------------------------

/// Minimum FE request message length (i64 IDs: 8-byte request_id, 8-byte session_id).
pub mod len {
    /// CONNECT (2.3.1.7.1): 4+4+4+8+10+10+4 = 44
    pub const CONNECT: usize = 44;
    /// SHAKE (0C) 2.3.1.7.3: 4+4+4+8+8 = 28
    pub const HANDSHAKE: usize = 28;
    /// TERMINATE (0E) 2.3.1.7.11: 4+4+4+8+8 = 28
    pub const TERMINATE: usize = 28;
    /// CHECKIN: 4+4+8+8 + etag 24 + station 4 + lane 4 + plate 10 + tid 24 + hash 16 = 106
    pub const CHECKIN: usize = 106;
    /// COMMIT: 4+4+8+8 + etag 24 + ... + ticket_id 8 + ... = 98
    pub const COMMIT: usize = 98;
    /// ROLLBACK: 4+4+8+8 + ... + ticket_id 8 + ... = 90
    pub const ROLLBACK: usize = 90;
}

// ---------------------------------------------------------------------------
// Read: request_id, session_id (and ticket_id) from buffer (8-byte IDs)
// ---------------------------------------------------------------------------

/// Parse TERMINATE (0E) 28-byte message: version_id 8..12, request_id 12..20, session_id 20..28.
pub fn parse_terminate_ids(data: &[u8]) -> Option<(i32, i64, i64)> {
    if data.len() < 28 {
        return None;
    }
    let version_id = i32::from_le_bytes(data[8..12].try_into().ok()?);
    let request_id = i64::from_le_bytes(data[12..20].try_into().ok()?);
    let session_id = i64::from_le_bytes(data[20..28].try_into().ok()?);
    Some((version_id, request_id, session_id))
}

/// Parse request_id and session_id after first 8 bytes (message_length, command_id).
/// Buffer must be at least 24 bytes. For SHAKE (0C) use `parse_shake_ids` (28 bytes).
pub fn parse_request_id_session_id(data: &[u8]) -> Option<(i64, i64)> {
    if data.len() < 24 {
        return None;
    }
    let request_id = i64::from_le_bytes(data[8..16].try_into().ok()?);
    let session_id = i64::from_le_bytes(data[16..24].try_into().ok()?);
    Some((request_id, session_id))
}

/// Parse SHAKE (0C) 28-byte message: version_id 8..12, request_id 12..20, session_id 20..28.
pub fn parse_shake_ids(data: &[u8]) -> Option<(i32, i64, i64)> {
    if data.len() < 28 {
        return None;
    }
    let version_id = i32::from_le_bytes(data[8..12].try_into().ok()?);
    let request_id = i64::from_le_bytes(data[12..20].try_into().ok()?);
    let session_id = i64::from_le_bytes(data[20..28].try_into().ok()?);
    Some((version_id, request_id, session_id))
}

/// Offsets (start, end) for body fields: toll_id, etag, lane, plate (use slice data[start..end]).
#[derive(Clone, Copy, Debug)]
pub struct FeBodyOffsets {
    pub toll: (usize, usize),
    pub etag: (usize, usize),
    pub lane: (usize, usize),
    pub plate: (usize, usize),
}

impl FeBodyOffsets {
    #[allow(dead_code)]
    pub fn toll_slice<'a>(&self, data: &'a [u8]) -> Option<&'a [u8]> {
        let (s, e) = self.toll;
        if e > s && data.len() >= e {
            Some(&data[s..e])
        } else {
            None
        }
    }
    #[allow(dead_code)]
    pub fn etag_slice<'a>(&self, data: &'a [u8]) -> Option<&'a [u8]> {
        let (s, e) = self.etag;
        if e > s && data.len() >= e {
            Some(&data[s..e])
        } else {
            None
        }
    }
    #[allow(dead_code)]
    pub fn lane_slice<'a>(&self, data: &'a [u8]) -> Option<&'a [u8]> {
        let (s, e) = self.lane;
        if e > s && data.len() >= e {
            Some(&data[s..e])
        } else {
            None
        }
    }
    #[allow(dead_code)]
    pub fn plate_slice<'a>(&self, data: &'a [u8]) -> Option<&'a [u8]> {
        let (s, e) = self.plate;
        if e > s && data.len() >= e {
            Some(&data[s..e])
        } else {
            None
        }
    }
}

/// Returns body offsets (toll, etag, lane, plate) by command (i64 layout: header 24 bytes).
pub fn fe_body_offsets(command_id: i32) -> FeBodyOffsets {
    match command_id {
        fe::CONNECT => FeBodyOffsets {
            toll: (0, 0),
            etag: (0, 0),
            lane: (0, 0),
            plate: (0, 0),
        },
        fe::CHECKIN => FeBodyOffsets {
            toll: (48, 52),
            etag: (24, 48),
            lane: (52, 56),
            plate: (56, 66),
        },
        fe::COMMIT | fe::ROLLBACK => FeBodyOffsets {
            toll: (48, 52),
            etag: (24, 48),
            lane: (52, 56),
            plate: (68, 78),
        },
        _ => FeBodyOffsets {
            toll: (48, 52),
            etag: (24, 48),
            lane: (52, 56),
            plate: (68, 78),
        },
    }
}

/// Returns ticket_id from COMMIT/ROLLBACK body; None if not COMMIT/ROLLBACK or data too short.
pub fn parse_ticket_id_commit_rollback(data: &[u8], command_id: i32) -> Option<i64> {
    if command_id != fe::COMMIT && command_id != fe::ROLLBACK {
        return None;
    }
    if data.len() >= 64 {
        Some(i64::from_le_bytes(data[56..64].try_into().ok()?))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Write: request_id, session_id (and ticket_id) to buffer (8 bytes each)
// ---------------------------------------------------------------------------

/// Write request_id and session_id to buffer (8 bytes each). Used for CONNECT_RESP, SHAKE_RESP, TERMINATE_RESP, COMMIT_RESP, ROLLBACK_RESP.
pub async fn write_fe_request_id_session_id<W: AsyncWriteExt + Unpin>(
    w: &mut W,
    request_id: i64,
    session_id: i64,
) -> io::Result<()> {
    w.write_i64_le(request_id).await?;
    w.write_i64_le(session_id).await?;
    Ok(())
}

/// Write ticket_id (8 bytes). Used for FE_CHECKIN_IN_RESP.
pub async fn write_fe_ticket_id<W: AsyncWriteExt + Unpin>(
    w: &mut W,
    ticket_id: i64,
) -> io::Result<()> {
    w.write_i64_le(ticket_id).await?;
    Ok(())
}

/// Message length for header+status only (COMMIT_RESP, ROLLBACK_RESP): 28 bytes. TERMINATE_RESP (0F) 2.3.1.7.12 uses `TERMINATE_RESP_LEN` (32). SHAKE_RESP uses `SHAKE_RESP_LEN` (32).
pub fn response_header_status_len() -> i32 {
    28
}

/// TERMINATE_RESP (0F) 2.3.1.7.12: 32 bytes (message_length, command_id, version_id, request_id, session_id, status).
pub const TERMINATE_RESP_LEN: i32 = 32;

/// CONNECT_RESP (2.3.1.7.2): 32 bytes (message_length, command_id, version_id, request_id, session_id, status).
pub const CONNECT_RESP_LEN: i32 = 32;

/// SHAKE_RESP (0D) 2.3.1.7.4: 32 bytes (message_length, command_id, version_id, request_id, session_id, status).
pub const SHAKE_RESP_LEN: i32 = 32;

/// Write CONNECT_RESP body: version_id(4), request_id(8), session_id(8), status(4). Caller writes message_length and command_id first.
pub async fn write_connect_resp_body<W: AsyncWriteExt + Unpin>(
    w: &mut W,
    version_id: i32,
    request_id: i64,
    session_id: i64,
    status: i32,
) -> io::Result<()> {
    w.write_i32_le(version_id).await?;
    w.write_i64_le(request_id).await?;
    w.write_i64_le(session_id).await?;
    w.write_i32_le(status).await?;
    Ok(())
}

/// Write SHAKE_RESP body: version_id(4), request_id(8), session_id(8), status(4). Caller writes message_length and command_id first.
pub async fn write_shake_resp_body<W: AsyncWriteExt + Unpin>(
    w: &mut W,
    version_id: i32,
    request_id: i64,
    session_id: i64,
    status: i32,
) -> io::Result<()> {
    w.write_i32_le(version_id).await?;
    w.write_i64_le(request_id).await?;
    w.write_i64_le(session_id).await?;
    w.write_i32_le(status).await?;
    Ok(())
}

/// Returns message_length for FE_CHECKIN_IN_RESP (i64: 4+4+8+8+4+24+4+4+4+4+4+4+4+10+4+4 = 98).
pub fn response_checkin_in_resp_len() -> i32 {
    98
}
