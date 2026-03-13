package fetest;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.Socket;
import java.util.concurrent.atomic.AtomicLong;

/**
 * TCP client: send encrypted FE frame (4-byte LE length + encrypted payload), receive same format and decrypt.
 */
public class FeClient implements AutoCloseable {

    private Socket socket;
    private InputStream in;
    private OutputStream out;
    private final String encryptionKey;
    private final AtomicLong requestIdGen = new AtomicLong(System.currentTimeMillis());

    public FeClient(String host, int port, String encryptionKey) throws IOException {
        this.socket = new Socket(host, port);
        this.socket.setSoTimeout(30_000);
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
     * Send plain payload: encrypt and wrap with 4-byte length, then write to socket.
     */
    public void send(byte[] plainPayload) throws Exception {
        byte[] encrypted = AesCrypto.encrypt(plainPayload, encryptionKey);
        byte[] frame = AesCrypto.wrapEncrypted(encrypted);
        out.write(frame);
        out.flush();
    }

    /**
     * Read one FE frame: 4 bytes LE length, then (length-4) bytes payload; decrypt and return.
     */
    public byte[] receive() throws IOException {
        byte[] lenBuf = new byte[4];
        int n = readFully(in, lenBuf);
        if (n != 4) throw new IOException("Short read length: " + n);
        int totalLen = (lenBuf[0] & 0xFF) | ((lenBuf[1] & 0xFF) << 8) | ((lenBuf[2] & 0xFF) << 16) | ((lenBuf[3] & 0xFF) << 24);
        if (totalLen < 4 || totalLen > 1024 * 1024) throw new IOException("Invalid frame length: " + totalLen);
        int payloadLen = totalLen - 4;
        byte[] payload = new byte[payloadLen];
        n = readFully(in, payload);
        if (n != payloadLen) throw new IOException("Short read payload: got " + n + " expected " + payloadLen);
        try {
            return AesCrypto.decrypt(payload, encryptionKey);
        } catch (Exception e) {
            throw new IOException("Decrypt failed: " + e.getMessage(), e);
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
