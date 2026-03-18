package fetest;

import javax.swing.*;
import javax.swing.border.TitledBorder;
import java.awt.*;
import java.awt.event.WindowAdapter;
import java.awt.event.WindowEvent;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.ScheduledFuture;
import java.util.concurrent.TimeUnit;

/**
 * FE Test Client UI: connection, etags, commands (no CONNECT/SHAKE/TERMINATE), dynamic form per command,
 * multi-etag selection with one request per etag.
 */
public class FeTestClientApp extends JFrame {

    private static final int CMD_CHECKIN = 0;
    private static final int CMD_COMMIT = 1;
    private static final int CMD_ROLLBACK = 2;
    private static final int CMD_LOOKUP_VEHICLE = 3;
    private static final int CMD_QUERY_VEHICLE_BOO = 4;
    private static final int CMD_CHECKOUT_RESERVE = 5;
    private static final int CMD_CHECKOUT_COMMIT = 6;
    private static final int CMD_CHECKOUT_ROLLBACK = 7;

    private final JComboBox<ConnectionProfile> connectionCombo = new JComboBox<>();
    private final DefaultComboBoxModel<ConnectionProfile> connectionModel = new DefaultComboBoxModel<>();
    private final JTextField hostField = new JTextField("127.0.0.1", 12);
    private final JTextField portField = new JTextField("7000", 6);
    private final JTextField userField = new JTextField("", 10);
    private final JPasswordField passField = new JPasswordField(10);
    private final JTextField encryptionKeyField = new JTextField(16);
    private final JTextField stationField = new JTextField("0", 6);
    private final JTextField laneField = new JTextField("0", 4);

    /** Timeout (giây) cho bản tin CONNECT, mặc định 10s. */
    private static final int DEFAULT_CONNECT_TIMEOUT_SEC = 10;

    private final DefaultListModel<String> etagListModel = new DefaultListModel<>();
    private final JList<String> etagList = new JList<>(etagListModel);

    private final JComboBox<String> commandCombo = new JComboBox<>(new String[]{
            "CHECKIN (0A)", "COMMIT (3A)", "ROLLBACK (3A)", "LOOKUP_VEHICLE (1AZ)", "QUERY_VEHICLE_BOO (1A)",
            "CHECKOUT_RESERVE (2AZ)", "CHECKOUT_COMMIT (3AZ)", "CHECKOUT_ROLLBACK (3AZ)"
    });

    private final JTextField sessionIdField = new JTextField("0", 14);
    private final JTextField ticketIdField = new JTextField("0", 14);
    private final JTextField etagField = new JTextField("", 22);
    private final JTextField plateField = new JTextField("", 12);
    private final JTextField tidField = new JTextField("", 20);
    private final JTextField hashField = new JTextField("", 18);
    private final JTextField statusField = new JTextField("0", 4);
    private final JTextField minBalanceField = new JTextField("0", 6);
    private final JTextField stationInField = new JTextField("0", 6);
    private final JTextField laneInField = new JTextField("0", 4);
    private final JTextField stationOutField = new JTextField("0", 6);
    private final JTextField laneOutField = new JTextField("0", 4);
    private final JTextField ticketInIdField = new JTextField("0", 14);
    private final JTextField ticketOutIdField = new JTextField("0", 14);
    private final JTextField ticketETagIdField = new JTextField("0", 14);
    private final JTextField hubIdField = new JTextField("0", 14);
    private final JTextField checkinDatetimeField = new JTextField("0", 16);
    private final JTextField checkinCommitDatetimeField = new JTextField("0", 16);
    private final JTextField transAmountField = new JTextField("0", 8);
    private final JTextField transDatetimeField = new JTextField("0", 16);
    private final JTextField ticketTypeField = new JTextField("L", 2);
    private final JTextField priceTicketTypeField = new JTextField("0", 4);
    private final JTextField ratingDetailLineField = new JTextField("0", 2);

    private final JPanel dynamicFormPanel = new JPanel();
    private final JButton refillButton = new JButton("Refill từ response trước");
    private final JButton sendButton = new JButton("Gửi (1 request / eTag đã chọn)");
    private final JButton connectButton = new JButton("Kết nối");
    private final JButton disconnectButton = new JButton("Ngắt");
    private final JButton testTcpButton = new JButton("Test TCP");

    private final JTextArea statusResponseArea = new JTextArea(12, 60);
    private final JLabel sendStatusLabel = new JLabel(" ");

    private FeClient client;
    private FeResponseParser.ParsedResponse lastResponse;
    private volatile long lastSessionId;
    private ScheduledExecutorService handshakeExecutor;
    private ScheduledFuture<?> handshakeTask;
    private String savedHost;
    private int savedPort;
    private String savedKey;
    private String savedUser;
    private String savedPass;
    private int savedTimeout = DEFAULT_CONNECT_TIMEOUT_SEC;
    private volatile boolean reconnectInProgress;
    private volatile boolean userRequestedDisconnect;

    public FeTestClientApp() {
        setTitle("FE Test Client - TCOC Protocol");
        setDefaultCloseOperation(JFrame.DO_NOTHING_ON_CLOSE);
        addWindowListener(new WindowAdapter() {
            @Override
            public void windowClosing(WindowEvent e) {
                cancelHandshake();
                if (handshakeExecutor != null) handshakeExecutor.shutdown();
                disconnect(false);
                dispose();
            }
        });
        handshakeExecutor = Executors.newSingleThreadScheduledExecutor();
        loadConnections();
        loadEtags();
        connectionCombo.setModel(connectionModel);
        connectionCombo.setRenderer((list, value, index, isSelected, cellHasFocus) -> {
            JLabel l = new JLabel(value != null ? value.toString() : "");
            if (isSelected) { l.setBackground(list.getSelectionBackground()); l.setForeground(list.getSelectionForeground()); l.setOpaque(true); }
            return l;
        });
        connectionCombo.addActionListener(e -> onConnectionSelected());
        if (connectionModel.getSize() > 0) {
            connectionCombo.setSelectedIndex(0);
            onConnectionSelected();
        }
        sessionIdField.setEditable(false);
        sessionIdField.setToolTipText("Từ CONNECT_RESP khi kết nối thành công, dùng xuyên suốt.");

        JPanel main = new JPanel(new BorderLayout(8, 8));
        main.setBorder(BorderFactory.createEmptyBorder(8, 8, 8, 8));

        JPanel top = new JPanel(new BorderLayout(4, 4));
        top.add(buildConnectionPanel(), BorderLayout.NORTH);
        top.add(buildCenterPanel(), BorderLayout.CENTER);
        main.add(top, BorderLayout.CENTER);
        main.add(buildBottomPanel(), BorderLayout.SOUTH);

        setContentPane(main);
        pack();
        setMinimumSize(new Dimension(780, 620));
        setLocationRelativeTo(null);
    }

    private JPanel buildConnectionPanel() {
        JPanel p = new JPanel(new BorderLayout(4, 4));
        p.setBorder(BorderFactory.createTitledBorder(BorderFactory.createEtchedBorder(), "Quản lý kết nối (nhiều profile, lưu lại)", TitledBorder.LEFT, TitledBorder.TOP));

        JPanel row1 = new JPanel(new FlowLayout(FlowLayout.LEFT, 6, 4));
        row1.add(new JLabel("Profile:"));
        row1.add(connectionCombo);
        JButton loadConnBtn = new JButton("Tải");
        JButton addConnBtn = new JButton("Thêm");
        JButton editConnBtn = new JButton("Sửa");
        JButton delConnBtn = new JButton("Xóa");
        loadConnBtn.addActionListener(e -> onConnectionSelected());
        addConnBtn.addActionListener(e -> addConnection());
        editConnBtn.addActionListener(e -> editConnection());
        delConnBtn.addActionListener(e -> deleteConnection());
        row1.add(loadConnBtn);
        row1.add(addConnBtn);
        row1.add(editConnBtn);
        row1.add(delConnBtn);
        row1.add(new JSeparator(SwingConstants.VERTICAL));
        connectButton.addActionListener(e -> doConnect());
        disconnectButton.addActionListener(e -> disconnect(true));
        disconnectButton.setEnabled(false);
        testTcpButton.addActionListener(e -> doTestTcp());
        testTcpButton.setToolTipText("Chỉ test kết nối TCP tới IP:Port (không gửi FE). Nếu OK nhưng Kết nối vẫn lỗi thì do giao thức/encryption.");
        row1.add(connectButton);
        row1.add(disconnectButton);
        p.add(row1, BorderLayout.NORTH);

        JPanel row2 = new JPanel(new FlowLayout(FlowLayout.LEFT, 6, 4));
        row2.add(new JLabel("IP:"));
        row2.add(hostField);
        row2.add(new JLabel("Port:"));
        row2.add(portField);
        row2.add(testTcpButton);
        row2.add(new JLabel("User:"));
        row2.add(userField);
        row2.add(new JLabel("Pass:"));
        row2.add(passField);
        row2.add(new JLabel("Encryption key (16):"));
        encryptionKeyField.setToolTipText("16 ký tự cho AES-128");
        row2.add(encryptionKeyField);
        row2.add(new JLabel("Station:"));
        row2.add(stationField);
        p.add(row2, BorderLayout.CENTER);

        return p;
    }

    private void loadConnections() {
        connectionModel.removeAllElements();
        for (ConnectionProfile c : ConnectionManager.load()) {
            connectionModel.addElement(c);
        }
    }

    private void onConnectionSelected() {
        ConnectionProfile sel = (ConnectionProfile) connectionCombo.getSelectedItem();
        if (sel == null) return;
        hostField.setText(sel.getHost());
        portField.setText(String.valueOf(sel.getPort()));
        userField.setText(sel.getUser());
        passField.setText(sel.getPassword());
        encryptionKeyField.setText(sel.getEncryptionKey());
        stationField.setText(sel.getStation());
        laneField.setText(sel.getLane());
    }

    private void addConnection() {
        ConnectionProfile c = new ConnectionProfile();
        c.setHost(hostField.getText().trim());
        try { c.setPort(Integer.parseInt(portField.getText().trim())); } catch (NumberFormatException ignored) { }
        c.setUser(userField.getText().trim());
        c.setPassword(new String(passField.getPassword()));
        c.setEncryptionKey(encryptionKeyField.getText());
        c.setStation(stationField.getText().trim());
        c.setLane("0");
        String name = JOptionPane.showInputDialog(this, "Tên profile:", "Kết nối " + c.getHost() + ":" + c.getPort());
        if (name != null) {
            c.setName(name.trim());
            connectionModel.addElement(c);
            ConnectionManager.save(connectionList());
            connectionCombo.setSelectedItem(c);
        }
    }

    private void editConnection() {
        ConnectionProfile sel = (ConnectionProfile) connectionCombo.getSelectedItem();
        if (sel == null) return;
        String name = JOptionPane.showInputDialog(this, "Tên profile:", sel.getName());
        if (name != null) sel.setName(name.trim());
        sel.setHost(hostField.getText().trim());
        try { sel.setPort(Integer.parseInt(portField.getText().trim())); } catch (NumberFormatException ignored) { }
        sel.setUser(userField.getText().trim());
        sel.setPassword(new String(passField.getPassword()));
        sel.setEncryptionKey(encryptionKeyField.getText());
        sel.setStation(stationField.getText().trim());
        ConnectionManager.save(connectionList());
    }

    private void deleteConnection() {
        ConnectionProfile sel = (ConnectionProfile) connectionCombo.getSelectedItem();
        if (sel == null) return;
        connectionModel.removeElement(sel);
        ConnectionManager.save(connectionList());
    }

    private List<ConnectionProfile> connectionList() {
        List<ConnectionProfile> list = new ArrayList<>();
        for (int i = 0; i < connectionModel.getSize(); i++) list.add(connectionModel.getElementAt(i));
        return list;
    }

    private JPanel buildCenterPanel() {
        JPanel center = new JPanel(new BorderLayout(6, 6));

        JPanel left = new JPanel(new BorderLayout(4, 4));
        left.setBorder(BorderFactory.createTitledBorder(BorderFactory.createEtchedBorder(), "Danh sách eTag (chọn nhiều → gửi 1 request/eTag)", TitledBorder.LEFT, TitledBorder.TOP));
        etagList.setSelectionMode(ListSelectionModel.MULTIPLE_INTERVAL_SELECTION);
        etagList.setVisibleRowCount(5);
        left.add(new JScrollPane(etagList), BorderLayout.CENTER);
        JPanel etagBtns = new JPanel(new FlowLayout(FlowLayout.LEFT));
        JButton addEtag = new JButton("Thêm eTag");
        JButton removeEtag = new JButton("Xóa");
        JButton useEtag = new JButton("Chọn vào form (eTag đơn)");
        addEtag.addActionListener(e -> {
            String s = JOptionPane.showInputDialog(this, "Nhập eTag:");
            if (s != null && !s.trim().isEmpty()) {
                etagListModel.addElement(s.trim());
                saveEtags();
            }
        });
        removeEtag.addActionListener(e -> {
            int[] indices = etagList.getSelectedIndices();
            for (int i = indices.length - 1; i >= 0; i--) etagListModel.remove(indices[i]);
            saveEtags();
        });
        useEtag.addActionListener(e -> {
            List<String> sel = etagList.getSelectedValuesList();
            if (!sel.isEmpty()) etagField.setText(sel.get(0));
        });
        etagBtns.add(addEtag);
        etagBtns.add(removeEtag);
        etagBtns.add(useEtag);
        left.add(etagBtns, BorderLayout.SOUTH);
        center.add(left, BorderLayout.WEST);

        JPanel right = new JPanel(new BorderLayout(4, 4));
        right.setBorder(BorderFactory.createTitledBorder(BorderFactory.createEtchedBorder(), "Command & Tham số (tự động theo loại bản tin)", TitledBorder.LEFT, TitledBorder.TOP));

        JPanel topForm = new JPanel(new FlowLayout(FlowLayout.LEFT));
        topForm.add(new JLabel("Command:"));
        topForm.add(commandCombo);
        commandCombo.addActionListener(e -> rebuildDynamicForm());

        dynamicFormPanel.setLayout(new GridBagLayout());
        right.add(topForm, BorderLayout.NORTH);
        right.add(new JScrollPane(dynamicFormPanel), BorderLayout.CENTER);
        JPanel btnRow = new JPanel(new FlowLayout(FlowLayout.LEFT));
        btnRow.add(refillButton);
        btnRow.add(sendButton);
        right.add(btnRow, BorderLayout.SOUTH);

        refillButton.addActionListener(e -> refillFromLastResponse());
        sendButton.addActionListener(e -> doSend());

        rebuildDynamicForm();
        center.add(right, BorderLayout.CENTER);

        return center;
    }

    private void rebuildDynamicForm() {
        dynamicFormPanel.removeAll();
        GridBagConstraints c = new GridBagConstraints();
        c.insets = new Insets(2, 4, 2, 4);
        c.anchor = GridBagConstraints.WEST;
        c.fill = GridBagConstraints.HORIZONTAL;
        int row = 0;

        int cmd = commandCombo.getSelectedIndex();
        c.gridx = 0;
        c.gridy = row;
        dynamicFormPanel.add(new JLabel("Session ID:"), c);
        c.gridx = 1;
        dynamicFormPanel.add(sessionIdField, c);
        row++;

        if (cmd == CMD_CHECKIN) {
            addRow(dynamicFormPanel, c, row++, "eTag:", etagField);
            addRow(dynamicFormPanel, c, row++, "Lane:", laneField);
            addRow(dynamicFormPanel, c, row++, "Plate:", plateField);
            addRow(dynamicFormPanel, c, row++, "TID:", tidField);
            addRow(dynamicFormPanel, c, row++, "Hash:", hashField);
        } else if (cmd == CMD_COMMIT || cmd == CMD_ROLLBACK) {
            addRow(dynamicFormPanel, c, row++, "Ticket ID:", ticketIdField);
            addRow(dynamicFormPanel, c, row++, "eTag:", etagField);
            addRow(dynamicFormPanel, c, row++, "Lane:", laneField);
            addRow(dynamicFormPanel, c, row++, "Status (plate):", statusField);
            addRow(dynamicFormPanel, c, row++, "Plate:", plateField);
        } else if (cmd == CMD_LOOKUP_VEHICLE) {
            addRow(dynamicFormPanel, c, row++, "eTag:", etagField);
            addRow(dynamicFormPanel, c, row++, "Lane:", laneField);
            addRow(dynamicFormPanel, c, row++, "TID:", tidField);
        } else if (cmd == CMD_QUERY_VEHICLE_BOO) {
            addRow(dynamicFormPanel, c, row++, "eTag:", etagField);
            addRow(dynamicFormPanel, c, row++, "Lane:", laneField);
            addRow(dynamicFormPanel, c, row++, "TID:", tidField);
            addRow(dynamicFormPanel, c, row++, "MinBalance:", minBalanceField);
        } else if (cmd == CMD_CHECKOUT_RESERVE) {
            addRow(dynamicFormPanel, c, row++, "eTag:", etagField);
            addRow(dynamicFormPanel, c, row++, "TID:", tidField);
            addRow(dynamicFormPanel, c, row++, "Ticket In ID:", ticketInIdField);
            addRow(dynamicFormPanel, c, row++, "Hub ID:", hubIdField);
            addRow(dynamicFormPanel, c, row++, "Ticket eTag ID:", ticketETagIdField);
            addRow(dynamicFormPanel, c, row++, "Ticket Out ID:", ticketOutIdField);
            addRow(dynamicFormPanel, c, row++, "CheckIn datetime:", checkinDatetimeField);
            addRow(dynamicFormPanel, c, row++, "CheckIn commit datetime:", checkinCommitDatetimeField);
            addRow(dynamicFormPanel, c, row++, "Station In:", stationInField);
            addRow(dynamicFormPanel, c, row++, "Lane In:", laneInField);
            addRow(dynamicFormPanel, c, row++, "Station Out:", stationOutField);
            addRow(dynamicFormPanel, c, row++, "Lane Out:", laneOutField);
            addRow(dynamicFormPanel, c, row++, "Plate:", plateField);
            addRow(dynamicFormPanel, c, row++, "Ticket type:", ticketTypeField);
            addRow(dynamicFormPanel, c, row++, "Price ticket type:", priceTicketTypeField);
            addRow(dynamicFormPanel, c, row++, "Trans amount:", transAmountField);
            addRow(dynamicFormPanel, c, row++, "Trans datetime:", transDatetimeField);
            addRow(dynamicFormPanel, c, row++, "Rating detail line:", ratingDetailLineField);
        } else if (cmd == CMD_CHECKOUT_COMMIT) {
            addRow(dynamicFormPanel, c, row++, "eTag:", etagField);
            addRow(dynamicFormPanel, c, row++, "TID:", tidField);
            addRow(dynamicFormPanel, c, row++, "Ticket In ID:", ticketInIdField);
            addRow(dynamicFormPanel, c, row++, "Hub ID:", hubIdField);
            addRow(dynamicFormPanel, c, row++, "Ticket Out ID:", ticketOutIdField);
            addRow(dynamicFormPanel, c, row++, "Ticket eTag ID:", ticketETagIdField);
            addRow(dynamicFormPanel, c, row++, "Station In:", stationInField);
            addRow(dynamicFormPanel, c, row++, "Lane In:", laneInField);
            addRow(dynamicFormPanel, c, row++, "Station Out:", stationOutField);
            addRow(dynamicFormPanel, c, row++, "Lane Out:", laneOutField);
            addRow(dynamicFormPanel, c, row++, "Plate (20):", plateField);
            addRow(dynamicFormPanel, c, row++, "Trans amount:", transAmountField);
            addRow(dynamicFormPanel, c, row++, "Trans datetime:", transDatetimeField);
        } else if (cmd == CMD_CHECKOUT_ROLLBACK) {
            addRow(dynamicFormPanel, c, row++, "eTag:", etagField);
            addRow(dynamicFormPanel, c, row++, "TID:", tidField);
            addRow(dynamicFormPanel, c, row++, "Ticket In ID:", ticketInIdField);
            addRow(dynamicFormPanel, c, row++, "Hub ID:", hubIdField);
            addRow(dynamicFormPanel, c, row++, "Ticket Out ID:", ticketOutIdField);
            addRow(dynamicFormPanel, c, row++, "Ticket eTag ID:", ticketETagIdField);
            addRow(dynamicFormPanel, c, row++, "Station In:", stationInField);
            addRow(dynamicFormPanel, c, row++, "Lane In:", laneInField);
            addRow(dynamicFormPanel, c, row++, "Station Out:", stationOutField);
            addRow(dynamicFormPanel, c, row++, "Lane Out:", laneOutField);
            addRow(dynamicFormPanel, c, row++, "Plate:", plateField);
            addRow(dynamicFormPanel, c, row++, "Trans amount:", transAmountField);
            addRow(dynamicFormPanel, c, row++, "Trans datetime:", transDatetimeField);
        }
        dynamicFormPanel.revalidate();
        dynamicFormPanel.repaint();
    }

    private static void addRow(JPanel p, GridBagConstraints c, int row, String label, JComponent field) {
        c.gridy = row;
        c.gridx = 0;
        p.add(new JLabel(label), c);
        c.gridx = 1;
        p.add(field, c);
    }

    private JPanel buildBottomPanel() {
        JPanel p = new JPanel(new BorderLayout(4, 4));
        p.setBorder(BorderFactory.createTitledBorder(BorderFactory.createEtchedBorder(), "Trạng thái gửi & Response", TitledBorder.LEFT, TitledBorder.TOP));
        statusResponseArea.setEditable(false);
        statusResponseArea.setFont(new Font(Font.MONOSPACED, Font.PLAIN, 12));
        JButton clearLogButton = new JButton("Clear log");
        clearLogButton.addActionListener(e -> {
            statusResponseArea.setText("");
            sendStatusLabel.setText(" ");
        });
        JPanel topBar = new JPanel(new FlowLayout(FlowLayout.LEFT));
        topBar.add(clearLogButton);
        p.add(topBar, BorderLayout.NORTH);
        p.add(new JScrollPane(statusResponseArea), BorderLayout.CENTER);
        p.add(sendStatusLabel, BorderLayout.SOUTH);
        return p;
    }

    private void loadEtags() {
        for (String etag : EtagListManager.load()) {
            etagListModel.addElement(etag);
        }
    }

    private void saveEtags() {
        List<String> list = new ArrayList<>();
        for (int i = 0; i < etagListModel.getSize(); i++) {
            list.add(etagListModel.getElementAt(i));
        }
        EtagListManager.save(list);
    }

    private void doConnect() {
        String host = hostField.getText().trim();
        String portStr = portField.getText().trim();
        String user = userField.getText().trim();
        String pass = new String(passField.getPassword());
        String key = encryptionKeyField.getText();
        int port;
        try {
            port = Integer.parseInt(portStr);
        } catch (NumberFormatException e) {
            appendStatus("Lỗi: Port không hợp lệ.");
            return;
        }
        final int timeout = DEFAULT_CONNECT_TIMEOUT_SEC;
        if (key.length() != 16) {
            appendStatus("Cảnh báo: Encryption key nên đủ 16 ký tự (AES-128).");
        }
        final String hostFinal = host;
        final int portFinal = port;
        final String userFinal = user;
        final String passFinal = pass;
        final String keyFinal = key;
        final int timeoutFinal = timeout;
        userRequestedDisconnect = false;
        connectButton.setEnabled(false);
        disconnectButton.setEnabled(false);
        SwingWorker<Void, Void> worker = new SwingWorker<Void, Void>() {
            String err;
            FeResponseParser.ParsedResponse connResp;

            @Override
            protected Void doInBackground() throws Exception {
                closeClientQuietly();
                try {
                    FeClient c = new FeClient(hostFinal, portFinal, keyFinal);
                    client = c;
                    byte[] connectMsg = FeMessageBuilder.buildConnect(c.nextRequestId(), userFinal, passFinal, timeoutFinal);
                    c.send(connectMsg);
                    byte[] respDecrypted = c.receive();
                    connResp = FeResponseParser.parse(respDecrypted);
                    lastResponse = connResp;
                    return null;
                } catch (Exception e) {
                    err = e.getMessage();
                    closeClientQuietly();
                    return null;
                }
            }

            @Override
            protected void done() {
                if (err != null) {
                    String msg = isTimeoutOrEmpty(err) ? "Timeout 5s: server không trả bản tin. Đã ngắt kết nối." : err;
                    appendStatus("Kết nối / CONNECT thất bại: " + msg + " (đã thử " + hostFinal + ":" + portFinal + ")");
                    connectButton.setEnabled(true);
                    disconnectButton.setEnabled(false);
                    return;
                }
                if (connResp != null && connResp.commandId == FeConstants.CONNECT_RESP) {
                    if (connResp.status == 0) {
                        lastSessionId = connResp.sessionId;
                        savedHost = hostFinal;
                        savedPort = portFinal;
                        savedKey = keyFinal;
                        savedUser = userFinal;
                        savedPass = passFinal;
                        savedTimeout = timeoutFinal;
                        sessionIdField.setText(String.valueOf(connResp.sessionId));
                        appendStatus("Đã kết nối " + hostFinal + ":" + portFinal + " và gửi CONNECT OK, session_id=" + connResp.sessionId);
                        connectButton.setEnabled(false);
                        disconnectButton.setEnabled(true);
                        startHandshakeTimer();
                    } else {
                        disconnect(false);
                        appendStatus("CONNECT trả về lỗi status=" + connResp.status + " (auth/cấu hình). Đã ngắt kết nối.");
                        connectButton.setEnabled(true);
                        disconnectButton.setEnabled(false);
                    }
                } else {
                    disconnect(false);
                    appendStatus("CONNECT: response không hợp lệ (cmd=" + (connResp != null ? connResp.commandId : -1) + ")");
                    connectButton.setEnabled(true);
                    disconnectButton.setEnabled(false);
                }
            }
        };
        worker.execute();
    }

    /** Chỉ test TCP tới host:port (không gửi FE). Giúp phân biệt lỗi do firewall/Java vs do server. */
    private void doTestTcp() {
        String host = hostField.getText().trim();
        String portStr = portField.getText().trim();
        int port;
        try {
            port = Integer.parseInt(portStr);
        } catch (NumberFormatException e) {
            appendStatus("Test TCP: Port không hợp lệ.");
            return;
        }
        final String h = host;
        final int pt = port;
        testTcpButton.setEnabled(false);
        appendStatus("Test TCP " + h + ":" + pt + " ...");
        SwingWorker<String, Void> w = new SwingWorker<String, Void>() {
            @Override
            protected String doInBackground() {
                try (Socket s = new Socket()) {
                    s.connect(new InetSocketAddress(h, pt), FeClient.CONNECT_AND_READ_TIMEOUT_MS);
                    return null; // success
                } catch (Exception e) {
                    return e.getMessage() != null ? e.getMessage() : e.toString();
                }
            }
            @Override
            protected void done() {
                testTcpButton.setEnabled(true);
                try {
                    String err = get();
                    if (err == null) {
                        appendStatus("Test TCP " + h + ":" + pt + " OK. Java kết nối được. Nếu Kết nối vẫn lỗi thì do giao thức/encryption key.");
                    } else {
                        appendStatus("Test TCP " + h + ":" + pt + " thất bại: " + err + " → Kiểm tra firewall/AV cho Java, hoặc server không listen port này.");
                    }
                } catch (Exception ignored) { }
            }
        };
        w.execute();
    }

    /** Chỉ đóng socket và clear client, không đổi nút / không ghi trạng thái (dùng trong worker). */
    private void closeClientQuietly() {
        if (client != null) {
            try {
                client.close();
            } catch (Exception ignored) { }
            client = null;
        }
    }

    /** True nếu lỗi là timeout đọc hoặc bản tin rỗng (để hiển thị thống nhất). */
    private static boolean isTimeoutOrEmpty(String err) {
        if (err == null) return false;
        return err.contains("timed out") || err.contains("Timeout") || err.contains("bản tin rỗng");
    }

    /**
     * Ngắt kết nối và cập nhật UI.
     * @param notifyUser true = ghi "Đã ngắt kết nối." (khi user bấm Ngắt); false = chỉ đóng, không ghi (khi lỗi CONNECT/reconnect).
     */
    private void disconnect(boolean notifyUser) {
        userRequestedDisconnect = true;
        cancelHandshake();
        if (client != null) {
            try {
                client.close();
            } catch (Exception ignored) { }
            client = null;
        }
        if (notifyUser) {
            appendStatus("Đã ngắt kết nối.");
        }
        connectButton.setEnabled(true);
        disconnectButton.setEnabled(false);
    }

    private void cancelHandshake() {
        if (handshakeTask != null) {
            handshakeTask.cancel(false);
            handshakeTask = null;
        }
    }

    private void startHandshakeTimer() {
        cancelHandshake();
        handshakeTask = handshakeExecutor.scheduleAtFixedRate(() -> {
            FeClient c = client;
            if (c == null || !c.isConnected()) return;
            long sid = lastSessionId;
            try {
                byte[] msg = FeMessageBuilder.buildHandshake(c.nextRequestId(), sid);
                c.send(msg);
                c.receive();
            } catch (Exception e) {
                connectionLost();
            }
        }, 5, 5, TimeUnit.SECONDS);
    }

    private void connectionLost() {
        cancelHandshake();
        if (client != null) {
            try { client.close(); } catch (Exception ignored) { }
            client = null;
        }
        SwingUtilities.invokeLater(() -> appendStatus("Mất kết nối, đang reconnect..."));
        if (!userRequestedDisconnect && !reconnectInProgress && savedHost != null) startReconnect();
    }

    private void startReconnect() {
        if (reconnectInProgress) return;
        reconnectInProgress = true;
        final String host = savedHost;
        final int port = savedPort;
        final String key = savedKey;
        final String user = savedUser;
        final String pass = savedPass;
        final int timeout = savedTimeout;
        SwingWorker<Void, Void> worker = new SwingWorker<Void, Void>() {
            String err;
            FeResponseParser.ParsedResponse connResp;

            @Override
            protected Void doInBackground() throws Exception {
                try {
                    FeClient c = new FeClient(host, port, key);
                    client = c;
                    byte[] connectMsg = FeMessageBuilder.buildConnect(c.nextRequestId(), user, pass, timeout);
                    c.send(connectMsg);
                    byte[] respDecrypted = c.receive();
                    connResp = FeResponseParser.parse(respDecrypted);
                    lastResponse = connResp;
                    return null;
                } catch (Exception e) {
                    err = e.getMessage();
                    if (client != null) {
                        try { client.close(); } catch (Exception ignored) { }
                        client = null;
                    }
                    return null;
                }
            }

            @Override
            protected void done() {
                reconnectInProgress = false;
                if (err != null) {
                    String msg = isTimeoutOrEmpty(err) ? "Timeout 5s: server không trả bản tin. Đã ngắt kết nối." : err;
                    appendStatus("Reconnect thất bại: " + msg);
                    connectButton.setEnabled(true);
                    disconnectButton.setEnabled(false);
                    return;
                }
                if (connResp != null && connResp.commandId == FeConstants.CONNECT_RESP && connResp.status == 0) {
                    userRequestedDisconnect = false;
                    lastSessionId = connResp.sessionId;
                    sessionIdField.setText(String.valueOf(connResp.sessionId));
                    appendStatus("Đã reconnect, session_id=" + connResp.sessionId);
                    connectButton.setEnabled(false);
                    disconnectButton.setEnabled(true);
                    startHandshakeTimer();
                } else {
                    appendStatus("Reconnect: response không hợp lệ hoặc status=" + (connResp != null ? connResp.status : -1));
                    connectButton.setEnabled(true);
                    disconnectButton.setEnabled(false);
                }
            }
        };
        worker.execute();
    }

    private void refillFromLastResponse() {
        if (lastResponse == null) {
            appendStatus("Chưa có response nào để refill.");
            return;
        }
        // session_id chỉ lấy từ CONNECT_RESP khi connect thành công, không ghi đè từ response khác
        ticketIdField.setText(String.valueOf(lastResponse.ticketId));
        ticketInIdField.setText(String.valueOf(lastResponse.ticketInId));
        ticketOutIdField.setText(String.valueOf(lastResponse.ticketOutId));
        ticketETagIdField.setText(String.valueOf(lastResponse.ticketETagId));
        hubIdField.setText(String.valueOf(lastResponse.hubId));
        if (lastResponse.etag != null && !lastResponse.etag.isEmpty()) etagField.setText(lastResponse.etag.trim());
        if (lastResponse.station != 0) {
            stationField.setText(String.valueOf(lastResponse.station));
            stationInField.setText(String.valueOf(lastResponse.station));
            stationOutField.setText(String.valueOf(lastResponse.station));
        }
        if (lastResponse.lane != 0) {
            laneField.setText(String.valueOf(lastResponse.lane));
            laneInField.setText(String.valueOf(lastResponse.lane));
            laneOutField.setText(String.valueOf(lastResponse.lane));
        }
        if (lastResponse.plate != null && !lastResponse.plate.isEmpty()) plateField.setText(lastResponse.plate.trim());
        appendStatus("Đã refill từ response trước: " + lastResponse.summary);
    }

    private void doSend() {
        if (client == null || !client.isConnected()) {
            appendStatus("Chưa kết nối. Nhấn Kết nối trước.");
            return;
        }
        List<String> etags = getEtagsToSend();
        if (etags.isEmpty()) {
            appendStatus("Chọn ít nhất một eTag trong danh sách hoặc nhập eTag vào ô eTag.");
            return;
        }
        sendStatusLabel.setText("Đang gửi " + etags.size() + " request...");
        sendButton.setEnabled(false);
        SwingWorker<Void, Void> worker = new SwingWorker<Void, Void>() {
            final List<String> errors = new ArrayList<>();
            final List<FeResponseParser.ParsedResponse> responses = new ArrayList<>();
            final List<String> requestLogs = new ArrayList<>();
            boolean disconnectedDuringSend = false;

            @Override
            protected Void doInBackground() throws Exception {
                int idx = commandCombo.getSelectedIndex();
                for (int i = 0; i < etags.size(); i++) {
                    String etag = etags.get(i);
                    try {
                        byte[] plain = buildRequestForEtag(idx, etag);
                        if (plain == null) {
                            errors.add("eTag " + etag + ": Không hỗ trợ command hoặc thiếu tham số.");
                            continue;
                        }
                        client.send(plain);
                        byte[] respDecrypted = client.receive();
                        FeResponseParser.ParsedResponse pr = FeResponseParser.parse(respDecrypted);
                        responses.add(pr);
                        lastResponse = pr;
                        appendStatus("  Response: " + (lastResponse != null ? lastResponse.toString() : "(none)"));
                    } catch (Exception e) {
                        String msg = e.getMessage() != null ? e.getMessage() : "";
                        if (isTimeoutOrEmpty(msg)) {
                            errors.add("eTag " + etag + ": Timeout 5s: server không trả bản tin. Đã ngắt kết nối.");
                            closeClientQuietly();
                            disconnectedDuringSend = true;
                            break;
                        }
                        errors.add("eTag " + etag + ": " + msg);
                    }
                }
                return null;
            }

            @Override
            protected void done() {
                sendButton.setEnabled(true);
                for (String line : requestLogs) appendStatus(line);
                for (String err : errors) appendStatus("Lỗi: " + err);
                for (int i = 0; i < responses.size(); i++) {
                    FeResponseParser.ParsedResponse pr = responses.get(i);
                    String etag = i < etags.size() ? etags.get(i) : "?";
                    appendStatus("[" + etag + "] " + pr.summary);
                    if (pr.rawHex != null)
                        appendStatus("  Response (raw hex): " + pr.rawHex);
                }
                if (disconnectedDuringSend) {
                    appendStatus("Đã ngắt kết nối (server không trả bản tin hoặc timeout 5s).");
                    connectButton.setEnabled(true);
                    disconnectButton.setEnabled(false);
                    if (!userRequestedDisconnect && savedHost != null && !reconnectInProgress) startReconnect();
                }
                if (!responses.isEmpty())
                    sendStatusLabel.setText("OK: " + responses.size() + " response(s). " + (errors.isEmpty() ? "" : errors.size() + " lỗi."));
                else if (!errors.isEmpty())
                    sendStatusLabel.setText("Lỗi: " + String.join("; ", errors));
            }
        };
        worker.execute();
    }

    /** Selected etags from list, or single from etag field if none selected. */
    private List<String> getEtagsToSend() {
        List<String> sel = etagList.getSelectedValuesList();
        if (!sel.isEmpty()) return new ArrayList<>(sel);
        String one = etagField.getText().trim();
        if (!one.isEmpty()) return List.of(one);
        return List.of();
    }

    private byte[] buildRequestForEtag(int commandIndex, String etag) {
        long reqId = client.nextRequestId();
        long sessionId = lastSessionId;
        long ticketId = parseLong(ticketIdField.getText(), 0);
        int station = (int) parseLong(stationField.getText(), 0);
        int lane = (int) parseLong(laneField.getText(), 0);
        String plate = plateField.getText().trim();
        String tid = tidField.getText().trim();
        String hash = hashField.getText().trim();
        int status = (int) parseLong(statusField.getText(), 0);
        int minBalance = (int) parseLong(minBalanceField.getText(), 0);

        long ticketInId = parseLong(ticketInIdField.getText(), 0);
        long ticketOutId = parseLong(ticketOutIdField.getText(), 0);
        long ticketETagId = parseLong(ticketETagIdField.getText(), 0);
        long hubId = parseLong(hubIdField.getText(), 0);
        long checkinDt = parseLong(checkinDatetimeField.getText(), 0);
        long checkinCommitDt = parseLong(checkinCommitDatetimeField.getText(), 0);
        int stationIn = (int) parseLong(stationInField.getText(), 0);
        int laneIn = (int) parseLong(laneInField.getText(), 0);
        int stationOut = (int) parseLong(stationOutField.getText(), 0);
        int laneOut = (int) parseLong(laneOutField.getText(), 0);
        int transAmount = (int) parseLong(transAmountField.getText(), 0);
        long transDatetime = parseLong(transDatetimeField.getText(), 0);
        int priceTicketType = (int) parseLong(priceTicketTypeField.getText(), 0);
        int ratingDetailLine = (int) parseLong(ratingDetailLineField.getText(), 0);
        String ticketType = ticketTypeField.getText().trim();

        switch (commandIndex) {
            case CMD_CHECKIN:
                return FeMessageBuilder.buildCheckin(reqId, sessionId, etag, station, lane, plate, tid, hash);
            case CMD_COMMIT:
                return FeMessageBuilder.buildCommit(reqId, sessionId, etag, station, lane, ticketId, status, plate, 0, 0, 0, 0, 0);
            case CMD_ROLLBACK:
                return FeMessageBuilder.buildRollback(reqId, sessionId, etag, station, lane, ticketId, status, plate, 0, 0, 0);
            case CMD_LOOKUP_VEHICLE:
                return FeMessageBuilder.buildLookupVehicle(reqId, sessionId, System.currentTimeMillis(), tid, etag, "C", "I", null, null);
            case CMD_QUERY_VEHICLE_BOO:
                return FeMessageBuilder.buildQueryVehicleBoo(reqId, sessionId, System.currentTimeMillis(), tid, etag, station, lane, "C", "I", minBalance, null, null);
            case CMD_CHECKOUT_RESERVE:
                return FeMessageBuilder.buildCheckoutReserveBoo(reqId, sessionId, System.currentTimeMillis(), tid, etag, ticketInId, hubId, ticketETagId, ticketOutId, checkinDt, checkinCommitDt, stationIn, laneIn, stationOut, laneOut, plate, ticketType, priceTicketType, transAmount, transDatetime, ratingDetailLine, null, null);
            case CMD_CHECKOUT_COMMIT:
                return FeMessageBuilder.buildCheckoutCommitBoo(reqId, sessionId, System.currentTimeMillis(), tid, etag, ticketInId, hubId, ticketOutId, ticketETagId, stationIn, laneIn, stationOut, laneOut, plate, transAmount, transDatetime, null, null);
            case CMD_CHECKOUT_ROLLBACK:
                return FeMessageBuilder.buildCheckoutRollbackBoo(reqId, sessionId, System.currentTimeMillis(), tid, etag, ticketInId, hubId, ticketOutId, ticketETagId, stationIn, laneIn, stationOut, laneOut, plate, transAmount, transDatetime, null, null);
            default:
                return null;
        }
    }

    private static long parseLong(String s, long def) {
        if (s == null || s.trim().isEmpty()) return def;
        try {
            return Long.parseLong(s.trim());
        } catch (NumberFormatException e) {
            return def;
        }
    }

    private static String bytesToHex(byte[] bytes) {
        if (bytes == null) return "";
        StringBuilder sb = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) sb.append(String.format("%02X", b & 0xFF));
        return sb.toString();
    }

    /** Decode bytes as UTF-8; replace invalid sequences and non-printable/control chars with '.'. */
    private static String bytesToUnicodeSafe(byte[] bytes) {
        if (bytes == null) return "";
        String s;
        try {
            s = new String(bytes, java.nio.charset.StandardCharsets.UTF_8);
        } catch (Exception e) {
            return "(invalid UTF-8)";
        }
        StringBuilder sb = new StringBuilder(s.length());
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            sb.append(c >= 32 && c != 127 ? c : '.');
        }
        return sb.toString();
    }

    private void appendStatus(String line) {
        SwingUtilities.invokeLater(() -> {
            statusResponseArea.append(line + "\n");
            statusResponseArea.setCaretPosition(statusResponseArea.getDocument().getLength());
        });
    }

    public static void main(String[] args) {
        try {
            UIManager.setLookAndFeel(UIManager.getSystemLookAndFeelClassName());
        } catch (Exception ignored) { }
        SwingUtilities.invokeLater(() -> {
            FeTestClientApp app = new FeTestClientApp();
            app.setVisible(true);
        });
    }
}
