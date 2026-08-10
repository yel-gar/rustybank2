use crate::db::Money;
use std::time::Duration;

pub const USERNAME_MAX_LENGTH: usize = 30;
pub const MAX_IDLE_TIMEOUT: Duration = Duration::from_secs(10);
pub const MAX_BI_STREAMS: u32 = 50;

pub const MONEY_PER_WORK: Money = 10;
pub const WORK_INTERVAL: Duration = Duration::from_secs(10);
