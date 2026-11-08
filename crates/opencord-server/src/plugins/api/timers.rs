// SPDX-License-Identifier: MPL-2.0

//! Timers API (plan Task 20, design 2026-11-07 §10.3).

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use mlua::{Function, Lua, Table};

use super::{PluginEnv, function, raise};
use crate::plugins::PluginEvent;

pub const TIMERS: &str = "__opencord_timers";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UtcDateTime {
    pub year: i32,
    pub month: u8,   // 1..=12
    pub day: u8,     // 1..=31
    pub hour: u8,    // 0..=23
    pub minute: u8,  // 0..=59
    pub second: u8,  // 0..=59
    pub weekday: u8, // 0..=6 (0 = Sunday, 1 = Monday, ..., 6 = Saturday)
}

impl UtcDateTime {
    pub fn from_unix_secs(secs: i64) -> Self {
        let days = secs.div_euclid(86_400);
        let rem_secs = secs.rem_euclid(86_400);
        let hour = (rem_secs / 3600) as u8;
        let minute = ((rem_secs % 3600) / 60) as u8;
        let second = (rem_secs % 60) as u8;

        // 1970-01-01 was Thursday (weekday 4).
        let weekday = ((days + 4).rem_euclid(7)) as u8;

        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let m = mp + if mp < 10 { 3 } else { -9 };
        let month = m as u8;
        let year = (yoe + era * 400 + if m <= 2 { 1 } else { 0 }) as i32;

        UtcDateTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
            weekday,
        }
    }

    pub fn to_unix_secs(&self) -> i64 {
        let y = self.year - if self.month <= 2 { 1 } else { 0 };
        let era = y.div_euclid(400);
        let yoe = y.rem_euclid(400);
        let m = self.month as i32 + if self.month > 2 { -3 } else { 9 };
        let doy = (153 * m + 2) / 5 + self.day as i32 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era as i64 * 146_097 + doe as i64 - 719_468;
        days * 86_400 + self.hour as i64 * 3600 + self.minute as i64 * 60 + self.second as i64
    }
}

pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

#[derive(Debug, Clone)]
pub struct CronExpr {
    pub minutes: HashSet<u8>,
    pub hours: HashSet<u8>,
    pub days_of_month: HashSet<u8>,
    pub dom_wild: bool,
    pub months: HashSet<u8>,
    pub days_of_week: HashSet<u8>,
    pub dow_wild: bool,
}

impl CronExpr {
    pub fn parse(expr: &str) -> Result<Self, String> {
        let fields: Vec<&str> = expr.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(format!(
                "INVALID_ARGUMENT: {expr}: expected 5 fields, got {}",
                fields.len()
            ));
        }

        let minutes = parse_field(fields[0], 0, 59, expr, false)?;
        let hours = parse_field(fields[1], 0, 23, expr, false)?;
        let dom_wild = fields[2] == "*";
        let days_of_month = parse_field(fields[2], 1, 31, expr, false)?;
        let months = parse_field(fields[3], 1, 12, expr, false)?;
        let dow_wild = fields[4] == "*";
        let days_of_week = parse_field(fields[4], 0, 7, expr, true)?;

        Ok(CronExpr {
            minutes,
            hours,
            days_of_month,
            dom_wild,
            months,
            days_of_week,
            dow_wild,
        })
    }

    /// Finds the next timestamp (at a minute boundary) strictly after `after_secs`.
    pub fn next_after(&self, after_secs: i64) -> Result<i64, String> {
        let mut start_secs = (after_secs / 60 + 1) * 60;
        let max_secs = start_secs + 5 * 365 * 86_400;

        while start_secs <= max_secs {
            let mut dt = UtcDateTime::from_unix_secs(start_secs);

            if !self.months.contains(&dt.month) {
                // Skip to beginning of next month
                if dt.month == 12 {
                    dt.year += 1;
                    dt.month = 1;
                } else {
                    dt.month += 1;
                }
                dt.day = 1;
                dt.hour = 0;
                dt.minute = 0;
                dt.second = 0;
                start_secs = dt.to_unix_secs();
                continue;
            }

            let dom_matches = self.days_of_month.contains(&dt.day);
            let dow_matches = self.days_of_week.contains(&dt.weekday);
            let day_matches = if self.dom_wild && self.dow_wild {
                true
            } else if self.dom_wild {
                dow_matches
            } else if self.dow_wild {
                dom_matches
            } else {
                dom_matches || dow_matches
            };

            if !day_matches {
                // Skip to next day
                dt.day += 1;
                if dt.day > days_in_month(dt.year, dt.month) {
                    if dt.month == 12 {
                        dt.year += 1;
                        dt.month = 1;
                    } else {
                        dt.month += 1;
                    }
                    dt.day = 1;
                }
                dt.hour = 0;
                dt.minute = 0;
                dt.second = 0;
                start_secs = dt.to_unix_secs();
                continue;
            }

            if !self.hours.contains(&dt.hour) {
                // Skip to next hour
                start_secs += 3600 - (dt.minute as i64 * 60);
                continue;
            }

            if !self.minutes.contains(&dt.minute) {
                start_secs += 60;
                continue;
            }

            return Ok(start_secs);
        }

        Err("no matching cron time found within 5 years".to_owned())
    }
}

fn parse_field(
    field: &str,
    min: u8,
    max: u8,
    full_expr: &str,
    is_dow: bool,
) -> Result<HashSet<u8>, String> {
    let mut result = HashSet::new();
    for part in field.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(format!("INVALID_ARGUMENT: {full_expr}: empty field part"));
        }

        let (range_str, step) = if let Some((r, s)) = part.split_once('/') {
            let step_num: u8 = s
                .parse()
                .map_err(|_| format!("INVALID_ARGUMENT: {full_expr}: invalid step '{s}'"))?;
            if step_num == 0 {
                return Err(format!("INVALID_ARGUMENT: {full_expr}: step cannot be 0"));
            }
            (r, step_num)
        } else {
            (part, 1)
        };

        let (start, end) = if range_str == "*" {
            (min, max)
        } else if let Some((s, e)) = range_str.split_once('-') {
            let start_num: u8 = s
                .parse()
                .map_err(|_| format!("INVALID_ARGUMENT: {full_expr}: invalid range start '{s}'"))?;
            let end_num: u8 = e
                .parse()
                .map_err(|_| format!("INVALID_ARGUMENT: {full_expr}: invalid range end '{e}'"))?;
            if start_num > end_num {
                return Err(format!(
                    "INVALID_ARGUMENT: {full_expr}: start '{start_num}' > end '{end_num}'"
                ));
            }
            (start_num, end_num)
        } else {
            let num: u8 = range_str.parse().map_err(|_| {
                format!("INVALID_ARGUMENT: {full_expr}: invalid number '{range_str}'")
            })?;
            (num, num)
        };

        if start < min || end > max {
            return Err(format!(
                "INVALID_ARGUMENT: {full_expr}: value out of range ({min}-{max})"
            ));
        }

        let mut val = start;
        while val <= end {
            if is_dow && val == 7 {
                result.insert(0); // 7 is Sunday, same as 0
            } else {
                result.insert(val);
            }
            if let Some(next) = val.checked_add(step) {
                val = next;
            } else {
                break;
            }
        }
    }

    if result.is_empty() {
        return Err(format!(
            "INVALID_ARGUMENT: {full_expr}: field matched no values"
        ));
    }

    Ok(result)
}

const MAX_TIMER_SECS: f64 = 365.0 * 86_400.0;

pub fn install(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    lua.set_named_registry_value(TIMERS, lua.create_table()?)?;

    let timer_table = lua.create_table()?;

    let env_after = env.clone();
    timer_table.raw_set(
        "after",
        function(lua, move |lua, (seconds, handler): (f64, Function)| {
            if !seconds.is_finite() || seconds < 0.0 || seconds > MAX_TIMER_SECS {
                return Err(raise(
                    "INVALID_ARGUMENT: delay must be finite, non-negative, and at most one year",
                ));
            }
            let timers = env_after
                .timers
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if timers.len() >= 1000 {
                return Err(raise("PLUGIN_LIMIT: at most 1,000 timers"));
            }
            drop(timers);
            let id = env_after.next_timer_id.fetch_add(1, Ordering::Relaxed);
            let reg: Table = lua.named_registry_value(TIMERS)?;
            reg.raw_set(id, handler)?;

            let env_c = env_after.clone();
            let handle = tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs_f64(seconds)).await;
                env_c.queue.push(PluginEvent {
                    name: "timer".into(),
                    payload: serde_json::json!({ "timer_id": id, "repeating": false }),
                    reply: None,
                });
            });

            env_after
                .timers
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(id, handle);
            Ok(id)
        })?,
    )?;

    let env_every = env.clone();
    timer_table.raw_set(
        "every",
        function(lua, move |lua, (seconds, handler): (f64, Function)| {
            if !seconds.is_finite() || seconds < 1.0 || seconds > MAX_TIMER_SECS {
                return Err(raise(
                    "INVALID_ARGUMENT: interval must be finite, at least 1 second, and at most one year",
                ));
            }
            let timers = env_every
                .timers
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if timers.len() >= 1000 {
                return Err(raise("PLUGIN_LIMIT: at most 1,000 timers"));
            }
            drop(timers);
            let id = env_every.next_timer_id.fetch_add(1, Ordering::Relaxed);
            let reg: Table = lua.named_registry_value(TIMERS)?;
            reg.raw_set(id, handler)?;

            let env_c = env_every.clone();
            let dur = Duration::from_secs_f64(seconds);
            let handle = tokio::spawn(async move {
                loop {
                    tokio::time::sleep(dur).await;
                    env_c.queue.push(PluginEvent {
                        name: "timer".into(),
                        payload: serde_json::json!({ "timer_id": id, "repeating": true }),
                        reply: None,
                    });
                }
            });

            env_every
                .timers
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(id, handle);
            Ok(id)
        })?,
    )?;

    let env_cron = env.clone();
    timer_table.raw_set(
        "cron",
        function(lua, move |lua, (expr, handler): (String, Function)| {
            let cron = CronExpr::parse(&expr).map_err(raise)?;
            let timers = env_cron
                .timers
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if timers.len() >= 1000 {
                return Err(raise("PLUGIN_LIMIT: at most 1,000 timers"));
            }
            drop(timers);
            let id = env_cron.next_timer_id.fetch_add(1, Ordering::Relaxed);
            let reg: Table = lua.named_registry_value(TIMERS)?;
            reg.raw_set(id, handler)?;

            let env_c = env_cron.clone();
            let handle = tokio::spawn(async move {
                loop {
                    let now_secs = crate::state::now_ms() / 1000;
                    let next_secs = match cron.next_after(now_secs) {
                        Ok(s) => s,
                        Err(_) => break,
                    };
                    let delay = (next_secs - now_secs).max(0) as u64;
                    tokio::time::sleep(Duration::from_secs(delay)).await;
                    env_c.queue.push(PluginEvent {
                        name: "timer".into(),
                        payload: serde_json::json!({ "timer_id": id, "repeating": true }),
                        reply: None,
                    });
                }
            });

            env_cron
                .timers
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(id, handle);
            Ok(id)
        })?,
    )?;

    let env_cancel = env.clone();
    timer_table.raw_set(
        "cancel",
        function(lua, move |lua, timer_id: u64| {
            if let Some(task) = env_cancel
                .timers
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&timer_id)
            {
                task.abort();
            }
            if let Ok(reg) = lua.named_registry_value::<Table>(TIMERS) {
                let _ = reg.raw_remove(timer_id);
            }
            Ok(())
        })?,
    )?;

    lua.globals().raw_set("timer", timer_table)?;
    Ok(())
}
