package fetest;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;

/**
 * Parse decrypted FE response payloads and extract session_id, ticket_id, etag, etc. for refill.
 */
public final class FeResponseParser {

    public static class ParsedResponse {
        public int commandId;
        public long requestId;
        public long sessionId;
        public int status;
        public long ticketId;
        public String etag;
        public int station;
        public int lane;
        public String plate;
        public String rawHex;
        public String summary;

        @Override
        public String toString() {
            return summary != null ? summary : ("cmd=0x" + Integer.toHexString(commandId) + " status=" + status + " sessionId=" + sessionId + " ticketId=" + ticketId);
        }
    }

    public static ParsedResponse parse(byte[] decrypted) {
        ParsedResponse r = new ParsedResponse();
        r.rawHex = bytesToHex(decrypted);
        if (decrypted.length < 8) {
            r.summary = "Too short: " + decrypted.length + " bytes";
            return r;
        }
        ByteBuffer b = ByteBuffer.wrap(decrypted).order(ByteOrder.LITTLE_ENDIAN);
        int msgLen = b.getInt(0);
        r.commandId = b.getInt(4);
        if (decrypted.length >= 20) {
            r.requestId = b.getLong(8);
            r.sessionId = b.getLong(16);
        }
        if (decrypted.length >= 24) r.status = b.getInt(20);

        switch (r.commandId) {
            case FeConstants.CONNECT_RESP:
                r.summary = String.format("CONNECT_RESP status=%d sessionId=%d", r.status, r.sessionId);
                break;
            case FeConstants.SHAKE_RESP:
                r.summary = String.format("SHAKE_RESP status=%d sessionId=%d", r.status, r.sessionId);
                break;
            case FeConstants.CHECKIN_RESP:
                // Layout: msg_len 4, cmd 4, req_id 8, sess_id 8, status 4, etag 24, station 4, lane 4, ticket_id 8, ...
                if (decrypted.length >= 68) {
                    r.etag = trimNull(new String(decrypted, 28, Math.min(24, decrypted.length - 28), StandardCharsets.UTF_8));
                    r.station = b.getInt(52);
                    r.lane = b.getInt(56);
                    r.ticketId = b.getLong(60);
                    r.plate = decrypted.length >= 90 ? trimNull(new String(decrypted, 80, 10, StandardCharsets.UTF_8)) : "";
                }
                r.summary = String.format("CHECKIN_RESP status=%d sessionId=%d ticketId=%d etag=%s station=%d lane=%d",
                        r.status, r.sessionId, r.ticketId, r.etag != null ? r.etag.trim() : "-", r.station, r.lane);
                break;
            case FeConstants.COMMIT_RESP:
            case FeConstants.ROLLBACK_RESP:
                r.summary = String.format("%s status=%d", r.commandId == FeConstants.COMMIT_RESP ? "COMMIT_RESP" : "ROLLBACK_RESP", r.status);
                break;
            case FeConstants.TERMINATE_RESP:
                r.summary = String.format("TERMINATE_RESP status=%d", r.status);
                break;
            case FeConstants.QUERY_VEHICLE_BOO_RESP:
                r.summary = String.format("QUERY_VEHICLE_BOO_RESP status=%d", r.status);
                break;
            case FeConstants.LOOKUP_VEHICLE_RESP:
                if (decrypted.length >= 32) r.etag = trimNull(new String(decrypted, 32, Math.min(24, decrypted.length - 32), StandardCharsets.UTF_8));
                r.summary = String.format("LOOKUP_VEHICLE_RESP status=%d etag=%s", r.status, r.etag != null ? r.etag.trim() : "-");
                break;
            default:
                r.summary = String.format("cmd=0x%02X status=%d sessionId=%d", r.commandId, r.status, r.sessionId);
                break;
        }
        return r;
    }

    private static String trimNull(String s) {
        if (s == null) return "";
        int i = s.indexOf('\0');
        return i >= 0 ? s.substring(0, i).trim() : s.trim();
    }

    private static String bytesToHex(byte[] bytes) {
        StringBuilder sb = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) sb.append(String.format("%02X", b & 0xFF));
        return sb.toString();
    }

    private FeResponseParser() {}
}
