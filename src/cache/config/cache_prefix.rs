//! Enum prefix key cache (Toll, TollLane, Price, ClosedCycleTransitionStage, ...).

use std::fmt;

#[derive(Debug, Clone, Copy)]
pub enum CachePrefix {
    Toll,
    TollLane,
    Price,
    #[allow(dead_code)]
    Blacklist,
    #[allow(dead_code)]
    Whitelist,
    /// Per-segment (TOLL_A, TOLL_B) -> Vec<CLOSED_CYCLE_TRANSITION_ID> from RATING_OWNER.CLOSED_CYCLE_TRANSITION_STAGE. Key = prefix:lo:hi.
    ClosedCycleTransitionStage,
    WbRoute,
    ExList,
    ExPriceList,
    /// Subscription history theo etag (key = prefix:etag, value = Vec<SubscriptionHistoryDto>).
    SubscriptionHistory,
    /// IP block: client bị block do quá nhiều lần kết nối lỗi (key = prefix:ip, value = blocked_until Unix timestamp).
    IpBlock,
    /// Connection server config theo IP (key = prefix:ip, value = TcocConnectionServer). Dùng cho CONNECT when DB fails.
    ConnectionServer,
    /// Connection user theo username:toll_id (key = prefix:username:toll_id, value = TcocUser). Dùng cho CONNECT when DB fails.
    ConnectionUser,
    /// Pending DB write retry: tcoc_session (key = prefix:session_id, value = TcocSession). Ghi khi save DB lỗi; retry task flush vào DB.
    DbRetryTcocSession,
    /// Pending DB write retry: tcoc_connection_server (key = prefix:ip, value = DbRetryConnServerEntry). Insert/update chung queue; id None = insert, Some(id) = update.
    DbRetryConnServer,
    /// Pending DB write retry: tcoc_user (key = prefix:username:toll_id, value = DbRetryTcocUserEntry). Insert/update chung queue; id None = insert, Some(id) = update.
    DbRetryTcocUser,
    /// TCD rating details (BOO): key = prefix:transport_trans_id, value = Vec<BOORatingDetail>. Dùng khi commit lấy rating từ cache trước, fallback DB.
    TcdRatingBoo,
    /// TCD rating details (BECT): key = prefix:transport_trans_id, value = Vec<BOORatingDetail>.
    TcdRatingBect,
}

impl CachePrefix {
    #[inline]
    pub fn as_str(&self) -> &'static str {
        match self {
            CachePrefix::Toll => "1_toll",
            CachePrefix::TollLane => "2_toll_lane",
            CachePrefix::Price => "3_price",
            CachePrefix::Blacklist => "4_blacklist",
            CachePrefix::Whitelist => "5_whitelist",
            CachePrefix::ClosedCycleTransitionStage => "7_closed_cycle_transition_stage",
            CachePrefix::WbRoute => "8_wb_route",
            CachePrefix::ExList => "9_ex_list",
            CachePrefix::ExPriceList => "10_ex_price_list",
            CachePrefix::SubscriptionHistory => "11_subscription_history",
            CachePrefix::IpBlock => "12_ip_block",
            CachePrefix::ConnectionServer => "13_conn_server",
            CachePrefix::ConnectionUser => "14_conn_user",
            CachePrefix::DbRetryTcocSession => "db_retry_tcoc_session",
            CachePrefix::DbRetryConnServer => "db_retry_conn_server",
            CachePrefix::DbRetryTcocUser => "db_retry_tcoc_user",
            CachePrefix::TcdRatingBoo => "15_tcd_rating_boo",
            CachePrefix::TcdRatingBect => "16_tcd_rating_bect",
        }
    }
}

impl fmt::Display for CachePrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
