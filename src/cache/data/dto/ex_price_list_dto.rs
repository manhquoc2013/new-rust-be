//! DTO EX_PRICE_LIST (bảng miễn/giảm giá theo station/price_ticket_type).
//! Định nghĩa bảng: STATION_ID (stage_id trạm kín; toll_id trạm mở; 0 toàn quốc), TYPE_STATION (0/1/2), STATUS '11' đã duyệt.

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExPriceListItemDto {
    pub station_id: i64,
    /// Ticket price type (lưu string, parse i32 khi dùng).
    pub price_ticket_type: String,
    pub etag: Option<String>,
    pub plate: Option<String>,
    pub vehicle_type: Option<String>,
    pub effect_date: Option<String>,
    pub expire_date: Option<String>,
    pub boo: Option<String>,
    pub vehicle_type_boo: Option<String>,
    /// Scope: 0 toàn quốc, 1 trạm mở, 2 trạm kín (TYPE_STATION).
    pub type_station: Option<String>,
}
