//! Token-bucket 速率限制器
//!
//! 通过 `CARDNOTE_MAX_RPM` 环境变量配置每分钟最大请求数。
//! 未设置时不限流（向后兼容）。

use std::time::{Duration, Instant};

/// Token-bucket 速率限制器
#[derive(Debug)]
pub struct RateLimiter {
    /// 最大 token 数（= RPM）
    capacity: u32,
    /// token 补充速率（tokens/秒）
    rate: f64,
    /// 当前 token 数
    tokens: f64,
    /// 上次补充时间
    last_refill: Instant,
}

impl RateLimiter {
    /// 创建限流器
    /// - `rpm`: 每分钟最大请求数
    pub fn new(rpm: u32) -> Self {
        let rps = rpm as f64 / 60.0;
        Self {
            capacity: rpm.max(1),
            rate: rps,
            tokens: rpm as f64,
            last_refill: Instant::now(),
        }
    }

    /// 获取一个 token，如果当前速率已达上限则等待
    pub async fn acquire(&mut self) {
        loop {
            self.refill();
            if self.tokens >= 1.0 {
                self.tokens -= 1.0;
                return;
            }
            // 等待足够的 token 补充
            let needed = 1.0 - self.tokens;
            let wait_secs = needed / self.rate;
            tokio::time::sleep(Duration::from_secs_f64(wait_secs)).await;
        }
    }

    /// 根据时间流逝补充 token
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.rate).min(self.capacity as f64);
        self.last_refill = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_creation() {
        let limiter = RateLimiter::new(60);
        assert_eq!(limiter.capacity, 60);
        assert!((limiter.rate - 1.0).abs() < 0.01); // 60 RPM = 1 RPS
    }

    #[test]
    fn test_rate_limiter_capacity() {
        let limiter = RateLimiter::new(10);
        assert_eq!(limiter.capacity, 10);
        assert_eq!(limiter.tokens, 10.0);
    }

    #[test]
    fn test_rate_limiter_min_rpm() {
        let limiter = RateLimiter::new(0);
        assert_eq!(limiter.capacity, 1); // 至少为 1
    }
}
