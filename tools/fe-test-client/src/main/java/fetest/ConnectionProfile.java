package fetest;

/** One saved connection: name, host, port, user, password, encryption key, station, lane. */
public class ConnectionProfile {
    private String name;
    private String host;
    private int port;
    private String user;
    private String password;
    private String encryptionKey;
    private String station;
    private String lane;

    public ConnectionProfile() {
        this("", "127.0.0.1", 7000, "", "", "", "0", "0");
    }

    public ConnectionProfile(String name, String host, int port, String user, String password, String encryptionKey, String station, String lane) {
        this.name = name != null ? name : "";
        this.host = host != null ? host : "127.0.0.1";
        this.port = port;
        this.user = user != null ? user : "";
        this.password = password != null ? password : "";
        this.encryptionKey = encryptionKey != null ? encryptionKey : "";
        this.station = station != null ? station : "0";
        this.lane = lane != null ? lane : "0";
    }

    public String getName() { return name; }
    public void setName(String name) { this.name = name != null ? name : ""; }
    public String getHost() { return host; }
    public void setHost(String host) { this.host = host != null ? host : ""; }
    public int getPort() { return port; }
    public void setPort(int port) { this.port = port; }
    public String getUser() { return user; }
    public void setUser(String user) { this.user = user != null ? user : ""; }
    public String getPassword() { return password; }
    public void setPassword(String password) { this.password = password != null ? password : ""; }
    public String getEncryptionKey() { return encryptionKey; }
    public void setEncryptionKey(String encryptionKey) { this.encryptionKey = encryptionKey != null ? encryptionKey : ""; }
    public String getStation() { return station; }
    public void setStation(String station) { this.station = station != null ? station : "0"; }
    public String getLane() { return lane; }
    public void setLane(String lane) { this.lane = lane != null ? lane : "0"; }

    @Override
    public String toString() { return name.isEmpty() ? (host + ":" + port) : name; }
}
