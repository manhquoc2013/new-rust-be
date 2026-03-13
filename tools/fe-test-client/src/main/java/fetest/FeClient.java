package fetest;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.concurrent.atomic.AtomicLong;

/**
 * TCP client: send encrypted FE frame (4-byte LE length + encrypted payload), receive same format and decrypt.
 * When server sends plain error (8 bytes: length=8 + error_code) before closing, receive() throws IOException with message from {@link #plainErrorMessage(int)}.
 */
public class FeClient implements AutoCloseable {

    /** Plain error codes from server (no encryption key, IP denied, etc.). */
    public static final int PLAIN_ERR_NO_ENCRYPTION_KEY = 301;
    public static final int PLAIN_ERR_EMPTY_KEY = 302;
    public static final int PLAIN_ERR_IP_DENIED = 307;
    public static final int PLAIN_ERR_IP_BLOCKED = 308;

    private Socket socket;
    private InputStream in;
    private OutputStream out;
    private final String encryptionKey;
    private final AtomicLong requestIdGen = new AtomicLong(System.currentTimeMillis());

    /** Timeout kết nối và đọc (ms). */
    public static final int CONNECT_AND_READ_TIMEOUT_MS = 5_000;

    public FeClient(String host, int port, String encryptionKey) throws IOException {
        this.socket = new Socket();
        this.socket.connect(new InetSocketAddress(host, port), CONNECT_AND_READ_TIMEOUT_MS);
        this.socket.setSoTimeout(CONNECT_AND_READ_TIMEOUT_MS);
        this.in = socket.getInputStream();
        this.out = socket.getOutputStream();
        this.encryptionKey = encryptionKey != null ? encryptionKey : "";
    }

    public boolean isConnected() {
        return socket != null && socket.isConnected() && !socket.isClosed();
    }

    public long nextRequestId() {
        return requestIdGen.incrementAndGet();
    }

    /**
     * Human-readable message for server plain error code (sent before encryption).
     */
    public static String plainErrorMessage(int code) {
        switch (code) {
            case PLAIN_ERR_NO_ENCRYPTION_KEY:
                return "Server: Không tìm thấy encryption key cho IP (301)";
            case PLAIN_ERR_EMPTY_KEY:
                return "Server: Encryption key rỗng cho IP (302)";
            case PLAIN_ERR_IP_DENIED:
                return "Server: IP bị từ chối - denylist (307)";
            case PLAIN_ERR_IP_BLOCKED:
                return "Server: IP bị khóa - quá nhiều lần lỗi (308)";
            default:
                return "Server trả lỗi (code " + code + ") trước khi thiết lập mã hóa";
        }
    }

    /**
     * Send plain payload: encrypt and wrap with 4-byte length, then write to socket.
     */
    public void send(byte[] plainPayload) throws Exception {
        byte[] encrypted = AesCrypto.encrypt(plainPayload, encryptionKey);
        byte[] frame = AesCrypto.wrapEncrypted(encrypted);
        out.write(frame);
        out.flush();
    }

    /**
     * Read one FE frame: 4 bytes LE length, then (length-4) bytes payload.
     * If length == 8: server sent plain error (no encryption); throws IOException with {@link #plainErrorMessage(int)}.
     * Otherwise decrypt and return.
     */
    public byte[] receive() throws IOException {
        byte[] lenBuf = new byte[4];
        int n = readFully(in, lenBuf);
        if (n == 0) {
            throw new IOException("Server đóng kết nối (không có dữ liệu). Kiểm tra IP có trong cấu hình server / encryption key / IP bị chặn.");
        }
        if (n != 4) {
            throw new IOException("Đọc length không đủ: " + n + " byte. Server có thể đã đóng kết nối.");
        }
        int totalLen = (lenBuf[0] & 0xFF) | ((lenBuf[1] & 0xFF) << 8) | ((lenBuf[2] & 0xFF) << 16) | ((lenBuf[3] & 0xFF) << 24);
        if (totalLen == 8) {
            byte[] payload = new byte[4];
            n = readFully(in, payload);
            if (n != 4) {
                throw new IOException("Server gửi bản tin lỗi nhưng đóng sớm (đọc " + n + "/4 byte).");
            }
            int code = ByteBuffer.wrap(payload).order(ByteOrder.LITTLE_ENDIAN).getInt();
            throw new IOException(plainErrorMessage(code));
        }
        if (totalLen < 4 || totalLen > 1024 * 1024) {
            throw new IOException("Độ dài frame không hợp lệ: " + totalLen);
        }
        int payloadLen = totalLen - 4;
        byte[] payload = new byte[payloadLen];
        n = readFully(in, payload);
        if (n != payloadLen) {
            throw new IOException("Đọc payload không đủ: " + n + "/" + payloadLen + ". Server có thể đã đóng kết nối.");
        }
        try {
            byte[] decrypted = AesCrypto.decrypt(payload, encryptionKey);
            if (decrypted == null || decrypted.length == 0) {
                throw new IOException("Server trả bản tin rỗng. Đã ngắt kết nối.");
            }
            return decrypted;
        } catch (IOException e) {
            throw e;
        } catch (Exception e) {
            throw new IOException("Giải mã thất bại: " + e.getMessage(), e);
        }
    }

    private static int readFully(InputStream in, byte[] b) throws IOException {
        int off = 0;
        while (off < b.length) {
            int r = in.read(b, off, b.length - off);
            if (r <= 0) return off;
            off += r;
        }
        return off;
    }

    @Override
    public void close() {
        try {
            if (socket != null) socket.close();
        } catch (IOException ignored) { }
        socket = null;
        in = null;
        out = null;
    }
}
