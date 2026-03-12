//! DTO subscription history (SUBSCRIPTION_HISTORY join SUBSCRIBER/ETAG), dùng khi tính phí.

/// Một bản ghi subscription history — nguồn RATING_OWNER.SUBSCRIPTION_HISTORY (join CRM SUBSCRIBER/ETAG, PRICE).
/// Khi có price_id: dùng bảng giá của subscription cho rating detail, không lấy theo segment.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SubscriptionHistoryDto {
    pub stage_id: i64,
    pub subscription_id: i64,
    pub price_type: Option<String>,
    /// PRICE_ID từ SUBSCRIPTION_HISTORY/PRICE. Có giá trị thì dùng price của subscription, không lấy giá theo segment.
    #[serde(default)]
    pub price_id: Option<i64>,
    /// BOT_ID từ BOT_TOLL (join PRICE). Đi kèm price_id cho rating detail.
    #[serde(default)]
    pub bot_id: Option<i64>,
    /// BOO của stage (từ TOLL_STAGE.BOO). None/empty = áp dụng mọi BOO.
    pub stage_boo: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}
