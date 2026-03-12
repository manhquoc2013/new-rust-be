//! Tạo ETDR từ FE_CHECKIN, VDTC/VETC response, hoặc từ bản ghi DB (BOO/TRANSPORT stage).

use crate::handlers::checkin::common::ticket_type_str_to_i32;
use crate::utils::timestamp_ms;

use super::ETDR;

impl ETDR {
    /// Tạo ETDR mới với các giá trị mặc định
    pub fn new() -> Self {
        Self::default()
    }

    /// Build ETDR from FE_CHECKIN and BOO CHECKIN_RESERVE_BOO_RESP (VETC path).
    /// `ticket_id` is the ticket ID created when sending CHECKIN_RESERVE_BOO.
    pub fn from_checkin_vetc(
        fe_checkin: &crate::models::TCOCmessages::FE_CHECKIN,
        vetc_checkin_resp: &crate::models::bect_messages::CHECKIN_RESERVE_BOO_RESP,
        vetc_query_resp: &crate::models::bect_messages::QUERY_VEHICLE_BOO_RESP,
        ticket_id: i64,
    ) -> Self {
        let now = timestamp_ms();

        Self {
            time_update: now,
            sub_id: 0,
            etag_id: fe_checkin.etag.clone(),
            etag_key: None,
            etag_number: fe_checkin.etag.clone(),
            account_id: 0,
            vehicle_id: 0,
            status: vetc_checkin_resp.status,
            hash_value: fe_checkin.hash_value.clone(),
            vehicle_type: vetc_query_resp.vehicle_type.to_string(),
            vehicle_type_ext1: String::new(),
            vehicle_type_ext2: String::new(),
            vehicle_type_ext3: String::new(),
            vehicle_type_ext4: String::new(),
            vehicle_type_ext5: String::new(),
            vehicle_type_ext6: String::new(),
            vehicle_type_ext7: String::new(),
            vehicle_type_ext8: String::new(),
            vehicle_type_ext9: String::new(),
            vehicle_type_ext10: String::new(),
            vehicle_type_profile: "STD".to_string(),
            plate: vetc_query_resp.plate.clone(),
            plate_status: 0,
            seat_num: Some(vetc_query_resp.seat),
            weight: 0,
            weight_goods: Some(vetc_query_resp.weight_goods),
            weight_all: Some(vetc_query_resp.weight_all),
            register_vehicle_type: vetc_query_resp.register_vehicle_type.clone(),
            vehicle_length: 0,
            sub_status: String::new(),
            etag_status: String::new(),
            auto_extend: String::new(),
            price_type: String::new(),
            start_date: String::new(),
            request_id: fe_checkin.request_id,
            ticket_id,
            ref_trans_id: vetc_checkin_resp.ref_trans_id,
            command_id: fe_checkin.command_id,
            t_id: fe_checkin.tid.clone(),
            ticket_type: ticket_type_str_to_i32(&vetc_query_resp.ticket_type),
            price_ticket_type: 0,
            price: 0,
            price_id: None,
            commit_amount: 0,
            reason_id: 0,
            image_count: 0,
            station_id: fe_checkin.station,
            station_type: 1,
            lane_id: fe_checkin.lane,
            toll_out: 0,
            lane_out: 0,
            ref_station_id: None,
            ref_lane_id: None,
            time_route_checkin: now,
            time_route_checkin_commit: 0,
            time_route_checkout: 0,
            time_station_checkin: now,
            checkin_datetime: now,
            checkin_commit_datetime: 0,
            checkout_datetime: 0,
            checkout_commit_datetime: 0,
            boo_ticket_id: vetc_checkin_resp.ref_trans_id,
            boo_trans_amount: 0,
            boo_trans_datetime: Some(now),
            boo_lane_type: Some("I".to_string()),
            boo_toll_type: Some("C".to_string()),
            boo_ticket_type: Some(vetc_query_resp.ticket_type.clone()),
            boo_subscription_ids: None,
            charge_in: None,
            sub_charge_in: None,
            charge_in_status: None,
            charge_trans_id: None,
            charge_datetime: None,
            charge_in104: None,
            etdr_dual: None,
            chd_type: None,
            chd_ref_id: None,
            chd_reason: None,
            trans_stage_details: Vec::new(),
            rating_details: Vec::new(),
            transition_close: None,
            toll_turning_time_code: None,
            check: 0,
            log_request: None,
            info_roll_back: None,
            free_turning: false,
            enough_min_balance: vetc_query_resp.min_balance_status == 0,
            rating_type: Some("E".to_string()),
            fe_trans_id: None,
            ticket_in_id: Some(ticket_id),
            ticket_eTag_id: None,
            ticket_out_id: None,
            hub_id: None,
            checkin_from_sync: false,
            event: None,
            db_saved: false,
            db_save_retry_count: 0,
            sync_status: 0, // chưa đồng bộ
        }
    }

    /// Build ETDR from FE_CHECKIN and BOO CHECKIN_RESERVE_BOO_RESP (VDTC path).
    /// ticket_id: ID from sequence/cache (BOO_TRANSPORT_TRANS_STAGE.transport_trans_id).
    pub fn from_checkin(
        fe_checkin: &crate::models::TCOCmessages::FE_CHECKIN,
        vdtc_checkin_resp: &crate::models::bect_messages::CHECKIN_RESERVE_BOO_RESP,
        vdtc_query_resp: &crate::models::bect_messages::QUERY_VEHICLE_BOO_RESP,
        ticket_id: i64,
    ) -> Self {
        let now = timestamp_ms();

        Self {
            time_update: now,
            sub_id: 0,
            etag_id: fe_checkin.etag.clone(),
            etag_key: None,
            etag_number: fe_checkin.etag.clone(),
            account_id: 0,
            vehicle_id: 0,
            status: vdtc_checkin_resp.status,
            hash_value: fe_checkin.hash_value.clone(),
            vehicle_type: vdtc_query_resp.vehicle_type.to_string(),
            vehicle_type_ext1: String::new(),
            vehicle_type_ext2: String::new(),
            vehicle_type_ext3: String::new(),
            vehicle_type_ext4: String::new(),
            vehicle_type_ext5: String::new(),
            vehicle_type_ext6: String::new(),
            vehicle_type_ext7: String::new(),
            vehicle_type_ext8: String::new(),
            vehicle_type_ext9: String::new(),
            vehicle_type_ext10: String::new(),
            vehicle_type_profile: "STD".to_string(),
            plate: vdtc_query_resp.plate.clone(),
            plate_status: 0,
            seat_num: Some(vdtc_query_resp.seat),
            weight: 0,
            weight_goods: Some(vdtc_query_resp.weight_goods),
            weight_all: Some(vdtc_query_resp.weight_all),
            register_vehicle_type: vdtc_query_resp.register_vehicle_type.clone(),
            vehicle_length: 0,
            sub_status: String::new(),
            etag_status: String::new(),
            auto_extend: String::new(),
            price_type: String::new(),
            start_date: String::new(),
            request_id: fe_checkin.request_id,
            ticket_id,
            ref_trans_id: vdtc_checkin_resp.ref_trans_id,
            command_id: fe_checkin.command_id,
            t_id: fe_checkin.tid.clone(),
            ticket_type: ticket_type_str_to_i32(&vdtc_query_resp.ticket_type),
            price_ticket_type: 0,
            price: 0,
            price_id: None,
            commit_amount: 0,
            reason_id: 0,
            image_count: 0,
            station_id: fe_checkin.station,
            station_type: 1,
            lane_id: fe_checkin.lane,
            toll_out: 0,
            lane_out: 0,
            ref_station_id: None,
            ref_lane_id: None,
            time_route_checkin: now,
            time_route_checkin_commit: 0,
            time_route_checkout: 0,
            time_station_checkin: now,
            checkin_datetime: now,
            checkin_commit_datetime: 0,
            checkout_datetime: 0,
            checkout_commit_datetime: 0,
            boo_ticket_id: vdtc_checkin_resp.ref_trans_id,
            boo_trans_amount: 0,
            boo_trans_datetime: Some(now),
            boo_lane_type: Some("I".to_string()),
            boo_toll_type: Some("C".to_string()),
            boo_ticket_type: Some(vdtc_query_resp.ticket_type.clone()),
            boo_subscription_ids: None,
            charge_in: None,
            sub_charge_in: None,
            charge_in_status: None,
            charge_trans_id: None,
            charge_datetime: None,
            charge_in104: None,
            etdr_dual: None,
            chd_type: None,
            chd_ref_id: None,
            chd_reason: None,
            trans_stage_details: Vec::new(),
            rating_details: Vec::new(),
            transition_close: None,
            toll_turning_time_code: None,
            check: 0,
            log_request: None,
            info_roll_back: None,
            free_turning: false,
            enough_min_balance: vdtc_query_resp.min_balance_status == 0,
            rating_type: Some("E".to_string()),
            fe_trans_id: Some(fe_checkin.request_id.to_string()),
            ticket_in_id: Some(ticket_id),
            ticket_eTag_id: None,
            ticket_out_id: None,
            hub_id: None,
            checkin_from_sync: false,
            event: None,
            db_saved: false,
            db_save_retry_count: 0,
            sync_status: 0, // chưa đồng bộ
        }
    }

    /// Tạo ETDR từ FE_CHECKIN cho BECT (ETC) - không cần VDTC response
    pub fn from_checkin_bect(fe_checkin: &crate::models::TCOCmessages::FE_CHECKIN) -> Self {
        let now = timestamp_ms();
        let ref_trans_id = fe_checkin.request_id * 1000000 + (now % 1000000);

        Self {
            time_update: now,
            sub_id: 0,
            etag_id: fe_checkin.etag.clone(),
            etag_key: None,
            etag_number: fe_checkin.etag.clone(),
            account_id: 0,
            vehicle_id: 0,
            status: 0,
            hash_value: fe_checkin.hash_value.clone(),
            vehicle_type: "1".to_string(),
            vehicle_type_ext1: String::new(),
            vehicle_type_ext2: String::new(),
            vehicle_type_ext3: String::new(),
            vehicle_type_ext4: String::new(),
            vehicle_type_ext5: String::new(),
            vehicle_type_ext6: String::new(),
            vehicle_type_ext7: String::new(),
            vehicle_type_ext8: String::new(),
            vehicle_type_ext9: String::new(),
            vehicle_type_ext10: String::new(),
            vehicle_type_profile: "STD".to_string(),
            plate: fe_checkin.plate.clone(),
            plate_status: 0,
            seat_num: None,
            weight: 0,
            weight_goods: None,
            weight_all: None,
            register_vehicle_type: String::new(),
            vehicle_length: 0,
            sub_status: String::new(),
            etag_status: String::new(),
            auto_extend: String::new(),
            price_type: String::new(),
            start_date: String::new(),
            request_id: fe_checkin.request_id,
            ticket_id: ref_trans_id,
            ref_trans_id,
            command_id: fe_checkin.command_id,
            t_id: fe_checkin.tid.clone(),
            ticket_type: 1,
            price_ticket_type: 1,
            price: 0,
            price_id: None,
            commit_amount: 0,
            reason_id: 0,
            image_count: 0,
            station_id: fe_checkin.station,
            station_type: 1,
            lane_id: fe_checkin.lane,
            toll_out: 0,
            lane_out: 0,
            ref_station_id: None,
            ref_lane_id: None,
            time_route_checkin: now,
            time_route_checkin_commit: 0,
            time_route_checkout: 0,
            time_station_checkin: now,
            checkin_datetime: now,
            checkin_commit_datetime: 0,
            checkout_datetime: 0,
            checkout_commit_datetime: 0,
            boo_ticket_id: ref_trans_id,
            boo_trans_amount: 0,
            boo_trans_datetime: Some(now),
            boo_lane_type: None,
            boo_toll_type: None,
            boo_ticket_type: Some("L".to_string()),
            boo_subscription_ids: None,
            charge_in: None,
            sub_charge_in: None,
            charge_in_status: None,
            charge_trans_id: None,
            charge_datetime: None,
            charge_in104: None,
            etdr_dual: None,
            chd_type: None,
            chd_ref_id: None,
            chd_reason: None,
            trans_stage_details: Vec::new(),
            rating_details: Vec::new(),
            transition_close: None,
            toll_turning_time_code: None,
            check: 0,
            log_request: None,
            info_roll_back: None,
            free_turning: false,
            enough_min_balance: true,
            rating_type: Some("E".to_string()),
            fe_trans_id: Some(fe_checkin.request_id.to_string()),
            ticket_in_id: Some(ref_trans_id),
            ticket_eTag_id: None,
            ticket_out_id: None,
            hub_id: None,
            checkin_from_sync: false,
            event: None,
            db_saved: false,
            db_save_retry_count: 0,
            sync_status: 0, // chưa đồng bộ
        }
    }

    /// Tạo ETDR từ bản ghi TRANSPORT_TRANSACTION_STAGE (checkin chưa checkout) cho BECT.
    pub fn from_transport_transaction_stage(
        stage: &crate::db::repositories::TransportTransactionStage,
    ) -> Self {
        let now = timestamp_ms();

        let checkin_datetime_ms = stage
            .checkin_datetime
            .as_deref()
            .and_then(|s| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok())
            .map(|dt| dt.and_utc().timestamp_millis())
            .or_else(|| {
                stage
                    .insert_datetime
                    .as_deref()
                    .and_then(|s| {
                        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()
                    })
                    .map(|dt| dt.and_utc().timestamp_millis())
            })
            .unwrap_or(now);

        let etag_str = stage.etag_number.as_deref().unwrap_or("").to_string();
        let ref_trans_id = stage.transport_trans_id;
        let ticket_id = ref_trans_id;

        Self {
            time_update: now,
            sub_id: stage.subscriber_id.unwrap_or(0) as i32,
            etag_id: etag_str.clone(),
            etag_key: stage.etag_id.map(|id| id as i32),
            etag_number: etag_str,
            account_id: stage.account_id.unwrap_or(0) as i32,
            vehicle_id: stage.vehicle_id.unwrap_or(0) as i32,
            // DB: 1 = success, 0 = error → ETDR/Kafka: 0 = success, non-zero = error
            status: stage
                .status
                .as_deref()
                .map(|s| if s == "1" { 0 } else { 1 })
                .unwrap_or(0),
            hash_value: stage.checkin_shift.as_deref().unwrap_or("").to_string(),
            vehicle_type: stage.vehicle_type.as_deref().unwrap_or("1").to_string(),
            vehicle_type_ext1: String::new(),
            vehicle_type_ext2: String::new(),
            vehicle_type_ext3: String::new(),
            vehicle_type_ext4: String::new(),
            vehicle_type_ext5: String::new(),
            vehicle_type_ext6: String::new(),
            vehicle_type_ext7: String::new(),
            vehicle_type_ext8: String::new(),
            vehicle_type_ext9: String::new(),
            vehicle_type_ext10: String::new(),
            vehicle_type_profile: stage
                .vehicle_type_profile
                .as_deref()
                .unwrap_or("STD")
                .to_string(),
            plate: stage
                .checkin_plate
                .as_deref()
                .or(stage.plate.as_deref())
                .unwrap_or("")
                .to_string(),
            plate_status: stage
                .checkin_plate_status
                .as_deref()
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0),
            seat_num: None,
            weight: stage.fe_weight.unwrap_or(0) as i32,
            weight_goods: None,
            weight_all: None,
            register_vehicle_type: String::new(),
            vehicle_length: stage.fe_vehicle_length.unwrap_or(0) as i32,
            sub_status: String::new(),
            etag_status: String::new(),
            auto_extend: String::new(),
            price_type: String::new(),
            start_date: String::new(),
            request_id: stage.request_id.unwrap_or(0),
            ticket_id,
            ref_trans_id,
            command_id: 0,
            t_id: stage.checkin_tid.as_deref().unwrap_or("").to_string(),
            ticket_type: stage
                .charge_type
                .as_deref()
                .map(ticket_type_str_to_i32)
                .unwrap_or(1),
            price_ticket_type: 1,
            price: 0,
            price_id: None,
            commit_amount: 0,
            reason_id: 0,
            image_count: stage.checkin_img_count.unwrap_or(0),
            station_id: stage.checkin_toll_id.unwrap_or(0) as i32,
            station_type: 1,
            lane_id: stage.checkin_lane_id.unwrap_or(0) as i32,
            toll_out: stage.checkout_toll_id.unwrap_or(0) as i32,
            lane_out: stage.checkout_lane_id.unwrap_or(0) as i32,
            ref_station_id: None,
            ref_lane_id: None,
            time_route_checkin: checkin_datetime_ms,
            time_route_checkin_commit: stage
                .checkin_commit_datetime
                .as_deref()
                .and_then(|s| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok())
                .map(|dt| dt.and_utc().timestamp_millis())
                .unwrap_or(0),
            time_route_checkout: stage
                .checkout_datetime
                .as_deref()
                .and_then(|s| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok())
                .map(|dt| dt.and_utc().timestamp_millis())
                .unwrap_or(0),
            time_station_checkin: checkin_datetime_ms,
            checkin_datetime: checkin_datetime_ms,
            checkin_commit_datetime: stage
                .checkin_commit_datetime
                .as_deref()
                .and_then(|s| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok())
                .map(|dt| dt.and_utc().timestamp_millis())
                .unwrap_or(0),
            checkout_datetime: stage
                .checkout_datetime
                .as_deref()
                .and_then(|s| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok())
                .map(|dt| dt.and_utc().timestamp_millis())
                .unwrap_or(0),
            checkout_commit_datetime: stage
                .checkout_commit_datetime
                .as_deref()
                .and_then(|s| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok())
                .map(|dt| dt.and_utc().timestamp_millis())
                .unwrap_or(0),
            boo_ticket_id: ref_trans_id,
            boo_trans_amount: 0,
            boo_trans_datetime: None,
            boo_lane_type: Some("I".to_string()),
            boo_toll_type: stage
                .toll_type
                .as_ref()
                .cloned()
                .or_else(|| Some("C".to_string())),
            boo_ticket_type: None,
            boo_subscription_ids: None,
            charge_in: stage.charge_in.clone(),
            sub_charge_in: None,
            charge_in_status: stage.charge_in_status.clone(),
            charge_trans_id: stage.charge_trans_id.clone(),
            charge_datetime: stage
                .charge_datetime
                .as_deref()
                .and_then(|s| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok())
                .map(|dt| dt.and_utc().timestamp_millis()),
            charge_in104: stage.charge_in_104.clone(),
            etdr_dual: None,
            chd_type: stage.chd_type.clone(),
            chd_ref_id: stage.chd_ref_id.map(|id| id.to_string()),
            chd_reason: stage.chd_reason.clone(),
            trans_stage_details: Vec::new(),
            rating_details: Vec::new(),
            transition_close: None,
            toll_turning_time_code: stage.turning_code.clone(),
            check: 0,
            log_request: None,
            info_roll_back: None,
            free_turning: false,
            enough_min_balance: true,
            rating_type: stage.rating_type.clone(),
            fe_trans_id: stage.fe_trans_id.clone(),
            ticket_in_id: stage.ticket_in_id,
            ticket_eTag_id: stage.ticket_eTag_id,
            ticket_out_id: stage.ticket_out_id,
            hub_id: stage.hub_id,
            checkin_from_sync: stage.transport_sync_id.is_some(),
            event: None,
            db_saved: true,
            db_save_retry_count: 0,
            sync_status: stage.sync_status.map(|s| s as i32).unwrap_or(0), // 0/None = chưa đồng bộ, 1 = đã đồng bộ
        }
    }
}
