package fetest;

import java.io.*;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.List;
import java.util.Properties;

/** Load/save connection profiles to user home .fe-test-client/connections.properties */
public final class ConnectionManager {
    private static final String DIR = ".fe-test-client";
    private static final String FILE = "connections.properties";
    private static final String PREFIX = "connection.";

    public static List<ConnectionProfile> load() {
        List<ConnectionProfile> list = new ArrayList<>();
        Path path = configPath();
        if (path == null || !Files.isRegularFile(path)) return list;
        Properties p = new Properties();
        try (InputStream in = Files.newInputStream(path)) {
            p.load(new InputStreamReader(in, java.nio.charset.StandardCharsets.UTF_8));
        } catch (IOException e) {
            return list;
        }
        int count = Integer.parseInt(p.getProperty("count", "0"));
        for (int i = 0; i < count; i++) {
            String name = p.getProperty(PREFIX + i + ".name", "");
            String host = p.getProperty(PREFIX + i + ".host", "127.0.0.1");
            int port = Integer.parseInt(p.getProperty(PREFIX + i + ".port", "7000"));
            String user = p.getProperty(PREFIX + i + ".user", "");
            String password = p.getProperty(PREFIX + i + ".password", "");
            String key = p.getProperty(PREFIX + i + ".encryptionKey", "");
            String station = p.getProperty(PREFIX + i + ".station", "0");
            String lane = p.getProperty(PREFIX + i + ".lane", "0");
            list.add(new ConnectionProfile(name, host, port, user, password, key, station, lane));
        }
        return list;
    }

    public static void save(List<ConnectionProfile> list) {
        Path path = configPath();
        if (path == null) return;
        try {
            Files.createDirectories(path.getParent());
        } catch (IOException e) {
            return;
        }
        Properties p = new Properties();
        p.setProperty("count", String.valueOf(list.size()));
        for (int i = 0; i < list.size(); i++) {
            ConnectionProfile c = list.get(i);
            p.setProperty(PREFIX + i + ".name", c.getName());
            p.setProperty(PREFIX + i + ".host", c.getHost());
            p.setProperty(PREFIX + i + ".port", String.valueOf(c.getPort()));
            p.setProperty(PREFIX + i + ".user", c.getUser());
            p.setProperty(PREFIX + i + ".password", c.getPassword());
            p.setProperty(PREFIX + i + ".encryptionKey", c.getEncryptionKey());
            p.setProperty(PREFIX + i + ".station", c.getStation());
            p.setProperty(PREFIX + i + ".lane", c.getLane());
        }
        try (OutputStream out = Files.newOutputStream(path);
             Writer w = new OutputStreamWriter(out, java.nio.charset.StandardCharsets.UTF_8)) {
            p.store(w, "FE Test Client connections");
        } catch (IOException ignored) { }
    }

    private static Path configPath() {
        String home = System.getProperty("user.home");
        if (home == null) return null;
        return Paths.get(home, DIR, FILE);
    }
}
