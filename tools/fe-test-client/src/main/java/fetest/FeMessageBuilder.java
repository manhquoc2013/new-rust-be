package fetest;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.Arrays;

/**
 * Build FE request messages (decrypted payload, little-endian). Caller encrypts and wraps with length.
 */
public final class FeMessageBuilder {

    private static byte[] pad(String s, int len) {
        byte[] b = s.getBytes(java.nio.charset.StandardCharsets.UTF_8);
        byte[] out = new byte[len];
        System.arraycopy(b, 0, out, 0, Math.min(b.length, len));
        return out;
    }

    /** CONNECT: 44 bytes */
    public static byte[] buildConnect(long requestId, String username, String password, int timeout) {
        ByteBuffer b = ByteBuffer.allocate(FeConstants.LEN_CONNECT).order(ByteOrder.LITTLE_ENDIAN);
        b.putInt(FeConstants.LEN_CONNECT);
        b.putInt(FeConstants.CONNECT);
        b.putInt(FeConstants.VERSION_ID_DEFAULT);
        b.putLong(requestId);
        b.put(pad(username, 10));
        b.put(pad(password, 10));
        b.putInt(timeout);
        return b.array();
    }

    /** HANDSHAKE (SHAKE): 28 bytes */
    public static byte[] buildHandshake(long requestId, long sessionId) {
        ByteBuffer b = ByteBuffer.allocate(FeConstants.LEN_HANDSHAKE).order(ByteOrder.LITTLE_ENDIAN);
        b.putInt(FeConstants.LEN_HANDSHAKE);
        b.putInt(FeConstants.HANDSHAKE);
        b.putInt(FeConstants.VERSION_ID_DEFAULT);
        b.putLong(requestId);
        b.putLong(sessionId);
        return b.array();
    }

    /** TERMINATE: 28 bytes */
    public static byte[] buildTerminate(long requestId, long sessionId) {
        ByteBuffer b = ByteBuffer.allocate(FeConstants.LEN_TERMINATE).order(ByteOrder.LITTLE_ENDIAN);
        b.putInt(FeConstants.LEN_TERMINATE);
        b.putInt(FeConstants.TERMINATE);
        b.putInt(FeConstants.VERSION_ID_DEFAULT);
        b.putLong(requestId);
        b.putLong(sessionId);
        return b.array();
    }

    /** CHECKIN: 106 bytes. etag 24, station 4, lane 4, plate 10, tid 24, hash_value 16 */
    public static byte[] buildCheckin(long requestId, long sessionId, String etag, int station, int lane,
                                       String plate, String tid, String hashValue) {
        ByteBuffer b = ByteBuffer.allocate(FeConstants.LEN_CHECKIN).order(ByteOrder.LITTLE_ENDIAN);
        b.putInt(FeConstants.LEN_CHECKIN);
        b.putInt(FeConstants.CHECKIN);
        b.putLong(requestId);
        b.putLong(sessionId);
        b.put(pad(etag != null ? etag : "", 24));
        b.putInt(station);
        b.putInt(lane);
        b.put(pad(plate != null ? plate : "", 10));
        b.put(pad(tid != null ? tid : "", 24));
        b.put(pad(hashValue != null ? hashValue : "", 16));
        return b.array();
    }

    /** COMMIT: 98 bytes. etag 24, station 4, lane 4, ticket_id 8, status 4, plate 10, image_count 4, vehicle_length 4, transaction_amount 4, weight 4, reason_id 4 */
    public static byte[] buildCommit(long requestId, long sessionId, String etag, int station, int lane,
                                      long ticketId, int status, String plate, int imageCount,
                                      int vehicleLength, int transactionAmount, int weight, int reasonId) {
        ByteBuffer b = ByteBuffer.allocate(FeConstants.LEN_COMMIT).order(ByteOrder.LITTLE_ENDIAN);
        b.putInt(FeConstants.LEN_COMMIT);
        b.putInt(FeConstants.COMMIT);
        b.putLong(requestId);
        b.putLong(sessionId);
        b.put(pad(etag != null ? etag : "", 24));
        b.putInt(station);
        b.putInt(lane);
        b.putLong(ticketId);
        b.putInt(status);
        b.put(pad(plate != null ? plate : "", 10));
        b.putInt(imageCount);
        b.putInt(vehicleLength);
        b.putInt(transactionAmount);
        b.putInt(weight);
        b.putInt(reasonId);
        return b.array();
    }

    /** ROLLBACK: 90 bytes */
    public static byte[] buildRollback(long requestId, long sessionId, String etag, int station, int lane,
                                        long ticketId, int status, String plate, int imageCount, int weight, int reasonId) {
        ByteBuffer b = ByteBuffer.allocate(FeConstants.LEN_ROLLBACK).order(ByteOrder.LITTLE_ENDIAN);
        b.putInt(FeConstants.LEN_ROLLBACK);
        b.putInt(FeConstants.ROLLBACK);
        b.putLong(requestId);
        b.putLong(sessionId);
        b.put(pad(etag != null ? etag : "", 24));
        b.putInt(station);
        b.putInt(lane);
        b.putLong(ticketId);
        b.putInt(status);
        b.put(pad(plate != null ? plate : "", 10));
        b.putInt(imageCount);
        b.putInt(weight);
        b.putInt(reasonId);
        return b.array();
    }

    /** LOOKUP_VEHICLE (1AZ) 2.3.1.7.15: 110 bytes. No station/lane; after etag: station_type 1, lane_type 1, general1 8, general2 16 */
    public static byte[] buildLookupVehicle(long requestId, long sessionId, long timestamp,
                                             String tid, String etag,
                                             String stationType, String laneType, byte[] general1, byte[] general2) {
        ByteBuffer b = ByteBuffer.allocate(FeConstants.LEN_LOOKUP_VEHICLE).order(ByteOrder.LITTLE_ENDIAN);
        b.putInt(FeConstants.LEN_LOOKUP_VEHICLE);
        b.putInt(FeConstants.LOOKUP_VEHICLE);
        b.putInt(FeConstants.VERSION_ID_DEFAULT);
        b.putLong(requestId);
        b.putLong(sessionId);
        b.putLong(timestamp);
        b.put(pad(tid != null ? tid : "", 24));
        b.put(pad(etag != null ? etag : "", 24));
        b.put(pad(stationType != null && !stationType.isEmpty() ? stationType.substring(0, 1) : "C", 1));
        b.put(pad(laneType != null && !laneType.isEmpty() ? laneType.substring(0, 1) : "I", 1));
        if (general1 != null && general1.length >= 8) b.put(Arrays.copyOf(general1, 8));
        else b.put(new byte[8]);
        if (general2 != null && general2.length >= 16) b.put(Arrays.copyOf(general2, 16));
        else b.put(new byte[16]);
        return b.array();
    }

    /** QUERY_VEHICLE_BOO: 122 bytes. Offsets: min_balance 94..98, general1 98..106, general2 106..122 */
    public static byte[] buildQueryVehicleBoo(long requestId, long sessionId, long timestamp,
                                               String tid, String etag, int station, int lane,
                                               String stationType, String laneType, int minBalance, byte[] general1, byte[] general2) {
        ByteBuffer b = ByteBuffer.allocate(FeConstants.LEN_QUERY_VEHICLE_BOO).order(ByteOrder.LITTLE_ENDIAN);
        b.putInt(FeConstants.LEN_QUERY_VEHICLE_BOO);
        b.putInt(FeConstants.QUERY_VEHICLE_BOO);
        b.putInt(FeConstants.VERSION_ID_DEFAULT);
        b.putLong(requestId);
        b.putLong(sessionId);
        b.putLong(timestamp);
        b.put(pad(tid != null ? tid : "", 24));
        b.put(pad(etag != null ? etag : "", 24));
        b.putInt(station);
        b.putInt(lane);
        b.put(pad(stationType != null && !stationType.isEmpty() ? stationType.substring(0, 1) : "C", 1));
        b.put(pad(laneType != null && !laneType.isEmpty() ? laneType.substring(0, 1) : "I", 1));
        b.putInt(minBalance);
        if (general1 != null && general1.length >= 8) b.put(Arrays.copyOf(general1, 8));
        else b.put(new byte[8]);
        if (general2 != null && general2.length >= 16) b.put(Arrays.copyOf(general2, 16));
        else b.put(new byte[16]);
        return b.array();
    }

    /** CHECKOUT_RESERVE_BOO: minimum 203 bytes (rating_detail_line=0, no rating_detail items). */
    public static byte[] buildCheckoutReserveBoo(long requestId, long sessionId, long timestamp,
                                                  String tid, String etag, long ticketInId, long hubId, long ticketETagId, long ticketOutId,
                                                  long checkinDatetime, long checkinCommitDatetime,
                                                  int stationIn, int laneIn, int stationOut, int laneOut,
                                                  String plate, String ticketType, int priceTicketType, int transAmount, long transDatetime,
                                                  int ratingDetailLine, byte[] general1, byte[] general2) {
        ByteBuffer b = ByteBuffer.allocate(FeConstants.LEN_CHECKOUT_RESERVE_BOO_MIN).order(ByteOrder.LITTLE_ENDIAN);
        b.putInt(FeConstants.LEN_CHECKOUT_RESERVE_BOO_MIN);
        b.putInt(FeConstants.CHECKOUT_RESERVE_BOO);
        b.putInt(FeConstants.VERSION_ID_DEFAULT);
        b.putLong(requestId);
        b.putLong(sessionId);
        b.putLong(timestamp);
        b.put(pad(tid != null ? tid : "", 24));
        b.put(pad(etag != null ? etag : "", 24));
        b.putLong(ticketInId);
        b.putLong(hubId != 0 ? hubId : 0);
        b.putLong(ticketETagId);
        b.putLong(ticketOutId);
        b.putLong(checkinDatetime);
        b.putLong(checkinCommitDatetime);
        b.putInt(stationIn);
        b.putInt(laneIn);
        b.putInt(stationOut);
        b.putInt(laneOut);
        b.put(pad(plate != null ? plate : "", 10));
        b.put(pad(ticketType != null && !ticketType.isEmpty() ? ticketType.substring(0, 1) : "L", 1));
        b.putInt(priceTicketType);
        b.putInt(transAmount);
        b.putLong(transDatetime);
        b.putInt(ratingDetailLine >= 0 ? ratingDetailLine : 0);
        if (general1 != null && general1.length >= 8) b.put(Arrays.copyOf(general1, 8));
        else b.put(new byte[8]);
        if (general2 != null && general2.length >= 16) b.put(Arrays.copyOf(general2, 16));
        else b.put(new byte[16]);
        return b.array();
    }

    /** CHECKOUT_COMMIT_BOO: 188 bytes. Plate 20 bytes. */
    public static byte[] buildCheckoutCommitBoo(long requestId, long sessionId, long timestamp,
                                                String tid, String etag, long ticketInId, long hubId, long ticketOutId, long ticketETagId,
                                                int stationIn, int laneIn, int stationOut, int laneOut,
                                                String plate, int transAmount, long transDatetime, byte[] general1, byte[] general2) {
        ByteBuffer b = ByteBuffer.allocate(FeConstants.LEN_CHECKOUT_COMMIT_BOO).order(ByteOrder.LITTLE_ENDIAN);
        b.putInt(FeConstants.LEN_CHECKOUT_COMMIT_BOO);
        b.putInt(FeConstants.CHECKOUT_COMMIT_BOO);
        b.putInt(FeConstants.VERSION_ID_DEFAULT);
        b.putLong(requestId);
        b.putLong(sessionId);
        b.putLong(timestamp);
        b.put(pad(tid != null ? tid : "", 24));
        b.put(pad(etag != null ? etag : "", 24));
        b.putLong(ticketInId);
        b.putLong(hubId);
        b.putLong(ticketOutId);
        b.putLong(ticketETagId);
        b.putInt(stationIn);
        b.putInt(laneIn);
        b.putInt(stationOut);
        b.putInt(laneOut);
        b.put(pad(plate != null ? plate : "", 20));
        b.putInt(transAmount);
        b.putLong(transDatetime);
        if (general1 != null && general1.length >= 8) b.put(Arrays.copyOf(general1, 8));
        else b.put(new byte[8]);
        if (general2 != null && general2.length >= 16) b.put(Arrays.copyOf(general2, 16));
        else b.put(new byte[16]);
        return b.array();
    }

    /** CHECKOUT_ROLLBACK_BOO: 178 bytes. Plate 10 bytes. */
    public static byte[] buildCheckoutRollbackBoo(long requestId, long sessionId, long timestamp,
                                                   String tid, String etag, long ticketInId, long hubId, long ticketOutId, long ticketETagId,
                                                   int stationIn, int laneIn, int stationOut, int laneOut,
                                                   String plate, int transAmount, long transDatetime, byte[] general1, byte[] general2) {
        ByteBuffer b = ByteBuffer.allocate(FeConstants.LEN_CHECKOUT_ROLLBACK_BOO).order(ByteOrder.LITTLE_ENDIAN);
        b.putInt(FeConstants.LEN_CHECKOUT_ROLLBACK_BOO);
        b.putInt(FeConstants.CHECKOUT_ROLLBACK_BOO);
        b.putInt(FeConstants.VERSION_ID_DEFAULT);
        b.putLong(requestId);
        b.putLong(sessionId);
        b.putLong(timestamp);
        b.put(pad(tid != null ? tid : "", 24));
        b.put(pad(etag != null ? etag : "", 24));
        b.putLong(ticketInId);
        b.putLong(hubId);
        b.putLong(ticketOutId);
        b.putLong(ticketETagId);
        b.putInt(stationIn);
        b.putInt(laneIn);
        b.putInt(stationOut);
        b.putInt(laneOut);
        b.put(pad(plate != null ? plate : "", 10));
        b.putInt(transAmount);
        b.putLong(transDatetime);
        if (general1 != null && general1.length >= 8) b.put(Arrays.copyOf(general1, 8));
        else b.put(new byte[8]);
        if (general2 != null && general2.length >= 16) b.put(Arrays.copyOf(general2, 16));
        else b.put(new byte[16]);
        return b.array();
    }

    private FeMessageBuilder() {}
}
