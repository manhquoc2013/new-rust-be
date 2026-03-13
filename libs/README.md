# Altibase ODBC libraries (optional)

Thư mục này chứa driver ODBC Altibase để kết nối Altibase từ ứng dụng.

**Để build Docker image có hỗ trợ Altibase**, đặt các file sau vào đây (từ Altibase client package):

- `libaltibase_odbc-64bit-ul64.so`
- `libodbccli_sl.so`
- `libalticapi_sl.so`

Nếu **không** có các file trên, Docker build vẫn thành công nhưng image sẽ không có driver Altibase (phù hợp dev/CI trên Mac hoặc môi trường không dùng Altibase).
