# FE Test Client

Công cụ Java (Swing) để test kết nối và gửi bản tin giao thức FE (TCOC) tới backend Rust.

## Chức năng

- **Quản lý kết nối**: IP, Port, Encryption key (16 ký tự AES-128), Station, Lane. Không gửi CONNECT/SHAKE/TERMINATE (session_id nhập tay hoặc lấy từ tool khác).
- **Danh sách eTag**: Thêm/Xóa eTag; chọn **nhiều** eTag → gửi tự động **một request cho mỗi eTag** (cùng tham số khác).
- **Command**: CHECKIN, COMMIT, ROLLBACK, LOOKUP_VEHICLE, QUERY_VEHICLE_BOO. Khi đổi command, form **tự động hiện đúng trường** tương ứng request.
- **Refill từ response trước**: Điền session_id, ticket_id, etag, station, lane, plate từ response gần nhất.
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

1. Nhập IP, Port, **Encryption key 16 ký tự**, Station, Lane → **Kết nối** (session_id lấy từ hệ thống khác hoặc nhập tay).
2. Chọn **Command**: form sẽ chỉ hiện các trường tương ứng (CHECKIN: eTag, Station, Lane, Plate, TID, Hash; COMMIT/ROLLBACK: Ticket ID, eTag, Station, Lane, Status, Plate; LOOKUP: eTag, Station, Lane, TID; QUERY: eTag, Station, Lane, TID, MinBalance).
3. **Nhiều eTag**: chọn nhiều eTag trong danh sách → **Gửi** = gửi lần lượt 1 request/eTag (cùng Session ID, Station, Lane, …). Nếu không chọn eTag nào thì dùng ô **eTag** đơn.
4. **Refill từ response trước**: Sau khi có response (ví dụ CHECKIN_RESP), nhấn để điền session_id, ticket_id, etag, station, lane, plate cho COMMIT/ROLLBACK tiếp.

Bản tin: AES-128-CBC (IV = 0), frame 4 byte length (LE) + encrypted payload.
