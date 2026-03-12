//! DTO EX_LIST (danh sách ngoại lệ theo station/vehicle_type/etag).
//! Định nghĩa bảng: STATION_ID (stage_id trạm kín; toll_id trạm mở; 0 toàn quốc), TYPE_STATION (0 toàn quốc; 1 trạm mở; 2 trạm kín), STATUS '11' đã duyệt.

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExListItemDto {
    pub station_id: i64,
    /// Vehicle type used for price lookup (ưu tiên VEHICLE_TYPE_BOO rồi VEHICLE_TYPE).
    pub vehicle_type: String,
    pub etag: Option<String>,
    pub plate: Option<String>,
    pub effect_date: Option<String>,
    pub expire_date: Option<String>,
    pub vehicle_type_profile: Option<String>,
    pub boo: Option<String>,
    pub vehicle_type_boo: Option<String>,
    /// Scope: 0 toàn quốc, 1 trạm mở, 2 trạm kín (TYPE_STATION).
    pub type_station: Option<String>,
}
