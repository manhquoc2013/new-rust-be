# Rà soát luồng cache và tối ưu hiệu năng

Tài liệu rà soát toàn bộ luồng xử lý cache trong dự án và đề xuất tối ưu hiệu năng.

## 1. Tổng quan kiến trúc cache

| Thành phần | Vai trò | Vị trí |
|------------|---------|--------|
| **CacheManager** | Gộp Moka + KeyDB; get/set/atomic_reload_prefix, load_prefix_from_keydb | `cache/config/cache_manager.rs` |
| **MokaCache** | Cache in-process (moka::future::Cache), key string → JSON | `cache/config/moka_cache.rs` |
| **KeyDB** | Redis-compatible, async get/set/MGET/set_many, SCAN, list | `cache/config/keydb.rs` |
| **TollCache / TollLaneCache** | TOLL, TOLL_LANE in-memory (HashMap + TTL + LRU), global OnceLock | `models/TollCache.rs` |
| **ETDRCache** | ETDR in-memory (per-etag lock + HashMap), sync KeyDB | `models/ETDR/cache.rs` |
| **Connection data** | ConnectionServer/User: memory (Mutex<HashMap>) + CacheManager | `cache/data/connection_data_cache.rs` |
| **Price/Subscription/RouteGraph/TollFeeList** | Memory layer + CacheManager (atomic_reload) | `cache/data/*_cache.rs` |
| **SimpleMemoryCache / PlaceholderDistributedCache** | Service layer (TCOC session, request, user, …) | `services/cache.rs` |
| **db_retry** | KeyDB + buffer in-memory khi ghi KeyDB lỗi; task retry ghi DB | `cache/data/db_retry.rs` |

Luồng đọc chung: **memory (Moka/HashMap/ETDR) → KeyDB → DB**. Luồng ghi reload: **DB → atomic_reload_prefix (Moka + KeyDB)** hoặc **set từng key (Moka + KeyDB)**.

---

## 2. Các vấn đề hiệu năng đã xác định

### 2.1 KeyDB: mọi thao tác dùng `write()` → serial hóa toàn bộ

**Vị trí:** `cache/config/keydb.rs`

- `get()`, `set()`, `exists()`, `get_many()`, `get_keys_with_prefix()` đều lấy `self.conn.write().await`.
- Redis/KeyDB cho phép nhiều read song song; việc dùng exclusive lock cho cả get khiến mọi request get/set đều xếp hàng.

**Đề xuất:** Hiện `redis::AsyncCommands` yêu cầu `&mut self` cho get/exists/… nên với **một** connection không thể dùng `RwLock::read()` cho get (chỉ có `write()` mới cho `&mut conn`). Hai hướng: (1) **Connection pool**: nhiều `MultiplexedConnection`, mỗi request lấy một conn từ pool → nhiều get song song; (2) Giữ nguyên và theo dõi latency; nếu KeyDB không phải nút thắt thì có thể hoãn.

---

### 2.2 MokaCache: không giới hạn dung lượng và TTL

**Vị trí:** `cache/config/moka_cache.rs`

- `Cache::builder()` không gọi `.max_capacity()` hay `.time_to_live()` → cache tăng không giới hạn, không evict theo thời gian.
- Rủi ro: bộ nhớ tăng dần (đặc biệt với nhiều prefix: toll, price, connection, …).

**Đề xuất:** Thêm `max_capacity` (ví dụ 50_000–100_000 entry) và `time_to_live` (ví dụ 1–2h) theo env, để vừa hâm nóng tốt vừa tránh OOM.

---

### 2.3 MokaCache: `invalidate_prefix` và `get_keys_with_prefix` O(n)

**Vị trí:** `cache/config/moka_cache.rs`

- `invalidate_prefix` và `get_keys_with_prefix` đều `self.cache.iter()` rồi lọc theo prefix → duyệt toàn bộ entry, O(n).
- `atomic_reload_prefix` trong CacheManager gọi `get_keys_with_prefix` (Moka + KeyDB) rồi invalidate từng key Moka → khi số key lớn, reload tốn thời gian và giữ lock lâu.

**Đề xuất:** (1) Giới hạn số key xóa mỗi lần (cap) để tránh spike; (2) Nếu có thể, cân nhắc cấu trúc theo prefix (ví dụ namespace riêng) để không phải iter toàn bộ; (3) Thêm metric/log thời gian reload để theo dõi.

---

### 2.4 atomic_reload_prefix: ghi Moka từng key, nhiều await

**Vị trí:** `cache/config/cache_manager.rs` — `atomic_reload_prefix`

- Vòng `for (key, value) in &new_data { self.moka.set(key, value).await; }` → N lần await, không batch.
- KeyDB đã dùng `set_many` (pipeline) là tốt; Moka vẫn từng key.

**Đề xuất:** Nếu Moka API hỗ trợ insert nhiều cặp (batch), dùng batch thay vì từng key. Nếu không, giữ như hiện tại nhưng theo dõi số key mỗi prefix; với prefix có hàng chục nghìn key có thể cần tách nhỏ hoặc lazy load.

---

### 2.5 Load fallback KeyDB: ghi Moka từng key trong vòng lặp

**Vị trí:** `cache/data/connection_data_cache.rs`, `cache/data/toll_cache.rs`, `cache/data/price_cache.rs`, …

- Khi load từ KeyDB (DB lỗi): `for (key, ref dto) in &keydb_loaded { cache.moka().set(key, dto).await; }` → N round-trip (await) vào Moka.
- Sau đó còn ghi `server_memory()`/`user_memory()` hoặc `replace_all_*` — phần này đã gom một lần.

**Đề xuất:** Gom thành `Vec<(String, T)>` giống `new_data` rồi gọi một lần `atomic_reload_prefix(prefix, keydb_loaded)` (hoặc helper chỉ ghi Moka nếu không cần xóa stale). Như vậy KeyDB đã load xong, chỉ cần một lần ghi Moka thay vì N lần set rời.

---

### 2.6 ETDR cache: cập nhật last_access trên mỗi lần đọc

**Vị trí:** `models/ETDR/cache.rs` — `get_latest_checkin`, `get_latest_checkin_without_checkout`

- Mỗi lần hit: `get_mut(etag)` → `entry.last_access = now; entry.access_count += 1` → ghi vào entry trong HashMap.
- Cùng với `get_etag_lock` (Mutex toàn cục cho map etag_locks), đọc nhiều etag có thể tranh lock; mỗi read lại thành write (cache line contention).

**Đề xuất:** (1) Cập nhật last_access/access_count không bắt buộc cho mọi read — có thể lấy mẫu (ví dụ 1/10) hoặc chỉ cập nhật khi cần LRU thật sự; (2) Hoặc dùng atomic (AtomicU64) cho last_access/access_count để tránh phải get_mut; (3) Giảm thời gian giữ `data.lock()`: đọc nhanh rồi trả về clone, cập nhật thống kê ngoài critical path nếu có thể.

---

### 2.7 TollCache / TollLaneCache: cùng mô hình read → write

**Vị trí:** `models/TollCache.rs`

- `get()`, `get_by_code()`, `get_by_toll_id()`: giữ `data.lock()` và `stats.lock()`, dùng `get_mut` để cập nhật last_access/access_count trên mỗi hit.
- Tương tự ETDR: mọi read đều biến thành write trong map, dễ tranh lock khi throughput cao.

**Đề xuất:** Giống ETDR: tránh get_mut cho mỗi read; dùng atomic cho thống kê hoặc cập nhật last_access không đồng bộ / lấy mẫu.

---

### 2.8 get_price: nhiều lookup và tạo key lặp lại

**Vị trí:** `cache/data/price_cache.rs` — `get_price`

- Tối đa 4 lookup: memory (key), memory (key_reverse), cache.get(key), cache.get(key_reverse), rồi có thể 2 lần query DB.
- `gen_key` gọi 2 lần với (toll_a, toll_b) và (toll_b, toll_a). Chuỗi key có thể được tạo một lần và tái sử dụng.

**Đề xuất:** Tạo sẵn `price_key` và `price_key_reverse` một lần; gọi `get_price_from_memory` / `cache.get` với hai key đó, tránh lặp logic gen_key.

---

### 2.9 SimpleMemoryCache: một RwLock cho toàn bộ HashMap

**Vị trí:** `services/cache.rs`

- TCOC session/request/user/response dùng `SimpleMemoryCache`: một `RwLock<HashMap<...>>` cho mọi key.
- Read (get) và write (set/remove) tranh nhau; throughput cao sẽ nghẽn tại lock này.

**Đề xuất:** (1) Dùng dashmap hoặc cache có sharding (nhiều bucket) để giảm contention; (2) Hoặc giữ design hiện tại nhưng đảm bảo TTL và giới hạn kích thước để tránh map quá lớn kéo dài thời gian giữ lock.

---

### 2.10 Connection data: hai tầng memory + CacheManager

**Vị trí:** `cache/data/connection_data_cache.rs`

- `get_connection_server_by_ip`: (1) `server_memory().lock()` → get; (2) nếu miss → `cache.get::<TcocConnectionServer>(&key).await` (Moka → KeyDB, và KeyDB đang dùng write lock); (3) nếu hit KeyDB thì `server_memory().lock()` lại để insert.
- Đã đúng thứ tự ưu tiên memory → CacheManager. Điểm nghẽn chính là KeyDB get dùng write lock (xem 2.1).

---

### 2.11 db_retry: flush buffer lên KeyDB từng key

**Vị trí:** `cache/data/db_retry.rs` — `run_db_retry_task`

- Khi KeyDB lại kết nối: flush session/conn_server/user từ buffer lên KeyDB bằng nhiều lần `k.set_result(&key, ...)`, mỗi key một round-trip.
- KeyDB đã có `set_many`; có thể gom theo batch (ví dụ 50–100 key) rồi gọi set_many để giảm round-trip.

**Đề xuất:** Gom entries cùng loại thành batch (ví dụ 50 session, 50 conn_server, 50 user), gọi `keydb.set_many()` (cần overload hoặc helper nhận Vec (key, value)) thay vì từng set_result. Chú ý KeyDB hiện set_many nhận `(String, T)`; có thể serialize sẵn rồi gom batch.

---

### 2.12 Cache reload: chạy song song nhưng không giới hạn tải

**Vị trí:** `cache/data/cache_reload.rs`

- `reload_all_cache` dùng `tokio::join!` cho 6 cache RATING và 2 connection; đã tránh reload trùng nhờ `RELOAD_IN_PROGRESS`.
- Nếu DB chậm hoặc số bản ghi lớn, một lần reload có thể tạo nhiều connection/query đồng thời.

**Đề xuất:** Giữ nguyên; nếu cần có thể thêm semaphore giới hạn số cache reload đồng thời hoặc giới hạn tần suất reload khi DB lỗi liên tục.

---

## 3. Thứ tự ưu tiên tối ưu

| Ưu tiên | Mục | Tác động | Effort |
|--------|-----|----------|--------|
| 1 | KeyDB: connection pool (nhiều connection) để get song song; hoặc giữ 1 conn nếu chưa đo được nút thắt | Cao nếu KeyDB là nút thắt | Trung bình (pool) |
| 2 | Moka: max_capacity + time_to_live | Trung bình — tránh OOM, evict cũ | Thấp |
| 3 | Fallback load KeyDB → gom một lần atomic_reload (hoặc batch set Moka) | Trung bình — giảm N await khi startup/reload | Thấp |
| 4 | db_retry flush buffer: batch set_many lên KeyDB | Trung bình — giảm round-trip khi KeyDB lại online | Trung bình |
| 5 | ETDR/TollCache: giảm cập nhật last_access trên mỗi read (atomic hoặc lấy mẫu) | Trung bình — giảm contention trên hot path | Trung bình |
| 6 | get_price: tạo key một lần, tái sử dụng | Nhỏ — code sạch hơn, ít allocation | Thấp |
| 7 | Moka invalidate_prefix / get_keys_with_prefix: cap hoặc cấu trúc theo prefix | Trung bình khi số key rất lớn | Cao |

---

## 4. Luồng cache đã đúng / nên giữ

- **TollCache / TollLaneCache**: `replace_all` xây map ngoài lock rồi gán một lần — tốt cho reader.
- **ETDR**: Per-etag lock (RwLock) cho phép nhiều etag xử lý song song; sync KeyDB qua `tokio::spawn` không block handler.
- **KeyDB**: Đã có set_many, get_many, remove_many, SCAN theo batch 1000 — tận dụng pipeline/batch.
- **Cache reload**: RELOAD_IN_PROGRESS tránh chạy trùng; RATING và connection tách pool đúng.
- **get_toll_with_fallback**: Thứ tự memory → CacheManager → DB, và populate lại memory khi hit Moka/KeyDB hoặc DB — đúng cache-aside.
- **Price**: Lớp memory (PRICE_MEMORY) trước, rồi CacheManager, rồi DB — hợp lý; chỉ cần tối ưu gen_key và số lần gọi (mục 2.8).

---

## 5. Rà soát sau tối ưu (logic + hiệu năng)

Sau khi triển khai các đề xuất (Moka env + cap, get_price key reuse, atomic_reload_prefix fallback, db_retry batch flush, ETDR/TollCache atomics), rà soát lại đảm bảo logic đúng, đủ, không ảnh hưởng xấu hiệu năng.

### 5.1 Logic đã kiểm tra

| Thành phần | Kiểm tra | Kết quả |
|------------|----------|--------|
| **TollCache / TollLaneCache** | `get`/`get_by_code`/`get_all`/`get_by_toll_id`: cập nhật `last_access`/`access_count` qua atomic; `evict_if_needed` sort theo `last_access.load(Ordering::Relaxed)`; `replace_all`/`put` khởi tạo entry với `AtomicI64`/`AtomicU64` | Đúng. |
| **TollLaneCache index** | Khi xóa entry hết hạn trong `get_by_toll_id` (remove khỏi `data`) phải đồng bộ index `toll_lanes` (toll_id → Vec\<lane_id\>) | **Đã sửa**: khi `data.remove(lane_id)` lấy được entry thì gọi `toll_lanes.get_mut(entry.lane.toll_id)`, `retain` bỏ lane_id, remove toll_id nếu list rỗng. Tránh chỉ mục lỗi thời, tích lũy lane_id “ma”. |
| **ETDRCache** | Read path dùng `data.get()` + atomic store/fetch_add; không dùng `get_mut` trên hot path | Đúng. |
| **db_retry flush** | Khi KeyDB connected: flush buffer theo batch (FLUSH_BATCH_SIZE=50); chunk lỗi thì đẩy cả chunk trở lại buffer (sessions/conn_servers/users); điều kiện insert buffer conn_servers/users: dưới cap hoặc key đã tồn tại | Đúng. |
| **MokaCache** | `invalidate_prefix` gọi `invalidate_prefix_capped(…).await` (đã sửa thiếu .await) | Đúng. |
| **Fallback KeyDB** | Các module load từ KeyDB dùng `atomic_reload_prefix(prefix, keydb_loaded)` + cập nhật memory/replace_all tương ứng; không lặp moka.set từng key | Đúng. |
| **closed_cycle_transition_stage_cache** | Fallback KeyDB: segment/cycle data loaded from DB or KeyDB; table CLOSED_CYCLE_TRANSITION_STAGE. | ClosedCycleTransitionStage prefix. |

### 5.2 Hiệu năng

- **Atomics (ETDR, TollCache, TollLaneCache)**: Đọc chỉ cần `data.get()` / `data.values()` không cần `get_mut` → giảm contention trên Mutex, không kéo dài thời gian giữ lock.
- **db_retry**: Flush batch 50 key/lần giảm round-trip KeyDB; khi lỗi cả chunk quay lại buffer, không mất dữ liệu.
- **TollLaneCache get_by_toll_id**: Đồng bộ index khi xóa expired chỉ thêm O(số expired) thao tác trong cùng lock, không tăng độ phức tạp tổng thể.

### 5.3 Cần theo dõi

- KeyDB: nếu latency get/set cao khi tải lớn, cân nhắc connection pool (env KEYDB_POOL_SIZE).
- Moka: `atomic_reload_prefix` vẫn ghi Moka từng key trong vòng for; nếu prefix có hàng chục nghìn key có thể đo thời gian reload và cân nhắc batch nếu Moka hỗ trợ.

---

## 6. Kết luận

- Luồng cache nhất quán: memory → KeyDB (hoặc Moka+KeyDB) → DB; reload và fallback KeyDB đã có.
- Cải thiện hiệu năng quan trọng nhất (khi KeyDB là nút thắt): **KeyDB connection pool** để nhiều get chạy song song; với một connection, redis crate yêu cầu `&mut` nên không dùng được read lock cho get.
- Tiếp theo: giới hạn Moka (max_capacity, TTL), gom ghi khi load từ KeyDB fallback, batch flush db_retry lên KeyDB, giảm write-on-read trong ETDR/TollCache. Sau rà soát: đã bổ sung đồng bộ index TollLaneCache khi xóa expired trong get_by_toll_id (mục 5.1).

Tài liệu này có thể cập nhật khi có thay đổi kiến trúc cache hoặc thêm metric đo tải.
