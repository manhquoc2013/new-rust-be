package fetest;

import javax.crypto.Cipher;
import javax.crypto.spec.IvParameterSpec;
import javax.crypto.spec.SecretKeySpec;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;

/**
 * AES-128-CBC with PKCS7 padding, IV = 0 (aligned with Rust backend crypto/aes).
 * Key must be 16 bytes (truncate or pad if needed).
 */
public final class AesCrypto {

    private static final String TRANSFORM = "AES/CBC/PKCS5Padding";
    private static final int IV_LEN = 16;
    private static final int KEY_LEN = 16;

    public static byte[] encrypt(byte[] plain, String keyStr) throws Exception {
        byte[] key = keyBytes(keyStr);
        Cipher cipher = Cipher.getInstance(TRANSFORM);
        cipher.init(Cipher.ENCRYPT_MODE, new SecretKeySpec(key, "AES"), new IvParameterSpec(new byte[IV_LEN]));
        return cipher.doFinal(plain);
    }

    public static byte[] decrypt(byte[] encrypted, String keyStr) throws Exception {
        byte[] key = keyBytes(keyStr);
        Cipher cipher = Cipher.getInstance(TRANSFORM);
        cipher.init(Cipher.DECRYPT_MODE, new SecretKeySpec(key, "AES"), new IvParameterSpec(new byte[IV_LEN]));
        return cipher.doFinal(encrypted);
    }

    /**
     * Wrap payload for FE: [4 bytes LE length (total = 4 + encrypted.length)][encrypted].
     */
    public static byte[] wrapEncrypted(byte[] encrypted) {
        int total = 4 + encrypted.length;
        byte[] out = new byte[total];
        out[0] = (byte) (total);
        out[1] = (byte) (total >> 8);
        out[2] = (byte) (total >> 16);
        out[3] = (byte) (total >> 24);
        System.arraycopy(encrypted, 0, out, 4, encrypted.length);
        return out;
    }

    private static byte[] keyBytes(String keyStr) {
        byte[] raw = keyStr.getBytes(StandardCharsets.UTF_8);
        if (raw.length == KEY_LEN) return raw;
        if (raw.length > KEY_LEN) return Arrays.copyOf(raw, KEY_LEN);
        return Arrays.copyOf(raw, KEY_LEN);
    }

    private AesCrypto() {}
}
