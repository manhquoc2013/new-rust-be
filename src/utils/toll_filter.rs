//! Kiểm tra toll_id có nằm trong chuỗi allow-list từ DB (dạng "1000-1009,1010,1015-1020").

/// Kiểm tra toll_id có nằm trong chuỗi từ DB không. Chuỗi dạng "1000-1009,1010,1015-1020".
#[inline]
pub fn is_toll_id_valid(request_toll_id: i64, db_toll_id_str: &Option<String>) -> bool {
    let Some(ref toll_id_str) = db_toll_id_str else {
        return true;
    };

    if toll_id_str.trim().is_empty() {
        return true;
    }

    let parts: Vec<&str> = toll_id_str.split(',').map(|s| s.trim()).collect();

    for part in parts {
        if part.is_empty() {
            continue;
        }

        if part.contains('-') {
            let range_parts: Vec<&str> = part.split('-').map(|s| s.trim()).collect();
            if range_parts.len() == 2 {
                if let (Ok(start), Ok(end)) =
                    (range_parts[0].parse::<i64>(), range_parts[1].parse::<i64>())
                {
                    if request_toll_id >= start && request_toll_id <= end {
                        return true;
                    }
                }
            }
        } else if let Ok(value) = part.parse::<i64>() {
            if request_toll_id == value {
                return true;
            }
        }
    }
    false
}
