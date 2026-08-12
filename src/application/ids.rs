//! UUIDv4 生成器 —— application 层唯一允许使用 `std::time` 的位置。
//!
//! memora 不引入 `uuid` crate：使用基于 (nanos, pid, atomic counter) 的
//! 自写实现，结果落在合法 UUIDv4 字节布局内（version=4、variant=10xx）。
//! 重复概率：同一纳秒内不同进程 × 同一 counter → 2^64 / (2^32 counter) ≈ 0，
//! 跨重启相同纳秒内的理论冲突也通过 pid 区分。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// 生成一个新的 UUIDv4 字符串。
pub fn uuid_v4() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    // 取纳秒时间戳的低 64 位。
    let nanos = now.as_nanos() as u64;
    let pid = std::process::id() as u64;
    // wrap-around 也无所谓 —— 唯一性由 (nanos, pid) 共同保证。
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);

    let mut bytes = [0u8; 16];
    bytes[0..8].copy_from_slice(&nanos.to_le_bytes());
    bytes[8..12].copy_from_slice(&pid.to_le_bytes()[..4]);
    bytes[12..14].copy_from_slice(&(counter as u16).to_le_bytes());
    // 顶部固定 version=4 + variant=10xx，落在 UUIDv4 RFC 4122 字节布局内。
    // UUID 字符串第 3 段 = bytes[6..8]，第 4 段首字符 = bytes[8]。
    // version 占 byte 6 高 4 位，variant 占 byte 8 高 2 位。
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_v4_format_is_strict() {
        let id = uuid_v4();
        assert_eq!(id.len(), 36);
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
        // version=4 + variant=10xx。
        assert_eq!(parts[2].as_bytes()[0], b'4');
        let variant_char = parts[3].as_bytes()[0];
        assert!(
            matches!(variant_char, b'8' | b'9' | b'a' | b'b'),
            "variant nibble must be 8/9/a/b, got {variant_char:?}"
        );
    }

    #[test]
    fn uuid_v4_is_unique_across_calls() {
        // 自增 counter + 时间戳保证两次调用得到不同 id。
        let a = uuid_v4();
        let b = uuid_v4();
        assert_ne!(a, b);
    }
}
