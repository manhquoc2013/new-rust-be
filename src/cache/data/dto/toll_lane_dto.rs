//! DTO làn thu phí (toll_lane), map với TOLL_LANE trong cache.

use serde::{Deserialize, Serialize};

use crate::models::TollCache::TOLL_LANE;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TollLaneDto {
    pub toll_lane_id: i32,
    pub toll_id: i32,
    pub lane_code: i32,
    pub lane_type: String,
    pub lane_name: String,
    pub status: String,
    pub free_lanes: String,
    pub free_lanes_limit: Option<i32>,
    pub free_lanes_second: String,
    pub free_toll_id: Option<i32>,
    pub dup_filter: String,
    pub free_limit_time: String,
    pub free_allow_loop: String,
    pub transit_lane: String,
    pub freeflow_lane: String,
}

impl TollLaneDto {
    /// Convert TollLaneDto to TOLL_LANE model
    pub fn to_toll_lane(&self) -> TOLL_LANE {
        TOLL_LANE {
            toll_lane_id: self.toll_lane_id,
            toll_id: self.toll_id,
            lane_code: self.lane_code,
            lane_type: self.lane_type.clone(),
            lane_name: self.lane_name.clone(),
            status: self.status.clone(),
            free_lanes: self.free_lanes.clone(),
            free_lanes_limit: self.free_lanes_limit,
            free_lanes_second: self.free_lanes_second.clone(),
            free_toll_id: self.free_toll_id,
            dup_filter: self.dup_filter.clone(),
            free_limit_time: self.free_limit_time.clone(),
            free_allow_loop: self.free_allow_loop.clone(),
            transit_lane: self.transit_lane.clone(),
            freeflow_lane: self.freeflow_lane.clone(),
        }
    }
}
