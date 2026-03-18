package fetest;

import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.List;

/** Load/save etag list to user home .fe-test-client/etags.txt (one etag per line). */
public final class EtagListManager {
    private static final String DIR = ".fe-test-client";
    private static final String FILE = "etags.txt";

    public static List<String> load() {
        Path path = configPath();
        if (path == null || !Files.isRegularFile(path)) return new ArrayList<>();
        List<String> list = new ArrayList<>();
        try {
            for (String line : Files.readAllLines(path, StandardCharsets.UTF_8)) {
                String s = line.trim();
                if (!s.isEmpty()) list.add(s);
            }
        } catch (IOException e) {
            return list;
        }
        return list;
    }

    public static void save(List<String> list) {
        Path path = configPath();
        if (path == null) return;
        try {
            Files.createDirectories(path.getParent());
            Files.write(path, list, StandardCharsets.UTF_8);
        } catch (IOException ignored) { }
    }

    private static Path configPath() {
        String home = System.getProperty("user.home");
        if (home == null) return null;
        return Paths.get(home, DIR, FILE);
    }

    private EtagListManager() {}
}
