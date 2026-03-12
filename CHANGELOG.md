# Lịch sử phiên bản (Changelog)

Mỗi phiên bản được mô tả bằng danh sách gạch đầu dòng (một ý thay đổi = một gạch đầu dòng).

## 0.2.5 (2026-03-06)

- **ETDR retry**: Key theo ticket_id (ENV `ETDR_RETRY_KEY_PREFIX`); merge cùng ticket_id trong sync task và merge module; mỗi chu kỳ một bản ghi, lock → ghi DB → cập nhật KeyDB/bộ nhớ → release lock (HA-safe); thêm tracing và log lỗi KeyDB.
- **Trùng ticket_in_id**: Hằng `DUPLICATE_TRANSACTION` (213); checkin common `ensure_no_duplicate_ticket_in_id_boo`/`bect`; checkin/commit VDTC/VETC/BECT kiểm tra trùng ticket_in_id trước save/BOO, trả 213 khi conflict; helpers thêm DUPLICATE_TRANSACTION vào `fe_status_detail`.
- **Processor / TCOC**: Trạng thái thành công TCOC dùng "0"; CONNECT/SHAKE response status `Some(0)` để đồng bộ với TCOC.
- **Roaming**: Luồng cache-first với ROAMING_REQUEST insert trì hoãn; thêm `req_receive_ms` cho process_duration; refactor boo_transport_trans_stage, roaming_request; sửa processor, rollback VDTC/VETC, tcoc_request_service, tcoc_response_service (nhất quán).
- **Logging**: Tách config, common, daily_rotating_writer; bỏ rotation.rs; thêm module statistics (log_db_retry, log_db_time) — ETDR sync và transport/BOO stage services ghi log thao tác DB; cleanup thêm `statistics_retention_days` (main đặt 30 ngày); constants bỏ submodule logging (chuyển/inline vào logging). Node ID (env `LOG_NODE_ID` hoặc random 6 ký tự hex) cho file log theo instance, suffix `{type}-{node_id}`; rotate theo kích thước với node_id trong đường dẫn; cắt file chính sau copy; xử lý lỗi truncate trên Windows. SanitizeWriter fast path khi buffer không có null/control; ensure_file_for_today chỉ kiểm tra ngày khi cần (flush 64KB hoặc chưa có file).
- **Refactor**: Handlers checkin VDTC/VETC, commit BECT/VDTC/VETC và transport/BOO stage services; style format logging trong cleanup và common.

## 0.2.4 (2026-03-05)

- **Commit VDTC/VETC**: Dùng CHECKOUT_COMMIT_BOO (154) chỉ cho làn OUT; bỏ checkin_from_sync khỏi lựa chọn command và log. Giữ nguyên checkout_datetime trong ETDR và Kafka khi đã set; VETC dùng checkout_datetime từ DB cho Kafka payload. Validate session và serialization (general1/general2).
- **ETDR / BOO / transport stage**: Thêm sync_status (0=chưa đồng bộ, 1=đã đồng bộ) vào model ETDR và entity BOO/transport stage; repo DB và merge/build cập nhật. Commit BECT/VDTC/VETC set sync_status=0 khi commit thành công (stage + ETDR cache). Thêm hằng BEST_RECORD_BOO_SYNC_WINDOW_MS cho lựa chọn bản ghi tốt nhất ETDR.
- **Checkin VDTC/VETC**: Luồng BOO stage, messages, common helpers (build_boo_from_sync, build_transport_stage_from_sync). Khi Sync khớp etag+ticket_in_id trong ETDR thì fill ETDR từ Sync và return; thêm fill_etdr_from_sync cho BOO/BECT. Bỏ ensure_fe_resp_ticket_id và get_ticket_id_for_boo_checkin_or_fallback từ common.
- **Checkin common**: allow(dead_code) cho get_existing_pending_ticket_id_for_etag.
- **Toll fee**: Whitelist rating dùng price_ticket_type DEFAULT (1) thay vì WL_TOLL. direct_price truyền price_id, bot_id, stage_id vào regular rating detail cho mọi segment. Refactor types, flow, direct_price, detail; toll_stage_dto alignment.
- **ETDR models**: Sync build và types_impl (constructors, types, updates). Refactor sync retry model.
- **Processor / TCOC / roaming**: Lưu TCOC response theo direction/station_in_for_out; thêm apply_post_handler_tcoc_update (spawn). Checkin/commit/rollback VDTC/VETC truyền direction, station_in_for_out và gọi post-handler TCOC update. Roaming: repo và handler (roaming_request, roaming_response) cập nhật theo direction/toll_in/toll_out. Luồng fire-and-forget cache-first: save_roaming_request_fire_and_forget, spawn_save_roaming_response_after_send_nonblocking; retry khi cache id=None; thêm req_send_timestamp/req_transmit_duration. Chuyển db_datetime_string_to_epoch_ms sang utils/epoch_datetime; checkin/commit/rollback VDTC/VETC dùng lưu roaming fire-and-forget.
- **Fix**: Tính duration time cho roaming; convert trans_amount none thành 0 khi checkOut.
- **DB / repos / services**: boo_transport_trans_stage và transport_transaction_stage repos (insert/update); roaming_request và transport_transaction_stage repos và transport_transaction_stage_service; tcoc_request/tcoc_response repos và services align direction/toll_in/toll_out.
- **Logging**: Cập nhật logging.mdc (ngôn ngữ, format, prefix component, cấu trúc). Cải thiện file_logger; bỏ hex/raw debug dài trong boo1_client và boo2_client. Áp dụng convention (prefix, fields) trong handshake, processor, services, keydb, constants, cleanup, common.
- **Refactor**: Handlers checkin/commit/rollback (common, VDTC, VETC), processor, roaming; file_logger; toll_stage_cache; price_ticket_type.
- **Config / chore**: Cargo.toml bật socket2 "all" features. Constants: MAX_ROTATED_FILES 100→1000. Scripts: next-version-tag.ps1 và README cho version-build tag; vietnamese_comments_to_english.py. project.mdc tham chiếu quy tắc commit message (tiếng Anh).
- **Docs / style**: Chuyển comment và doc comment tiếng Việt sang tiếng Anh trong src và .cursor rules. Format code (formatter).

## 0.2.3 (2026-03-03)

- **Config VDTC**: VDTC multi-host: `VDTC_HOST` nhận danh sách IP cách nhau bằng `;` (vd. IP1;IP2;IP3), mỗi IP mở một kết nối; request gửi round-robin, response nhận từ bất kỳ; thêm `VDTC_REQUEST_TIMEOUT_SECS` (mặc định 10) cho timeout từng request VDTC.
- **Config server**: TCP keepalive trên server (phát hiện kết nối half-open): `TCP_KEEPALIVE_IDLE_SECONDS`, `TCP_KEEPALIVE_INTERVAL_SECONDS`, `TCP_KEEPALIVE_RETRIES`; `HANDSHAKE_TIMEOUT_SECONDS` mặc định 0 (tắt ép đóng, dùng TCP keepalive).
- **Network**: Cờ `peer_disconnected`: khi client đóng kết nối (EOF, ConnectionReset, lỗi ghi) thì handshake timeout không gửi TERMINATE_RESP nữa (tránh gửi vào socket đã đóng).
- **Utils**: Hàm `fe_status_detail(status)` trả về chuỗi mô tả mã lỗi FE/BECT cho log (trace failed transaction); dùng trong checkin/commit BECT, VDTC, VETC.
- **Checkin VETC**: Trả về `fe_resp.station` / `fe_resp.lane` (trạm/làn vào) cho checkout; rating_detail dùng `price_id`/`stage_id` by value (Copy).
- **Cache**: Refactor cursor: dùng `if let Some(mut cursor)` trong toll_cache, price_cache, toll_lane_cache, toll_stage_cache, db_retry.
- **BOO2 client**: Restructure nội bộ.
- **Connection**: Truyền `peer_disconnected` (Arc<AtomicBool>) vào `handle_connection`; đơn giản hóa điều kiện gọi `process_message_buffer`.
- **main**: Thêm một số clippy allows (too_many_arguments, field_reassign_with_default, v.v.).
- **env.template**: Bổ sung mô tả VDTC multi-host, VDTC_REQUEST_TIMEOUT_SECS, TCP keepalive, handshake timeout.

## 0.2.2 (2026-03-01)

- **Ticket ID**: Refactor sang mô hình prefix + suffix từ DB sequence. **TicketIdService** (singleton): lấy sequence một lần làm prefix, counter 1..suffix_max (ENV `TICKET_ID_SUFFIX_MAX`, mặc định 999); khi suffix đạt `TICKET_ID_PREFETCH_THRESHOLD` (mặc định 980) thì prefetch sequence tiếp theo; id = prefix * suffix_multiplier + suffix; không kiểm tra trùng khi dùng id từ block. **Utils ticket_id**: đơn giản hóa, chỉ còn fallback `generate_ticket_id_async()` khi DB/block không dùng được (timestamp-based). **Constants**: thêm `ticket_id::suffix_max()`, `suffix_multiplier()`, `prefetch_threshold()`, Lazy ENV `TICKET_ID_SUFFIX_MAX`, `TICKET_ID_PREFETCH_THRESHOLD`; retry chỉ khi ticket_id = 0 (bỏ duplicate check trong loop).
- **db_retry**: Refactor hàng đợi: session FIFO; conn_server/user theo logical key (ip, username:toll_id) — ghi đè bản mới nhất; buffer in-memory khi KeyDB ghi thất bại, task retry flush lên KeyDB khi kết nối lại; dùng mediation_pool_holder.
- **Cache**: Cập nhật cache_prefix, keydb, connection_data_cache, cache_reload và các data cache (price, subscription_history, toll, toll_lane, toll_stage, toll_fee_list, dto) theo logic load/prefix hiện tại.
- **Toll fee**: Refactor helpers, flow, segment, direct_price, detail, types, service (logic tính phí giữ nguyên).
- **main**: Điều chỉnh luồng khởi động (cache, db_retry, connection).
- **DB/Handlers/Services**: Điều chỉnh tcoc_users, transport_transaction_stage repository, connect handler, tcoc_connection_server_service, tcoc_session_service, tcoc_user_service, boo_transport_trans_stage_service, transport_transaction_stage_service, route_graph.
- **env.template**: Thêm TICKET_ID_SUFFIX_MAX, TICKET_ID_PREFETCH_THRESHOLD; cập nhật CACHE_RELOAD_INTERVAL_SECONDS, KEYDB_URL.

## 0.2.1 (2026-02-28)

- **Cache**: Thêm connection_data_cache (TCOC_CONNECTION_SERVER, TCOC_USER từ MEDIATION), db_retry task, wb_route_cache, ex_list_cache, ex_price_list_cache; cập nhật config/reload và các data cache hiện có.
- **DB**: Thêm repositories tcoc_connection_server, tcoc_sessions, tcoc_users.
- **Handlers**: Thêm connect; cập nhật checkin (bect, vdtc, vetc).
- **Services**: Thêm tcoc_connection_server_service, tcoc_session_service, tcoc_user_service; mở rộng toll_fee (flow, helpers, segment).
- **Utils**: Thêm epoch_datetime (epoch sec/ms → DateTime).
- **main**: Nối cache (connection, db_retry), DB và handlers; load connection cache khi startup, task db_retry và DB init retry MEDIATION.

## 0.2.0 (2026-02-26)

- **Repos**: Thêm `find_latest_by_etag` vào transport_trans_stage_sync; thống nhất BOO/transport dùng `find_latest_by_etag`, bỏ find_latest_by_etag_without_checkout và find_*_with*_without_commit.
- **Tài liệu**: Mục tài liệu theo module trong FLOW_DOCUMENTATION; cập nhật timestamp.
- **Tài liệu**: Đồng bộ luồng khởi động (connection cache, db_retry, DB init retry MEDIATION); mục Cache (connection_data_cache, db_retry, wb_route, ex_list, ex_price_list).
- **Checkout**: Lấy bản ghi checkIn mới nhất từ cache + DB (sync + bảng chính); trả lỗi nếu etag đã checkout.
- **TCP**: Timeout đọc socket (SOCKET_READ_TIMEOUT_SECONDS) và timeout kết nối BOO (TCP_CONNECT_TIMEOUT_VDTC/VETC).
- **BOO client**: Sửa retry khi app dừng; sửa reconnect.
- **Toll fee / cache**: Mở rộng flow, direct_price, segment, helpers; ex_list_dto, ex_price_list_dto, toll_fee_list_cache, ETDR cache; checkin BECT cập nhật.
- **VDTC**: Sửa ticket_in_id checkout, sửa checkOut/commit.
- **VETC**: Sửa amount commit.
- **BECT**: Kiểm tra số dư tối thiểu (min balance).
- Cập nhật tài liệu theo logic hiện tại.

## 0.1.19 (2026-02-25)

- **VETC**: Gửi đúng tính phí (price calculation); dùng COMMIT 154/155 khi có hub_id cho checkOut.
- **Fix**: TRANSITION_CLOSE; lưu dữ liệu đúng vào TRANSPORT_TRANSACTION_STAGE.
- **Kafka**: Trường status trong message là i32.
- **Logic**: Cập nhật lưu ticket_in_id/ticket_out_id, ticket.
- **VDTC**: Thêm logging cho commit (refactor log file).
- Cập nhật tài liệu và phiên bản ứng dụng.

## 0.1.18 (2026-02-24)

- **IP denylist**: Env **IP_DENYLIST** (pattern phân cách dấu phẩy: 10.10.10.10, 10.10.*, 10.*). **CompiledIpDenylist** (utils/ip_denylist.rs) biên dịch một lần, từ chối kết nối ngay nếu IP khớp, không ghi log.
- **IP block**: Env **IP_BLOCK_ENABLED**, **IP_BLOCK_FAILURE_THRESHOLD** (mặc định 5), **IP_BLOCK_DURATION_SECONDS** (mặc định 900 = 15 phút). **IpBlockService**: bộ đếm lỗi in-memory, trạng thái block chỉ Redis/KeyDB; handshake timeout cũng tính lỗi; Redis lỗi → fail-open.
- **Cache load**: Thứ tự ưu tiên DB, khi KeyDB kết nối thì DB lỗi mới fallback KeyDB; reload định kỳ chỉ từ DB.
- **Log**: Thêm conn_id vào log (new connection, connection closed).
- **DB lịch sử**: BOO_TRANS_STAGE_TCD_HIS, BOO_TRANSPORT_TRANS_STAGE_H (repositories + ghi sau save/update).
- **Constants**: commit_retry (MAX_RETRIES, INITIAL_RETRY_DELAY_MS), verify_after_save_service (MAX_RETRIES) trong constants.rs.
- **Commit handlers**: VETC/VDTC/BECT dùng get_boo_record_with_retry() từ commit/common.
- **Checkin handlers**: Verify sau save chuyển vào service (verify_after_save_service), handler chỉ gọi get_by_id một lần.
- **Kafka**: Sửa lỗi publish; cập nhật logic gửi và get nearly checkin by etag.
- **KeyDB**: Kiểm tra và bypass khi không kết nối; log luồng tính phí.

## 0.1.17 (2026-02-23)

- **Logging**: Thêm env **LOG_DEBUG_FILE** (1, true, yes): khi bật thì tạo file `*-debug-*.log` và ghi mọi log mức debug trở lên; mặc định tắt.
- **file_logger**: File debug và registry level chỉ dùng khi enable_debug_file; rotation/cleanup áp dụng file debug khi bật.
- **KeyDB**: Bỏ log debug khi Redis set_ex success.
- **Constants**: Thêm MAX_DEBUG_HEX_BYTES.
- **BOO1/BOO2**: Log debug (raw, hex) chỉ ghi khi tracing Level::DEBUG; message fe_request_id đổi sang tiếng Anh (collision in pending).
- Tài liệu: cập nhật Logging, LOG_DEBUG_FILE, changelog.

## 0.1.16 (2026-02-21)

- **Refactor cấu trúc handlers và models/services**: CHECKIN: VETC/VDTC tách thành module con `checkin/vetc/` (handler.rs, boo_stage.rs, messages.rs), `checkin/vdtc/` (tương tự); BECT giữ file đơn `checkin/bect.rs`.
- COMMIT: VETC/VDTC/BECT tách thành `commit/vetc/`, `commit/vdtc/`, `commit/bect/` (mỗi thư mục có handler.rs, vetc/vdtc có thêm messages.rs).
- ROLLBACK: giữ file đơn vetc.rs, vdtc.rs, bect.rs.
- Models: ETDR từ file đơn thành thư mục `ETDR/` (mod.rs, types_impl, cache, sync).
- Services: kafka_producer_service từ file đơn thành thư mục `kafka_producer_service/` (mod.rs, service.rs, types.rs).
- Tài liệu FLOW_DOCUMENTATION cập nhật cấu trúc checkin/commit/rollback, models, services và cây thư mục.

## 0.1.15 (2026-02-20)

- **Toll fee refactor**: Tách `toll_fee_service` thành module `services/toll_fee/` (error, types, helpers, detail, segment, direct_price, flow, service).
- Logic tính phí không đổi: TollFeeService::calculate_toll_fee, RouteGraph, direct price, segment loop, no route; handlers (VETC/VDTC/BECT) và toll_fee_list_cache dùng `crate::services::toll_fee::TollFeeService` và `TollFeeListContext`.
- Tài liệu: FLOW_DOCUMENTATION cập nhật mục Services và cấu trúc thư mục.

## 0.1.14 (2026-02-14)

- **Epoch millisecond**: Thống nhất thời gian epoch theo **millisecond** (timestamp, trans_datetime, commit_datetime trong VETC/VDTC; ETDR các trường datetime; log cleanup cutoff/modified).
- **Utils epoch_to_datetime_utc**: Hàm `epoch_to_datetime_utc(epoch)` chuyển epoch sang `DateTime<Utc>`, **hỗ trợ cả giây và millisecond** (epoch >= 1e12 → ms, ngược lại → giây); dùng khi đọc dữ liệu từ DB/bản tin có thể định dạng cũ hoặc mới.
- **Ticket_id**: Format chính khi có KeyDB: 19 ký tự `'9' + yyyyMMdd(8) + ip_suffix(3) + counter(7)`; counter KeyDB key `ticket_<ip_suffix>`, persist định kỳ (TICKET_ID_PERSIST_INTERVAL_MS). Fallback khi chưa KeyDB: `timestamp_ms * 10000 + sequence`.
- Tài liệu: FLOW_DOCUMENTATION cập nhật Utils (epoch_datetime, ticket_id), mục datetime (epoch ms, epoch_to_datetime_utc).

## 0.1.13 (2026-02-13)

- **BECT ACCOUNT_TRANSACTION**: Ghi nhận giao dịch tài khoản (CRM_OWNER.ACCOUNT_TRANSACTION): insert lúc checkout (reserve, AUTOCOMMIT=2, AUTOCOMMIT_STATUS null), lưu ACCOUNT_TRANS_ID vào TRANSPORT_TRANSACTION_STAGE; commit (out) cập nhật AUTOCOMMIT_STATUS (1=COMMIT, 2=ROLLBACK), FINISH_DATETIME, NEW_BALANCE/NEW_AVAILABLE_BAL (allow_commit_or_rollback).
- **BECT bypass**: Env `BECT_SKIP_ACCOUNT_CHECK`, `BECT_SKIP_BALANCE_CHECK` (1/true/yes) để bỏ qua kiểm tra subscriber/account và available_balance >= trans_amount (checkout và commit).
- **Kafka producer**: Partition từ cache (get_partition_count_cached); config `KAFKA_TOPIC_TRANS_PARTITION_COUNT` (không gọi list_topics), `KAFKA_TOPIC_TRANS_FIXED_PARTITION` (gửi cố định một partition); batch gửi theo từng partition (all partitions available).

## 0.1.12 (2026-02-13)

- **Quy tắc viết tài liệu**: Thêm rule viết tài liệu (chuẩn markdownlint, dễ hiểu, đủ thông tin) trong `project.mdc` và rule `doc-writing.mdc` (globs `**/*.md`).
- Thêm cấu hình `.markdownlint.json`.
- Chỉnh `FLOW_DOCUMENTATION.md` theo markdownlint (blanks around headings/lists/fences, fenced code language, trailing spaces).

## 0.1.11 (2025-02-13)

- **Datetime (checkin/checkout)**: Tài liệu hóa và rà soát checkin_datetime, checkin_commit_datetime, checkout_datetime, checkout_commit_datetime; logic áp dụng thống nhất BECT/VETC/VDTC; BECT Kafka payload thêm fallback checkin_commit_datetime.

## 0.1.10 (2025-02-13)

- **BECT – Retry khi trừ tiền lỗi**: Tài liệu hóa: checkout (CHECKIN out) không cho phép thực hiện lại cho cùng etag; commit (COMMIT out) cho phép gửi lại để thử trừ tiền (retry) cho đến khi SUCC hoặc xử lý khác.

## 0.1.9 (2025-02-13)

- **Tài liệu (double-check logic/code)**: Cache khởi động: TOLL_CYCLE → TOLL_STAGE (xây RouteGraph từ TOLL_STAGE); reload cùng bộ cache.
- Kafka: Producer gửi checkin/checkout lên topic_trans_hub_online; Consumer CHECKIN_HUB_INFO (get_valid_partitions, KAFKA_CONSUMER_OFFSET_FILE, cập nhật hub_id vào BOO_TRANSPORT_TRANS_STAGE / TRANSPORT_TRANSACTION_STAGE).
- Cache data: toll_stage_cache, toll_stage_dto, DTO (subscription_history, wb_list_route, ex_list, ex_price_list); route_graph từ TOLL_STAGE (BFS find_route), toll_fee_service + WB/Ex; kafka_consumer_service.
- Env: KAFKA_TOPIC_CHECKIN_PARTITION_COUNT, KAFKA_CONSUMER_OFFSET_FILE. session_map trong TCP server.

## 0.1.8 (2025-02-13)

- **Tài liệu**: Rà soát và cập nhật logic FLOW_DOCUMENTATION.md: CONNECT_RESP session_id từ sequence (fallback conn_id); luồng FE chi tiết (tx_client_requests → Logic Handler → tx_logic_replies, reply từ rx_socket_write); Logic Handler chỉ forward reply, business logic trong process_request/handle_connection; ROAMING_REQUEST/ROAMING_RESPONSE chỉ lưu khi VETC/VDTC (không BECT); Kafka producer dùng một topic KAFKA_TOPIC_TRANS_HUB_ONLINE (checkin+checkout), consumer KAFKA_TOPIC_CHECKIN_HUB_ONLINE; sơ đồ kiến trúc và fallback text đồng bộ.

## 0.1.7 (2025-02-12)

- **Refactoring**: Tách `generate_ticket_id()` từ `constants.rs` sang `utils/ticket_id.rs` để tổ chức code tốt hơn.
- Thêm unit tests cho ticket_id generator.
- Giữ backward compatibility: re-export từ `constants::generate_ticket_id()` vẫn hoạt động.

## 0.1.6 (2025-02-12)

- **FE Protocol**: Thêm ticket_id vào FE_COMMIT_IN_RESP và FE_ROLLBACK_RESP (4 hoặc 8 bytes theo FeIdWidth) để FE đối chiếu với request.
- **ETDR**: Đổi ticket_id từ i32 sang i64 để đồng bộ với FE protocol và transport_trans_id.
- **Ticket ID Generation**: Thêm retry logic và duplicate check cho get_ticket_id_for_boo_checkin() và get_ticket_id_for_bect_checkin(); đa luồng an toàn (DB NEXTVAL atomic, fallback AtomicI64); kiểm tra duplicate trong ETDR cache và DB; retry tối đa 2 lần nếu ticket_id = 0 hoặc duplicate; fallback cuối cùng dùng generate_ticket_id().

## 0.1.5 (2025-02-12)

- Cập nhật tài liệu theo logic hiện tại: encryption_key và toll_id từ TCOC_CONNECTION_SERVER theo IP khi accept connection; validate toll_id (Error 301 nếu không trong list); FE protocol fe_id_width (i32/i64) detect và serialize; TCOC_REQUEST, ROAMING_REQUEST/ROAMING_RESPONSE (direction từ lane_type); CONNECT session_id từ sequence, TCOC_SESSION, SessionUpdate; CHECKIN/COMMIT/ROLLBACK phân loại eTag VETC/VDTC/BECT (BECT không gọi BOO); ETDR retry/cleanup tasks, Kafka consumer (CHECKIN_HUB_INFO); cấu trúc DB (TcocConnectionServer, RoamingRequest/Response).

## 0.1.4 (2025-02-11)

- Cập nhật tài liệu: chi tiết logic BOO1/BOO2 client (connection state management với double-check, session ID cache lock-free, pending requests với timeout/cleanup, reconnection với handshake timeout threshold, error handling chi tiết).

## 0.1.3 (2025-02-10)

- Rà soát Kafka: tối ưu worker_loop (tái sử dụng Vec checkin/checkout records), timeout batch theo số record (base + 30ms/record, max 2x base), cập nhật comment worker/luồng.
- Tài liệu: luồng Kafka chi tiết, graceful shutdown (queue không drain).

## 0.1.2 (2025-02-10)

- Thêm Kafka: config (kafka.rs), KafkaProducerService (queue + worker, retry/backoff), models hub_events (CheckinEvent, CheckoutEvent).
- Cập nhật tài liệu: luồng khởi động (logging, DB, cache, Kafka), cấu trúc configs/models/services, biến môi trường Kafka.

## 0.1.1 (2025-02-08)

- Cập nhật tài liệu: cấu trúc dự án (cache, logging, db, services), luồng reply, handlers theo thư mục (checkin/commit/rollback), cấu hình KEYDB/CACHE_RELOAD/DB.
- Thêm lịch sử phiên bản.

## 0.1.0 (phiên bản ban đầu)

- TCP server ETC middleware, kết nối VETC/VDTC, CONNECT/HANDSHAKE/CHECKIN/COMMIT/ROLLBACK/TERMINATE, AES-CBC, cache (KeyDB/Moka), DB repositories, logging.
