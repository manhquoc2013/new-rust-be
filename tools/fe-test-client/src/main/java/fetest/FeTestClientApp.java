package fetest;

import javax.swing.*;
import javax.swing.border.TitledBorder;
import java.awt.*;
import java.awt.event.WindowAdapter;
import java.awt.event.WindowEvent;
import java.util.ArrayList;
import java.util.List;

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

    private final JTextField hostField = new JTextField("127.0.0.1", 12);
    private final JTextField portField = new JTextField("7000", 6);
    private final JTextField encryptionKeyField = new JTextField(16);
    private final JTextField stationField = new JTextField("0", 6);
    private final JTextField laneField = new JTextField("0", 4);

    private final DefaultListModel<String> etagListModel = new DefaultListModel<>();
    private final JList<String> etagList = new JList<>(etagListModel);

    private final JComboBox<String> commandCombo = new JComboBox<>(new String[]{
            "CHECKIN (0x66)", "COMMIT (0x68)", "ROLLBACK (0x6A)", "LOOKUP_VEHICLE (0x96)", "QUERY_VEHICLE_BOO (0x64)"
    });

    private final JTextField sessionIdField = new JTextField("0", 14);
    private final JTextField ticketIdField = new JTextField("0", 14);
    private final JTextField etagField = new JTextField("", 22);
    private final JTextField plateField = new JTextField("", 12);
    private final JTextField tidField = new JTextField("", 20);
    private final JTextField hashField = new JTextField("", 18);
    private final JTextField statusField = new JTextField("0", 4);
    private final JTextField minBalanceField = new JTextField("0", 6);

    private final JPanel dynamicFormPanel = new JPanel();
    private final JButton refillButton = new JButton("Refill từ response trước");
    private final JButton sendButton = new JButton("Gửi (1 request / eTag đã chọn)");
    private final JButton connectButton = new JButton("Kết nối");
    private final JButton disconnectButton = new JButton("Ngắt");

    private final JTextArea statusResponseArea = new JTextArea(12, 60);
    private final JLabel sendStatusLabel = new JLabel(" ");

    private FeClient client;
    private FeResponseParser.ParsedResponse lastResponse;

    public FeTestClientApp() {
        setTitle("FE Test Client - TCOC Protocol");
        setDefaultCloseOperation(JFrame.DO_NOTHING_ON_CLOSE);
        addWindowListener(new WindowAdapter() {
            @Override
            public void windowClosing(WindowEvent e) {
                disconnect();
                dispose();
            }
        });

        JPanel main = new JPanel(new BorderLayout(8, 8));
        main.setBorder(BorderFactory.createEmptyBorder(8, 8, 8, 8));

        JPanel top = new JPanel(new BorderLayout(4, 4));
        top.add(buildConnectionPanel(), BorderLayout.NORTH);
        top.add(buildCenterPanel(), BorderLayout.CENTER);
        main.add(top, BorderLayout.CENTER);
        main.add(buildBottomPanel(), BorderLayout.SOUTH);

        setContentPane(main);
        pack();
        setMinimumSize(new Dimension(720, 580));
        setLocationRelativeTo(null);
    }

    private JPanel buildConnectionPanel() {
        JPanel p = new JPanel(new FlowLayout(FlowLayout.LEFT, 6, 4));
        p.setBorder(BorderFactory.createTitledBorder(BorderFactory.createEtchedBorder(), "Kết nối", TitledBorder.LEFT, TitledBorder.TOP));

        p.add(new JLabel("IP:"));
        p.add(hostField);
        p.add(new JLabel("Port:"));
        p.add(portField);
        p.add(new JLabel("Encryption key (16):"));
        encryptionKeyField.setToolTipText("16 ký tự cho AES-128");
        p.add(encryptionKeyField);
        p.add(new JLabel("Station:"));
        p.add(stationField);
        p.add(new JLabel("Lane:"));
        p.add(laneField);
        p.add(connectButton);
        p.add(disconnectButton);
        disconnectButton.setEnabled(false);

        connectButton.addActionListener(e -> doConnect());
        disconnectButton.addActionListener(e -> disconnect());

        return p;
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
            }
        });
        removeEtag.addActionListener(e -> {
            int[] indices = etagList.getSelectedIndices();
            for (int i = indices.length - 1; i >= 0; i--) etagListModel.remove(indices[i]);
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
            addRow(dynamicFormPanel, c, row++, "Station:", stationField);
            addRow(dynamicFormPanel, c, row++, "Lane:", laneField);
            addRow(dynamicFormPanel, c, row++, "Plate:", plateField);
            addRow(dynamicFormPanel, c, row++, "TID:", tidField);
            addRow(dynamicFormPanel, c, row++, "Hash:", hashField);
        } else if (cmd == CMD_COMMIT || cmd == CMD_ROLLBACK) {
            addRow(dynamicFormPanel, c, row++, "Ticket ID:", ticketIdField);
            addRow(dynamicFormPanel, c, row++, "eTag:", etagField);
            addRow(dynamicFormPanel, c, row++, "Station:", stationField);
            addRow(dynamicFormPanel, c, row++, "Lane:", laneField);
            addRow(dynamicFormPanel, c, row++, "Status (plate):", statusField);
            addRow(dynamicFormPanel, c, row++, "Plate:", plateField);
        } else if (cmd == CMD_LOOKUP_VEHICLE) {
            addRow(dynamicFormPanel, c, row++, "eTag:", etagField);
            addRow(dynamicFormPanel, c, row++, "Station:", stationField);
            addRow(dynamicFormPanel, c, row++, "Lane:", laneField);
            addRow(dynamicFormPanel, c, row++, "TID:", tidField);
        } else if (cmd == CMD_QUERY_VEHICLE_BOO) {
            addRow(dynamicFormPanel, c, row++, "eTag:", etagField);
            addRow(dynamicFormPanel, c, row++, "Station:", stationField);
            addRow(dynamicFormPanel, c, row++, "Lane:", laneField);
            addRow(dynamicFormPanel, c, row++, "TID:", tidField);
            addRow(dynamicFormPanel, c, row++, "MinBalance:", minBalanceField);
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
        p.add(new JScrollPane(statusResponseArea), BorderLayout.CENTER);
        p.add(sendStatusLabel, BorderLayout.SOUTH);
        return p;
    }

    private void doConnect() {
        String host = hostField.getText().trim();
        String portStr = portField.getText().trim();
        int port;
        try {
            port = Integer.parseInt(portStr);
        } catch (NumberFormatException e) {
            appendStatus("Lỗi: Port không hợp lệ.");
            return;
        }
        String key = encryptionKeyField.getText();
        if (key.length() != 16) {
            appendStatus("Cảnh báo: Encryption key nên đủ 16 ký tự (AES-128).");
        }
        try {
            disconnect();
            client = new FeClient(host, port, key);
            appendStatus("Đã kết nối " + host + ":" + port);
            connectButton.setEnabled(false);
            disconnectButton.setEnabled(true);
        } catch (Exception e) {
            appendStatus("Kết nối thất bại: " + e.getMessage());
        }
    }

    private void disconnect() {
        if (client != null) {
            try {
                client.close();
            } catch (Exception ignored) { }
            client = null;
        }
        appendStatus("Đã ngắt kết nối.");
        connectButton.setEnabled(true);
        disconnectButton.setEnabled(false);
    }

    private void refillFromLastResponse() {
        if (lastResponse == null) {
            appendStatus("Chưa có response nào để refill.");
            return;
        }
        sessionIdField.setText(String.valueOf(lastResponse.sessionId));
        ticketIdField.setText(String.valueOf(lastResponse.ticketId));
        if (lastResponse.etag != null && !lastResponse.etag.isEmpty()) etagField.setText(lastResponse.etag.trim());
        if (lastResponse.station != 0) stationField.setText(String.valueOf(lastResponse.station));
        if (lastResponse.lane != 0) laneField.setText(String.valueOf(lastResponse.lane));
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
                    } catch (Exception e) {
                        errors.add("eTag " + etag + ": " + e.getMessage());
                    }
                }
                return null;
            }

            @Override
            protected void done() {
                sendButton.setEnabled(true);
                for (String err : errors) appendStatus("Lỗi: " + err);
                for (int i = 0; i < responses.size(); i++) {
                    FeResponseParser.ParsedResponse pr = responses.get(i);
                    String etag = i < etags.size() ? etags.get(i) : "?";
                    appendStatus("[" + etag + "] " + pr.summary);
                    if (pr.rawHex != null && pr.rawHex.length() > 150)
                        appendStatus("  Hex: " + pr.rawHex.substring(0, 150) + "...");
                    else if (pr.rawHex != null)
                        appendStatus("  Hex: " + pr.rawHex);
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
        long sessionId = parseLong(sessionIdField.getText(), 0);
        long ticketId = parseLong(ticketIdField.getText(), 0);
        int station = (int) parseLong(stationField.getText(), 0);
        int lane = (int) parseLong(laneField.getText(), 0);
        String plate = plateField.getText().trim();
        String tid = tidField.getText().trim();
        String hash = hashField.getText().trim();
        int status = (int) parseLong(statusField.getText(), 0);
        int minBalance = (int) parseLong(minBalanceField.getText(), 0);

        switch (commandIndex) {
            case CMD_CHECKIN:
                return FeMessageBuilder.buildCheckin(reqId, sessionId, etag, station, lane, plate, tid, hash);
            case CMD_COMMIT:
                return FeMessageBuilder.buildCommit(reqId, sessionId, etag, station, lane, ticketId, status, plate, 0, 0, 0, 0, 0);
            case CMD_ROLLBACK:
                return FeMessageBuilder.buildRollback(reqId, sessionId, etag, station, lane, ticketId, status, plate, 0, 0, 0);
            case CMD_LOOKUP_VEHICLE:
                return FeMessageBuilder.buildLookupVehicle(reqId, sessionId, System.currentTimeMillis(), tid, etag, station, lane, "C", "I", null, null);
            case CMD_QUERY_VEHICLE_BOO:
                return FeMessageBuilder.buildQueryVehicleBoo(reqId, sessionId, System.currentTimeMillis(), tid, etag, station, lane, "C", "I", minBalance, null, null);
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
