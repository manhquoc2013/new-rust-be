//! Parse and build CHECKOUT_RESERVE_BOO (2AZ) / CHECKOUT_RESERVE_BOO_RESP (2BZ) per spec 2.3.1.7.17 / 2.3.1.7.18.
//! Request size is variable (depends on rating_detail_line); response is fixed 100 bytes.

use crate::fe_protocol;
use crate::models::bect_messages::{CHECKOUT_RESERVE_BOO, CHECKOUT_RESERVE_BOO_RESP};
use crate::models::rating_detail::RatingDetail;
use std::error::Error;

/// Fixed offset after which rating_detail list and general1/general2 start.
const OFFSET_AFTER_RATING_DETAIL_LINE: usize = 179;
/// Bytes per RatingDetail item in wire format (price_id 8, boo 4, toll_a 4, toll_b 4, ticket_type 1, subscription_id 25, price_ticket_type 4, price_amount 4, vehicle_type 4, bot_id 8, stage_id 8).
const RATING_DETAIL_ITEM_BYTES: usize = 74;
/// general1 8 + general2 16
const TRAILER_BYTES: usize = 24;

/// Parse CHECKOUT_RESERVE_BOO (2AZ) from FE buffer. Normalizes string fields (trim null).
pub fn parse_checkout_reserve_boo(data: &[u8]) -> Result<CHECKOUT_RESERVE_BOO, Box<dyn Error>> {
    if data.len() < fe_protocol::len::CHECKOUT_RESERVE_BOO_MIN {
        return Err(format!(
            "CHECKOUT_RESERVE_BOO message too short: {} bytes (minimum {})",
            data.len(),
            fe_protocol::len::CHECKOUT_RESERVE_BOO_MIN
        )
        .into());
    }
    let mut req = CHECKOUT_RESERVE_BOO::default();
    req.message_length = i32::from_le_bytes(data[0..4].try_into()?);
    req.command_id = i32::from_le_bytes(data[4..8].try_into()?);
    req.version_id = i32::from_le_bytes(data[8..12].try_into()?);
    req.request_id = i64::from_le_bytes(data[12..20].try_into()?);
    req.session_id = i64::from_le_bytes(data[20..28].try_into()?);
    req.timestamp = i64::from_le_bytes(data[28..36].try_into()?);
    req.tid = String::from_utf8_lossy(&data[36..60])
        .trim_end_matches('\0')
        .trim()
        .to_string();
    req.etag = String::from_utf8_lossy(&data[60..84])
        .trim_end_matches('\0')
        .trim()
        .to_string();
    req.ticket_in_id = i64::from_le_bytes(data[84..92].try_into()?);
    let hub_val = i64::from_le_bytes(data[92..100].try_into()?);
    req.hub_id = if hub_val != 0 { Some(hub_val) } else { None };
    req.ticket_eTag_id = i64::from_le_bytes(data[100..108].try_into()?);
    req.ticket_out_id = i64::from_le_bytes(data[108..116].try_into()?);
    req.checkin_datetime = i64::from_le_bytes(data[116..124].try_into()?);
    req.checkin_commit_datetime = i64::from_le_bytes(data[124..132].try_into()?);
    req.station_in = i32::from_le_bytes(data[132..136].try_into()?);
    req.lane_in = i32::from_le_bytes(data[136..140].try_into()?);
    req.station_out = i32::from_le_bytes(data[140..144].try_into()?);
    req.lane_out = i32::from_le_bytes(data[144..148].try_into()?);
    req.plate = String::from_utf8_lossy(&data[148..158])
        .trim_end_matches('\0')
        .trim()
        .to_string();
    let tt = data.get(158).copied().unwrap_or(0);
    req.ticket_type = if tt > 0 {
        char::from(tt).to_string()
    } else {
        String::new()
    };
    req.price_ticket_type = i32::from_le_bytes(data[159..163].try_into()?);
    req.trans_amount = i32::from_le_bytes(data[163..167].try_into()?);
    req.trans_datetime = i64::from_le_bytes(data[167..175].try_into()?);
    req.rating_detail_line = i32::from_le_bytes(data[175..179].try_into()?);

    let n = req.rating_detail_line.max(0) as usize;
    let detail_start = OFFSET_AFTER_RATING_DETAIL_LINE;
    let detail_end = detail_start.saturating_add(n.saturating_mul(RATING_DETAIL_ITEM_BYTES));
    if data.len() >= detail_end {
        for i in 0..n {
            let base = detail_start + i * RATING_DETAIL_ITEM_BYTES;
            let mut rd = RatingDetail::default();
            let price_id_val = i64::from_le_bytes(data[base..base + 8].try_into()?);
            rd.price_id = if price_id_val != 0 {
                Some(price_id_val)
            } else {
                None
            };
            rd.boo = i32::from_le_bytes(data[base + 8..base + 12].try_into()?);
            rd.toll_a_id = i32::from_le_bytes(data[base + 12..base + 16].try_into()?);
            rd.toll_b_id = i32::from_le_bytes(data[base + 16..base + 20].try_into()?);
            let tt_byte = data.get(base + 20).copied().unwrap_or(0);
            rd.ticket_type = if tt_byte > 0 {
                char::from(tt_byte).to_string()
            } else {
                String::new()
            };
            rd.subscription_id = String::from_utf8_lossy(&data[base + 21..base + 46])
                .trim_end_matches('\0')
                .trim()
                .to_string();
            rd.price_ticket_type = i32::from_le_bytes(data[base + 46..base + 50].try_into()?);
            rd.price_amount = i32::from_le_bytes(data[base + 50..base + 54].try_into()?);
            rd.vehicle_type = i32::from_le_bytes(data[base + 54..base + 58].try_into()?);
            let bot_val = i64::from_le_bytes(data[base + 58..base + 66].try_into()?);
            rd.bot_id = if bot_val != 0 { Some(bot_val) } else { None };
            let stage_val = i64::from_le_bytes(data[base + 66..base + 74].try_into()?);
            rd.stage_id = if stage_val != 0 {
                Some(stage_val)
            } else {
                None
            };
            req.rating_detail.push(rd);
        }
    }

    let trailer_start = detail_end;
    if data.len() >= trailer_start + TRAILER_BYTES {
        req.general1
            .copy_from_slice(&data[trailer_start..trailer_start + 8]);
        req.general2
            .copy_from_slice(&data[trailer_start + 8..trailer_start + TRAILER_BYTES]);
    }
    Ok(req)
}

/// Build CHECKOUT_RESERVE_BOO_RESP (2BZ) buffer: 100 bytes (message_length, command_id, version_id, request_id, session_id, timestamp, process_time, ticket_in_id, hub_id, ticket_eTag_id, ticket_out_id, status, general1, general2).
pub fn build_checkout_reserve_boo_resp(resp: &CHECKOUT_RESERVE_BOO_RESP) -> Vec<u8> {
    let cap = fe_protocol::response_checkout_reserve_boo_resp_len() as usize;
    let mut buf = Vec::with_capacity(cap);
    buf.extend_from_slice(&(resp.message_length as i32).to_le_bytes());
    buf.extend_from_slice(&(resp.command_id as i32).to_le_bytes());
    buf.extend_from_slice(&(resp.version_id as i32).to_le_bytes());
    buf.extend_from_slice(&resp.request_id.to_le_bytes());
    buf.extend_from_slice(&resp.session_id.to_le_bytes());
    buf.extend_from_slice(&resp.timestamp.to_le_bytes());
    buf.extend_from_slice(&(resp.process_time as i32).to_le_bytes());
    buf.extend_from_slice(&resp.ticket_in_id.to_le_bytes());
    buf.extend_from_slice(&resp.hub_id.unwrap_or(0).to_le_bytes());
    buf.extend_from_slice(&resp.ticket_eTag_id.to_le_bytes());
    buf.extend_from_slice(&resp.ticket_out_id.to_le_bytes());
    buf.extend_from_slice(&(resp.status as i32).to_le_bytes());
    buf.extend_from_slice(&resp.general1);
    buf.extend_from_slice(&resp.general2);
    buf.resize(cap, 0);
    buf
}
