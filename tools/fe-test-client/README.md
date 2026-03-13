# FE Test Client

Công cụ Java (Swing) để test kết nối và gửi bản tin giao thức FE (TCOC) tới backend Rust.

## Chức năng

- **Quản lý nhiều kết nối, lưu lại**: Danh sách profile (Tên, IP, Port, **User, Pass**, Encryption key, Station, Lane). **Thêm / Sửa / Xóa** profile; **Tải** = điền form từ profile đang chọn. Lưu vào `~/.fe-test-client/connections.properties`, tự load khi mở app.
- **Danh sách eTag**: Thêm/Xóa eTag; chọn **nhiều** eTag → gửi tự động **một request cho mỗi eTag** (cùng tham số khác).
- **Đầy đủ bản tin CheckIn / CheckOut / Commit / Rollback**:
  - **CHECKIN** (0x66), **COMMIT** (0x68 checkin-commit), **ROLLBACK** (0x6A checkin-rollback)
  - **LOOKUP_VEHICLE** (0x96), **QUERY_VEHICLE_BOO** (0x64)
  - **CHECKOUT_RESERVE** (0x98), **CHECKOUT_COMMIT** (0x9A), **CHECKOUT_ROLLBACK** (0x9C)
  Khi đổi command, form **tự động hiện đúng trường** tương ứng (session_id, ticket_in_id, ticket_out_id, station_in/out, lane_in/out, v.v.).
- **Refill từ response trước**: Điền session_id, ticket_id, ticket_in_id, ticket_out_id, ticket_eTag_id, hub_id, etag, station, lane, plate từ response gần nhất (CHECKIN_RESP, CHECKOUT_*_RESP).
- **Trạng thái & Response**: Hiển thị trạng thái gửi và từng response (theo eTag).

## Yêu cầu

- Java 11+
- Maven 3.x

## Build & chạy

```bash
cd tools/fe-test-client
mvn compile
mvn exec:java -Dexec.mainClass="fetest.FeTestClientApp"
```

Hoặc đóng gói rồi chạy JAR:

```bash
mvn package
java -jar target/fe-test-client-1.0.0.jar
```

## Cách dùng

1. **Kết nối**: Chọn profile trong **Profile** (hoặc Thêm mới: nhập IP, Port, Encryption key 16 ký tự, Station, Lane → **Thêm** → đặt tên). **Tải** = điền form từ profile đang chọn. **Kết nối** = mở socket tới IP:Port đã điền (session_id nhập tay hoặc lấy từ refill).
2. Chọn **Command**: form hiện đúng trường theo loại bản tin (CHECKIN; COMMIT/ROLLBACK; LOOKUP; QUERY; CHECKOUT_RESERVE / CHECKOUT_COMMIT / CHECKOUT_ROLLBACK với ticket_in_id, ticket_out_id, station_in/out, lane_in/out, trans_amount, …).
3. **Nhiều eTag**: chọn nhiều eTag trong danh sách → **Gửi** = gửi lần lượt 1 request/eTag. Nếu không chọn thì dùng ô **eTag** đơn.
4. **Refill từ response trước**: Sau khi có response (CHECKIN_RESP, CHECKOUT_*_RESP), nhấn để điền session_id, ticket_id, ticket_in_id, ticket_out_id, ticket_eTag_id, hub_id, etag, station, lane, plate cho các bản tin tiếp theo.

Bản tin: AES-128-CBC (IV = 0), frame 4 byte length (LE) + encrypted payload.
