//! Generator ticket_id unique cho các giao dịch ETC (VETC/VDTC/BECT).
//!
//! Ticket_id từ DB: giá trị seq trực tiếp (i64) từ TRANSPORT_TRANS_STAGE_SEQ, dùng chung cho cả BOO và BECT.
//! Kiểm tra trùng ở cả hai bảng TRANSPORT_TRANSACTION_STAGE và BOO_TRANSPORT_TRANS_STAGE (và ETDR cache).
//!
//! Local: UUID → hash (XXH3) → 64-bit number với MSB = 1.
//! Luồng: Service (BOO/BECT) lấy seq từ DB → kiểm tra trùng → prefix + suffix atomic (1..suffix_max, ENV TICKET_ID_SUFFIX_MAX);
//! khi suffix >= prefetch_threshold prefetch seq tiếp theo; khi suffix = suffix_max đổi sang prefix mới. Fallback: generate_ticket_id_async().

use uuid::Uuid;
use xxhash_rust::xxh3::xxh3_64;

use crate::constants::ticket_id as ticket_id_consts;

/// Sinh ticket_id local: UUID → hash XXH3 chuỗi UUID → 64-bit number, MSB cố định = 1.
#[inline]
pub fn generate_ticket_id_local() -> i64 {
    let u = Uuid::new_v4();
    let s = u.to_string();
    let value = xxh3_64(s.as_bytes());
    let with_msb = value | (1u64 << 63);
    // Đảm bảo chữ số cuối luôn là TICKET_ID_LOCAL_LAST_DIGIT (2).
    let last_digit_fixed = with_msb - (with_msb % 10) + ticket_id_consts::LOCAL_LAST_DIGIT;
    last_digit_fixed as i64
}

/// Sinh ticket_id (async wrapper, luôn dùng local).
pub async fn generate_ticket_id_async() -> i64 {
    generate_ticket_id_local()
}
