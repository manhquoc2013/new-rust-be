package fetest;

/**
 * FE protocol command IDs and message lengths (aligned with Rust backend fe_protocol / constants::fe).
 */
public final class FeConstants {

    // Command IDs (request)
    public static final int CONNECT = 0x6C;
    public static final int CONNECT_RESP = 0x6D;
    public static final int HANDSHAKE = 0x1C;
    public static final int SHAKE_RESP = 0x6F;
    public static final int CHECKIN = 0x66;
    public static final int CHECKIN_RESP = 0x67;
    public static final int COMMIT = 0x68;
    public static final int COMMIT_RESP = 0x69;
    public static final int ROLLBACK = 0x6A;
    public static final int ROLLBACK_RESP = 0x6B;
    public static final int TERMINATE = 0x70;
    public static final int TERMINATE_RESP = 0x71;
    public static final int QUERY_VEHICLE_BOO = 0x64;
    public static final int QUERY_VEHICLE_BOO_RESP = 0x65;
    public static final int LOOKUP_VEHICLE = 0x96;
    public static final int LOOKUP_VEHICLE_RESP = 0x97;
    public static final int CHECKOUT_RESERVE_BOO = 0x98;
    public static final int CHECKOUT_RESERVE_BOO_RESP = 0x99;
    public static final int CHECKOUT_COMMIT_BOO = 0x9A;
    public static final int CHECKOUT_COMMIT_BOO_RESP = 0x9B;
    public static final int CHECKOUT_ROLLBACK_BOO = 0x9C;
    public static final int CHECKOUT_ROLLBACK_BOO_RESP = 0x9D;

    // Message lengths (decrypted payload)
    public static final int LEN_CONNECT = 44;
    public static final int LEN_HANDSHAKE = 28;
    public static final int LEN_TERMINATE = 28;
    public static final int LEN_CHECKIN = 106;
    public static final int LEN_COMMIT = 98;
    public static final int LEN_ROLLBACK = 90;
    public static final int LEN_QUERY_VEHICLE_BOO = 122;
    public static final int LEN_LOOKUP_VEHICLE = 110;
    public static final int LEN_CHECKOUT_COMMIT_BOO = 188;
    public static final int LEN_CHECKOUT_ROLLBACK_BOO = 178;

    public static final int CONNECT_RESP_LEN = 32;
    public static final int SHAKE_RESP_LEN = 32;
    public static final int CHECKIN_RESP_LEN = 98;
    public static final int COMMIT_RESP_LEN = 28;
    public static final int ROLLBACK_RESP_LEN = 28;
    public static final int TERMINATE_RESP_LEN = 32;

    public static final int VERSION_ID_DEFAULT = 1;

    private FeConstants() {}
}
