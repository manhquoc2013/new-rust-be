//! DTO danh sách whitelist/blacklist route (WB_LIST).
//! STATION_ID: cycle_id (trạm kín), toll_id (trạm mở), 0 = toàn quốc. CYCLE_ID: NULL hoặc 0 = áp dụng toàn quốc; > 0 = chỉ áp dụng khi cycle của đoạn trùng.

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WbListRouteDto {
    /// STATION_ID: cycle_id (trạm kín), toll_id (trạm mở), 0 = toàn quốc.
    pub toll_id: i64,
    /// CYCLE_ID trong WB: NULL hoặc 0 = áp dụng toàn quốc (không lọc theo cycle); > 0 = chỉ áp dụng khi cycle_id đoạn (tollA,tollB) trùng.
    pub cycle_id: Option<i64>,
    /// ITEM_TYPE: W = Whitelist, B = Blacklist.
    pub item_type: String,
    pub etag: Option<String>,
    pub plate: Option<String>,
    pub plate_type: Option<String>,
    pub effect_date: Option<String>,
    pub expire_date: Option<String>,
    pub type_station: Option<String>,
    pub boo: Option<String>,
    pub vehicle_type: Option<String>,
}
