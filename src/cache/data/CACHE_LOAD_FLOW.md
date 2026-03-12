# Luồng load cache (DB + KeyDB)

## 1. Khi ứng dụng khởi động (main.rs)

- **KeyDB không kết nối** (`load_from_keydb = false`): Chỉ lấy từ DB. DB lỗi → log lỗi, cache trống (không fallback).
- **KeyDB kết nối** (`load_from_keydb = true`):
  1. **Ưu tiên DB**: Gọi `get_*_cache(db_pool, cache, true)` → thử load từ DB trước.
  2. **DB thành công**: Dùng `atomic_reload_prefix()` hoặc `cache.set()` → ghi vào **Moka + KeyDB**. Log `"loaded from DB, synced to KeyDB"`.
  3. **DB thất bại**: Fallback đọc từ KeyDB (`load_prefix_from_keydb`), ghi vào **chỉ Moka** (`cache.moka().set()`), **không ghi ngược KeyDB** (tránh ghi đè dữ liệu KeyDB khi nguồn là bản cũ từ KeyDB).

## 2. Reload cache định kỳ (cache_reload.rs)

- Gọi `get_*_cache(..., load_from_keydb = true)` — khi DB lỗi hoặc không có pool thì fallback đọc từ KeyDB để hệ thống không gián đoạn khi DB mất.
- **RATING cache** (toll, lane, price, stage, toll_fee_list, subscription_history): reload khi rating_pool_holder có pool; DB thành công → ghi Moka + KeyDB; DB lỗi/không pool → fallback KeyDB, ghi Moka.
- **Connection cache** (connection server, connection user): reload **chỉ khi** mediation_pool_holder có pool; cùng quy tắc DB/KeyDB như trên.
- KeyDB không kết nối: `keydb.set()` no-op (chỉ lock, không gửi Redis), không ảnh hưởng luồng.
- Có `RELOAD_IN_PROGRESS`: reload đang chạy thì bỏ qua chu kỳ tiếp theo.

## 3. Hiệu năng

- **Không thêm round-trip**: Khi DB thành công chỉ có 1 lần load DB + 1 lần ghi Moka + 1 lần ghi KeyDB (đã có sẵn từ trước).
- **Fallback**: Chỉ khi DB lỗi mới đọc KeyDB (SCAN + MGET theo prefix); không đọc KeyDB khi DB thành công.
- **KeyDB down**: Mọi `keydb.set()` chỉ kiểm tra `conn`, không gửi mạng; reload/startup vẫn chạy bình thường.
- **Reload**: Chạy trong task nền, có `RELOAD_IN_PROGRESS` tránh chạy trùng; reload chưa xong thì bỏ qua chu kỳ tiếp theo.

## 4. Bảng luồng tóm tắt

| Sự kiện        | Nguồn đọc | Ghi Moka | Ghi KeyDB |
|----------------|-----------|----------|-----------|
| Startup, DB OK | DB        | Có       | Có        |
| Startup, DB Err, KeyDB OK | KeyDB (fallback) | Có | Không |
| Startup, DB Err, KeyDB down | -   | Không (cache trống) | - |
| Reload, DB OK  | DB        | Có       | Có        |
| Reload, DB Err | -         | Giữ cũ   | Không đổi |
