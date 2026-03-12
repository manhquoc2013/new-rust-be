//! Cập nhật ETDR từ response commit/checkout.
//! BOO check-in commit: nhận CHECKIN_COMMIT_BOO_RESP (3B, 0x69) sau khi gửi CHECKIN_COMMIT_BOO (3A, 0x68).

use crate::utils::timestamp_ms;

use super::ETDR;

impl ETDR {
    /// Update ETDR from BOO CHECKIN_COMMIT_BOO_RESP (3B) – VDTC path.
    pub fn update_from_commit(
        &mut self,
        vdtc_commit_resp: &crate::models::bect_messages::CHECKIN_COMMIT_BOO_RESP,
    ) {
        let now = timestamp_ms();

        self.status = vdtc_commit_resp.status; // 0 = success, non-zero = error
        self.checkin_commit_datetime = now;
        self.time_route_checkin_commit = now;
        self.ref_trans_id = vdtc_commit_resp.ref_trans_id;
    }

    /// Update ETDR from BOO CHECKIN_COMMIT_BOO_RESP (3B) – VETC path.
    pub fn update_from_commit_vetc(
        &mut self,
        vetc_commit_resp: &crate::models::bect_messages::CHECKIN_COMMIT_BOO_RESP,
    ) {
        let now = timestamp_ms();

        self.status = vetc_commit_resp.status; // 0 = success, non-zero = error
        self.checkin_commit_datetime = now;
        self.time_route_checkin_commit = now;
        self.ref_trans_id = vetc_commit_resp.ref_trans_id;
    }

    /// Update ETDR from BOO CHECKOUT_RESERVE_BOO and CHECKOUT_RESERVE_BOO_RESP (VETC path).
    pub fn update_from_checkout_vetc(
        &mut self,
        vetc_checkout: &crate::models::bect_messages::CHECKOUT_RESERVE_BOO,
        vetc_checkout_resp: &crate::models::bect_messages::CHECKOUT_RESERVE_BOO_RESP,
    ) {
        let now = timestamp_ms();

        self.status = vetc_checkout_resp.status; // 0 = success, non-zero = error
        self.checkout_datetime = now;
        self.time_route_checkout = now;
        self.time_update = now;
        self.toll_out = vetc_checkout.station_out;
        self.lane_out = vetc_checkout.lane_out;
        self.ticket_out_id = Some(self.ticket_id);
        if let Some(id) = vetc_checkout_resp.hub_id {
            if id > 0 {
                self.hub_id = Some(id);
            }
        }
        self.ticket_eTag_id = Some(vetc_checkout_resp.ticket_eTag_id);
        self.price = vetc_checkout.trans_amount;
        self.boo_trans_amount = vetc_checkout.trans_amount;
        self.boo_trans_datetime = Some(vetc_checkout.trans_datetime);
        self.boo_lane_type = Some("O".to_string());
    }

    /// Update ETDR from BOO CHECKOUT_RESERVE_BOO and CHECKOUT_RESERVE_BOO_RESP (VDTC path).
    pub fn update_from_checkout(
        &mut self,
        vdtc_checkout: &crate::models::bect_messages::CHECKOUT_RESERVE_BOO,
        vdtc_checkout_resp: &crate::models::bect_messages::CHECKOUT_RESERVE_BOO_RESP,
    ) {
        let now = timestamp_ms();

        self.status = vdtc_checkout_resp.status; // 0 = success, non-zero = error
        self.checkout_datetime = now;
        self.time_route_checkout = now;
        self.time_update = now;
        self.toll_out = vdtc_checkout.station_out;
        self.lane_out = vdtc_checkout.lane_out;
        self.ticket_out_id = Some(self.ticket_id);
        if let Some(id) = vdtc_checkout_resp.hub_id {
            if id > 0 {
                self.hub_id = Some(id);
            }
        }
        self.ticket_eTag_id = Some(vdtc_checkout_resp.ticket_eTag_id);
        self.price = vdtc_checkout.trans_amount;
        self.boo_trans_amount = vdtc_checkout.trans_amount;
        self.boo_trans_datetime = Some(vdtc_checkout.trans_datetime);
        self.boo_lane_type = Some("O".to_string());
    }

    /// Cập nhật thông tin checkout từ FE_CHECKIN (cho BECT - không cần gọi VDTC)
    pub fn update_from_checkout_local(
        &mut self,
        fe_checkin: &crate::models::TCOCmessages::FE_CHECKIN,
        trans_amount: i32,
    ) {
        let now = timestamp_ms();

        self.status = 0; // BECT checkout local: success
        self.checkout_datetime = now;
        self.time_route_checkout = now;
        self.time_update = now;
        self.toll_out = fe_checkin.station;
        self.lane_out = fe_checkin.lane;
        self.price = trans_amount;
        self.boo_trans_amount = trans_amount;
        self.boo_trans_datetime = Some(now);
        self.boo_lane_type = Some("O".to_string());
        if !fe_checkin.plate.trim().is_empty() {
            self.plate = fe_checkin.plate.trim().to_string();
        }
    }

    /// Update ETDR from CHECKOUT_RESERVE_BOO (BECT local checkout at exit station).
    pub fn update_from_checkout_local_boo(
        &mut self,
        req: &crate::models::bect_messages::CHECKOUT_RESERVE_BOO,
        trans_amount: i32,
    ) {
        let now = timestamp_ms();

        self.status = 0;
        self.checkout_datetime = now;
        self.time_route_checkout = now;
        self.time_update = now;
        self.toll_out = req.station_out;
        self.lane_out = req.lane_out;
        self.price = trans_amount;
        self.boo_trans_amount = trans_amount;
        self.boo_trans_datetime = Some(now);
        self.boo_lane_type = Some("O".to_string());
        if !req.plate.trim().is_empty() {
            self.plate = req.plate.trim().to_string();
        }
    }
}
