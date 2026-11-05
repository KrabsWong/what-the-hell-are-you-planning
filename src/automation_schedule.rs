//! Pure trigger arithmetic for automations. No UI, process or storage dependencies.
//!
//! All trigger points are represented as UTC epoch seconds. Rules retain their local
//! wall-clock time and IANA timezone. Preview and dispatch call the same functions.
use chrono::{Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc, Weekday};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub const MIN_INTERVAL_MINUTES: u32 = 1;
pub const MAX_INTERVAL_MINUTES: u32 = 10 * 365 * 24 * 60;
/// A missed calendar run can be at most 16 days behind; anything larger is clock
/// corruption rather than a realistic wake-from-sleep. This bounds the search.
const MAX_CALENDAR_SEARCH_DAYS: i64 = 16;
const MAX_PREVIEW: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScheduleSpec {
    /// Fixed frequency anchored to the moment the automation was enabled.
    Interval { minutes: u32 },
    /// One local wall-clock time every day.
    Daily { hour: u8, minute: u8 },
    /// One local wall-clock time on each selected weekday (0 = Monday).
    Weekly { days: Vec<u8>, hour: u8, minute: u8 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
    pub spec: ScheduleSpec,
    /// IANA identifier such as `Asia/Shanghai`. Fixed-offset strings are rejected.
    pub timezone: String,
}

impl Schedule {
    pub fn timezone(&self) -> Result<Tz, String> {
        parse_timezone(&self.timezone)
    }
    pub fn validate(&self) -> Result<(), String> {
        self.timezone()?;
        validate_spec(&self.spec)
    }
    /// First trigger strictly after the enable instant.
    pub fn first_after(&self, enabled_at: i64) -> Result<i64, String> {
        self.validate()?;
        next_after(&self.spec, self.timezone()?, enabled_at, enabled_at)
    }
    pub fn next_after(&self, anchor: i64, after: i64) -> Result<i64, String> {
        self.validate()?;
        next_after(&self.spec, self.timezone()?, anchor, after)
    }
    pub fn preview(&self, anchor: i64, from: i64, count: usize) -> Result<Vec<i64>, String> {
        self.validate()?;
        let count = count.clamp(1, MAX_PREVIEW);
        let mut values = Vec::with_capacity(count);
        let mut cursor = from;
        for _ in 0..count {
            let next = next_after(&self.spec, self.timezone()?, anchor, cursor)?;
            values.push(next);
            cursor = next;
        }
        Ok(values)
    }
    pub fn signature(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

pub fn parse_timezone(value: &str) -> Result<Tz, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Choose a time zone.".into());
    }
    let tz = Tz::from_str(value)
        .map_err(|_| format!("“{value}” is not a known IANA time zone. Choose a city time zone such as Asia/Shanghai."))?;
    // A fixed UTC offset is not an IANA zone and must not masquerade as one.
    if tz.name() == "UTC" && value != "UTC" {
        return Err("Use an IANA time zone such as Asia/Shanghai, not a fixed offset.".into());
    }
    Ok(tz)
}

fn validate_spec(spec: &ScheduleSpec) -> Result<(), String> {
    match spec {
        ScheduleSpec::Interval { minutes } => {
            if *minutes < MIN_INTERVAL_MINUTES {
                return Err(format!(
                    "The interval must be at least {MIN_INTERVAL_MINUTES} minute."
                ));
            }
            if *minutes > MAX_INTERVAL_MINUTES {
                return Err("The interval is too long for the first version (maximum 10 years).".into());
            }
            interval_seconds(*minutes)?;
        }
        ScheduleSpec::Daily { hour, minute } => {
            validate_clock(*hour, *minute)?;
        }
        ScheduleSpec::Weekly { days, hour, minute } => {
            validate_clock(*hour, *minute)?;
            if days.is_empty() {
                return Err("Choose at least one weekday.".into());
            }
            if days.iter().any(|day| *day > 6) {
                return Err("Weekdays must be 0 (Monday) through 6 (Sunday).".into());
            }
            let mut unique = days.clone();
            unique.sort_unstable();
            unique.dedup();
            if unique != *days {
                return Err("Weekdays must be unique and sorted.".into());
            }
        }
    }
    Ok(())
}

fn validate_clock(hour: u8, minute: u8) -> Result<(), String> {
    if hour > 23 || minute > 59 {
        return Err("Enter a valid local time (00:00–23:59).".into());
    }
    Ok(())
}

/// First-version default allowed delay: one nominal period, capped at 24 hours.
pub fn default_allowed_delay(schedule: &Schedule) -> Result<i64, String> {
    schedule.validate()?;
    Ok(match schedule.spec {
        ScheduleSpec::Interval { minutes } => interval_seconds(minutes)?.clamp(60, 86_400),
        ScheduleSpec::Daily { .. } | ScheduleSpec::Weekly { .. } => 86_400,
    })
}

pub fn interval_seconds(minutes: u32) -> Result<i64, String> {
    let minutes = i64::from(minutes);
    minutes
        .checked_mul(60)
        .filter(|seconds| *seconds > 0)
        .ok_or_else(|| "The interval is too large to schedule.".into())
}

/// Trigger strictly after `after`, using the fixed anchor grid for intervals.
pub fn next_after(spec: &ScheduleSpec, tz: Tz, anchor: i64, after: i64) -> Result<i64, String> {
    match spec {
        ScheduleSpec::Interval { minutes } => {
            let period = interval_seconds(*minutes)?;
            let first = anchor
                .checked_add(period)
                .ok_or("The schedule anchor is out of range.")?;
            if after < first {
                return Ok(first);
            }
            let steps = (after - first) / period;
            first
                .checked_add(
                    steps
                        .checked_add(1)
                        .and_then(|steps| steps.checked_mul(period))
                        .ok_or("The schedule is out of range.")?,
                )
                .ok_or_else(|| "The next trigger is out of range.".into())
        }
        ScheduleSpec::Daily { .. } | ScheduleSpec::Weekly { .. } => calendar_next(spec, tz, after),
    }
}

fn calendar_next(spec: &ScheduleSpec, tz: Tz, after: i64) -> Result<i64, String> {
    let (time, days) = calendar_parts(spec)?;
    let local = Utc
        .timestamp_opt(after, 0)
        .single()
        .ok_or("Invalid reference time.")?
        .with_timezone(&tz);
    let start = local.date_naive();
    for offset in 0..=MAX_CALENDAR_SEARCH_DAYS {
        let date = start
            .checked_add_signed(Duration::days(offset))
            .ok_or("The calendar date is out of range.")?;
        if let Some(days) = &days
            && !days.contains(&weekday_index(date.weekday()))
        {
            continue;
        }
        if let Some(candidate) = local_trigger(tz, date, time)?
            && candidate > after
        {
            return Ok(candidate);
        }
    }
    Err("No calendar trigger could be computed; check the time zone and scheduled time.".into())
}

fn calendar_parts(spec: &ScheduleSpec) -> Result<(NaiveTime, Option<Vec<u8>>), String> {
    match spec {
        ScheduleSpec::Daily { hour, minute } => Ok((clock(*hour, *minute)?, None)),
        ScheduleSpec::Weekly { days, hour, minute } => {
            Ok((clock(*hour, *minute)?, Some(days.clone())))
        }
        ScheduleSpec::Interval { .. } => Err("Not a calendar schedule.".into()),
    }
}

fn clock(hour: u8, minute: u8) -> Result<NaiveTime, String> {
    validate_clock(hour, minute)?;
    NaiveTime::from_hms_opt(u32::from(hour), u32::from(minute), 0)
        .ok_or_else(|| "Enter a valid local time.".into())
}

/// Resolve a local wall time. A nonexistent DST wall time is skipped;
/// an ambiguous one uses its first (earliest UTC) occurrence.
pub fn local_trigger(tz: Tz, date: NaiveDate, time: NaiveTime) -> Result<Option<i64>, String> {
    let local = NaiveDateTime::new(date, time);
    match tz.from_local_datetime(&local) {
        LocalResult::Single(value) => Ok(Some(value.timestamp())),
        LocalResult::Ambiguous(earliest, _) => Ok(Some(earliest.timestamp())),
        LocalResult::None => Ok(None),
    }
}

/// Latest calendar or interval trigger at or before `now`, or `None` before the first
/// grid point. Bounded: intervals are arithmetic, calendars search at most 16 days back.
pub fn latest_at_or_before(spec: &ScheduleSpec, anchor: i64, now: i64) -> Result<Option<i64>, String> {
    match spec {
        ScheduleSpec::Interval { minutes } => {
            let period = interval_seconds(*minutes)?;
            let first = anchor
                .checked_add(period)
                .ok_or("The schedule anchor is out of range.")?;
            if now < first {
                return Ok(None);
            }
            let steps = (now - first) / period;
            Ok(Some(
                first
                    .checked_add(
                        steps
                            .checked_mul(period)
                            .ok_or("The schedule is out of range.")?,
                    )
                    .ok_or("The schedule is out of range.")?,
            ))
        }
        ScheduleSpec::Daily { .. } | ScheduleSpec::Weekly { .. } => Ok(None),
    }
}

/// Calendar `latest_at_or_before` needs the timezone, stored by the caller in `Schedule`.
pub fn latest_due_for(schedule: &Schedule, anchor: i64, now: i64) -> Result<Option<i64>, String> {
    schedule.validate()?;
    match &schedule.spec {
        ScheduleSpec::Interval { .. } => latest_at_or_before(&schedule.spec, anchor, now),
        ScheduleSpec::Daily { .. } | ScheduleSpec::Weekly { .. } => {
            let tz = schedule.timezone()?;
            Ok(calendar_latest_in(tz, &schedule.spec, now))
        }
    }
}

fn calendar_latest_in(tz: Tz, spec: &ScheduleSpec, now: i64) -> Option<i64> {
    let Ok((time, days)) = calendar_parts(spec) else { return None };
    let local = Utc.timestamp_opt(now, 0).single()?.with_timezone(&tz);
    let start = local.date_naive();
    for offset in 0..=MAX_CALENDAR_SEARCH_DAYS {
        let date = start.checked_sub_signed(Duration::days(offset))?;
        if let Some(days) = &days
            && !days.contains(&weekday_index(date.weekday()))
        {
            continue;
        }
        if let Ok(Some(candidate)) = local_trigger(tz, date, time)
            && candidate <= now
        {
            return Some(candidate);
        }
    }
    None
}

/// Number of triggers strictly after `from` and at or before `to`, by arithmetic for
/// intervals and a bounded calendar walk (no entities created).
pub fn count_between(schedule: &Schedule, anchor: i64, from: i64, to: i64) -> Result<u64, String> {
    if to <= from {
        return Ok(0);
    }
    match &schedule.spec {
        ScheduleSpec::Interval { minutes } => {
            let period = interval_seconds(*minutes)?;
            let first = anchor.checked_add(period).ok_or("The anchor is out of range.")?;
            if to < first {
                return Ok(0);
            }
            let start_index = if from < first {
                0
            } else {
                ((from - first) / period) + 1
            };
            let end_index = (to - first) / period;
            Ok(end_index.saturating_sub(start_index) as u64 + 1)
        }
        _ => {
            let mut count = 0u64;
            let mut cursor = from;
            for _ in 0..4096 {
                let next = next_after(&schedule.spec, schedule.timezone()?, anchor, cursor)?;
                if next > to {
                    break;
                }
                count += 1;
                cursor = next;
            }
            Ok(count)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DueDecision {
    /// Latest trigger that should be represented now.
    pub trigger: i64,
    /// Triggers after the old cursor and before `trigger` collapsed into a summary.
    pub skipped_count: u64,
    pub skipped_from: Option<i64>,
    pub skipped_to: Option<i64>,
    /// True when the newest trigger is older than the allowed delay.
    pub expired: bool,
    pub next_cursor: i64,
}

/// Decide what an enabled automation owes at `now`.
///
/// `cursor` is the next unprocessed trigger. Time going backwards cannot replay an
/// already processed point because a trigger is only produced when `now >= cursor`.
pub fn decide(
    schedule: &Schedule,
    anchor: i64,
    cursor: i64,
    now: i64,
    allowed_delay_seconds: i64,
    catch_up: CatchUp,
) -> Result<Option<DueDecision>, String> {
    schedule.validate()?;
    if allowed_delay_seconds <= 0 {
        return Err("The allowed delay must be a positive duration.".into());
    }
    if now < cursor {
        return Ok(None);
    }
    let latest = latest_due_for(schedule, anchor, now)?.unwrap_or(cursor);
    let trigger = latest.max(cursor);
    let expired = now.saturating_sub(trigger) > allowed_delay_seconds;
    let next_cursor = schedule.next_after(anchor, trigger)?;
    let has_older = trigger > cursor;
    if catch_up == CatchUp::SkipAll && has_older {
        let skipped_count = count_between(schedule, anchor, cursor, trigger)? + 1;
        return Ok(Some(DueDecision {
            trigger,
            skipped_count,
            skipped_from: Some(cursor),
            skipped_to: Some(trigger),
            expired: false,
            next_cursor,
        }));
    }
    // Older triggers collapse into one summary; only the newest is represented.
    let (skipped_count, skipped_from, skipped_to) = if has_older {
        match previous_trigger(schedule, anchor, trigger) {
            Some(older) if older >= cursor => (
                count_between(schedule, anchor, cursor, older)? + 1,
                Some(cursor),
                Some(older),
            ),
            _ => (0, None, None),
        }
    } else {
        (0, None, None)
    };
    Ok(Some(DueDecision {
        trigger,
        skipped_count,
        skipped_from,
        skipped_to,
        expired,
        next_cursor,
    }))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatchUp {
    /// Run only the newest missed trigger (default).
    LatestOnly,
    /// Skip every missed trigger and wait for the next future one.
    SkipAll,
}

fn previous_trigger(schedule: &Schedule, anchor: i64, current: i64) -> Option<i64> {
    match &schedule.spec {
        ScheduleSpec::Interval { minutes } => {
            let period = interval_seconds(*minutes).ok()?;
            Some(current - period).filter(|value| *value > anchor)
        }
        _ => {
            let tz = schedule.timezone().ok()?;
            let (time, days) = calendar_parts(&schedule.spec).ok()?;
            let local = Utc.timestamp_opt(current, 0).single()?.with_timezone(&tz);
            let start = local.date_naive();
            for offset in 1..=MAX_CALENDAR_SEARCH_DAYS {
                let date = start.checked_sub_signed(Duration::days(offset))?;
                if let Some(days) = &days
                    && !days.contains(&weekday_index(date.weekday()))
                {
                    continue;
                }
                if let Ok(Some(candidate)) = local_trigger(tz, date, time)
                    && candidate < current
                {
                    return Some(candidate);
                }
            }
            None
        }
    }
}

pub fn weekday_index(day: Weekday) -> u8 {
    day.num_days_from_monday() as u8
}

/// Previous natural day in the automation's timezone, as a half-open UTC window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataWindow {
    pub start_utc: i64,
    /// Exclusive.
    pub end_utc: i64,
}

pub fn previous_local_day(reference_utc: i64, tz: Tz) -> Result<DataWindow, String> {
    let local = Utc
        .timestamp_opt(reference_utc, 0)
        .single()
        .ok_or("Invalid reference time.")?
        .with_timezone(&tz);
    let end_date = local.date_naive();
    let start_date = end_date
        .pred_opt()
        .ok_or("The reference date is out of range.")?;
    Ok(DataWindow {
        start_utc: local_midnight(tz, start_date)?,
        end_utc: local_midnight(tz, end_date)?,
    })
}

/// Midnight can be nonexistent on a DST transition; use the first existing wall time.
fn local_midnight(tz: Tz, date: NaiveDate) -> Result<i64, String> {
    for minute in 0..=(6 * 60) {
        let time = NaiveTime::from_hms_opt(minute / 60, minute % 60, 0)
            .ok_or("Invalid local midnight.")?;
        if let Some(value) = local_trigger(tz, date, time)? {
            return Ok(value);
        }
    }
    Err("The local date has no resolvable start in this time zone.".into())
}

/// A resolved point in time used by scheduling, previews and occurrence context.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledTime {
    pub utc: i64,
    pub timezone: String,
    pub local: String,
}

impl ScheduledTime {
    pub fn new(utc: i64, timezone: &str) -> Result<Self, String> {
        let tz = parse_timezone(timezone)?;
        let value = Utc
            .timestamp_opt(utc, 0)
            .single()
            .ok_or("Invalid scheduled time.")?
            .with_timezone(&tz);
        Ok(Self {
            utc,
            timezone: timezone.into(),
            local: value.format("%Y-%m-%d %H:%M %Z").to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn interval(minutes: u32, tz: &str) -> Schedule {
        Schedule {
            spec: ScheduleSpec::Interval { minutes },
            timezone: tz.into(),
        }
    }
    fn daily(hour: u8, minute: u8, tz: &str) -> Schedule {
        Schedule {
            spec: ScheduleSpec::Daily { hour, minute },
            timezone: tz.into(),
        }
    }
    fn weekly(days: &[u8], hour: u8, minute: u8, tz: &str) -> Schedule {
        Schedule {
            spec: ScheduleSpec::Weekly {
                days: days.to_vec(),
                hour,
                minute,
            },
            timezone: tz.into(),
        }
    }
    fn utc(text: &str) -> i64 {
        DateTime::parse_from_rfc3339(text).unwrap().timestamp()
    }
    fn text(value: i64) -> String {
        DateTime::<Utc>::from_timestamp(value, 0)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string()
    }

    #[test]
    fn interval_preview_is_fixed_from_the_enable_anchor_not_completion() {
        let enabled = utc("2026-11-05T10:03:00Z");
        let schedule = interval(5, "UTC");
        let preview = schedule.preview(enabled, enabled, 3).unwrap();
        assert_eq!(
            preview.iter().map(|v| text(*v)).collect::<Vec<_>>(),
            [
                "2026-11-05T10:08:00Z",
                "2026-11-05T10:13:00Z",
                "2026-11-05T10:18:00Z"
            ]
        );
        // A long run does not change the series.
        assert_eq!(schedule.next_after(enabled, preview[0]).unwrap(), preview[1]);
        assert_eq!(schedule.next_after(enabled, preview[1]).unwrap(), preview[2]);
    }

    #[test]
    fn daily_and_weekly_cross_month_year_and_leap_day() {
        let schedule = daily(8, 30, "Asia/Shanghai");
        let enabled = utc("2026-12-31T09:00:00Z"); // 17:00 Shanghai
        assert_eq!(
            text(schedule.first_after(enabled).unwrap()),
            "2027-01-01T00:30:00Z"
        );
        let leap = daily(9, 0, "UTC").first_after(utc("2028-02-28T09:01:00Z")).unwrap();
        assert_eq!(text(leap), "2028-02-29T09:00:00Z");
        let weekly_schedule = weekly(&[4], 18, 0, "Asia/Shanghai");
        let friday = weekly_schedule
            .preview(utc("2026-11-05T00:00:00Z"), utc("2026-11-05T00:00:00Z"), 2)
            .unwrap();
        assert_eq!(text(friday[0]), "2026-11-06T10:00:00Z");
        assert_eq!(text(friday[1]), "2026-11-13T10:00:00Z");
    }

    #[test]
    fn invalid_schedules_are_rejected() {
        for schedule in [
            interval(0, "UTC"),
            Schedule {
                spec: ScheduleSpec::Daily { hour: 24, minute: 0 },
                timezone: "UTC".into(),
            },
            Schedule {
                spec: ScheduleSpec::Weekly {
                    days: vec![],
                    hour: 9,
                    minute: 0,
                },
                timezone: "UTC".into(),
            },
            Schedule {
                spec: ScheduleSpec::Weekly {
                    days: vec![1, 1],
                    hour: 9,
                    minute: 0,
                },
                timezone: "UTC".into(),
            },
            Schedule {
                spec: ScheduleSpec::Daily { hour: 9, minute: 0 },
                timezone: "+08:00".into(),
            },
            Schedule {
                spec: ScheduleSpec::Daily { hour: 9, minute: 0 },
                timezone: "Not/AZone".into(),
            },
        ] {
            assert!(schedule.validate().is_err(), "{schedule:?}");
        }
        let overflow = Schedule {
            spec: ScheduleSpec::Interval { minutes: u32::MAX },
            timezone: "UTC".into(),
        };
        assert!(overflow.validate().is_err());
        assert!(interval(1, "UTC").validate().is_ok());
    }

    #[test]
    fn dst_nonexistent_time_is_skipped_and_ambiguous_time_occurs_once() {
        // America/New_York springs forward 2026-03-08 at 02:00 → 03:00.
        let gap = daily(2, 30, "America/New_York");
        let after = utc("2026-03-07T08:00:00Z");
        let next = gap.first_after(after).unwrap();
        assert_eq!(text(next), "2026-03-09T06:30:00Z");
        // Fall back 2026-11-01: 01:30 occurs twice; choose the first (EDT, UTC-4).
        let ambiguous = daily(1, 30, "America/New_York");
        let next = ambiguous.first_after(utc("2026-10-31T12:00:00Z")).unwrap();
        assert_eq!(text(next), "2026-11-01T05:30:00Z");
        assert_eq!(
            text(ambiguous.next_after(utc("2026-10-31T12:00:00Z"), next).unwrap()),
            "2026-11-02T06:30:00Z"
        );
    }

    #[test]
    fn stored_rules_ignore_the_process_timezone() {
        let schedule = daily(9, 0, "Europe/Berlin");
        let first = schedule.first_after(utc("2026-07-01T00:00:00Z")).unwrap();
        assert_eq!(text(first), "2026-07-01T07:00:00Z");
        let winter = schedule.first_after(utc("2026-12-01T00:00:00Z")).unwrap();
        assert_eq!(text(winter), "2026-12-01T08:00:00Z");
    }

    #[test]
    fn catch_up_only_chooses_the_latest_trigger_and_summarizes_older_ones() {
        let enabled = utc("2026-11-05T00:00:00Z");
        let schedule = interval(1, "UTC");
        let now = utc("2026-11-05T00:10:00Z");
        let decision = decide(&schedule, enabled, enabled + 60, now, 86_400, CatchUp::LatestOnly)
            .unwrap()
            .unwrap();
        assert_eq!(text(decision.trigger), "2026-11-05T00:10:00Z");
        assert_eq!(decision.skipped_count, 9);
        assert_eq!(text(decision.skipped_from.unwrap()), "2026-11-05T00:01:00Z");
        assert_eq!(text(decision.skipped_to.unwrap()), "2026-11-05T00:09:00Z");
        assert_eq!(text(decision.next_cursor), "2026-11-05T00:11:00Z");
        let skip = decide(&schedule, enabled, enabled + 60, now, 86_400, CatchUp::SkipAll)
            .unwrap()
            .unwrap();
        assert_eq!(skip.skipped_count, 10);
        assert_eq!(text(skip.next_cursor), "2026-11-05T00:11:00Z");
    }

    #[test]
    fn expired_latest_is_flagged_but_cursor_still_advances() {
        let enabled = utc("2026-11-05T00:00:00Z");
        let schedule = interval(60, "UTC");
        let decision = decide(
            &schedule,
            enabled,
            enabled + 3600,
            enabled + 6 * 3600 + 120,
            60,
            CatchUp::LatestOnly,
        )
        .unwrap()
        .unwrap();
        assert!(decision.expired);
        assert_eq!(text(decision.next_cursor), "2026-11-05T07:00:00Z");
    }

    #[test]
    fn clock_rollback_never_replays_a_processed_trigger() {
        let enabled = utc("2026-11-05T00:00:00Z");
        let schedule = interval(5, "UTC");
        let cursor = enabled + 30 * 60;
        assert_eq!(
            decide(&schedule, enabled, cursor, cursor - 600, 86_400, CatchUp::LatestOnly).unwrap(),
            None
        );
        assert_eq!(
            decide(&schedule, enabled, cursor, cursor + 10, 86_400, CatchUp::LatestOnly).unwrap().unwrap().trigger,
            cursor
        );
    }

    #[test]
    fn year_scale_missed_range_is_bounded_and_not_expanded_into_entities() {
        let enabled = utc("2025-01-01T00:00:00Z");
        let schedule = interval(1, "UTC");
        let now = enabled + 365 * 86_400 + 180;
        let decision = decide(
            &schedule,
            enabled,
            enabled + 60,
            now,
            86_400,
            CatchUp::LatestOnly,
        )
        .unwrap()
        .unwrap();
        assert!(!decision.expired);
        assert_eq!(decision.skipped_count, 365 * 24 * 60 + 2);
        assert_eq!(decision.next_cursor, enabled + 365 * 86_400 + 240);
        let expired = decide(
            &schedule,
            enabled,
            enabled + 60,
            enabled + 90,
            10,
            CatchUp::LatestOnly,
        )
        .unwrap()
        .unwrap();
        assert!(expired.expired);
        assert_eq!(expired.skipped_count, 0);
        let daily = daily(9, 0, "UTC");
        let count = count_between(
            &daily,
            enabled,
            utc("2025-01-01T09:00:00Z"),
            utc("2026-01-01T09:00:00Z"),
        )
        .unwrap();
        assert_eq!(count, 365);
    }

    #[test]
    fn previous_natural_day_uses_the_task_timezone() {
        let reference = utc("2026-11-03T02:30:00Z"); // Tuesday 10:30 Shanghai
        let window = previous_local_day(reference, parse_timezone("Asia/Shanghai").unwrap()).unwrap();
        assert_eq!(window.start_utc, utc("2026-11-01T16:00:00Z"));
        assert_eq!(window.end_utc, utc("2026-11-02T16:00:00Z"));
        let ny = previous_local_day(
            utc("2026-03-09T12:00:00Z"),
            parse_timezone("America/New_York").unwrap(),
        )
        .unwrap();
        // 2026-03-08 is a 23-hour local day.
        assert_eq!(ny.end_utc - ny.start_utc, 23 * 3600);
    }

    #[test]
    fn calendar_latest_is_bounded_after_a_large_forward_jump() {
        let schedule = weekly(&[1, 3], 10, 0, "Asia/Shanghai");
        let now = utc("2027-06-01T03:00:00Z");
        let anchor = utc("2026-01-01T00:00:00Z");
        let latest = latest_due_for(&schedule, anchor, now).unwrap().unwrap();
        assert_eq!(latest, utc("2027-06-01T02:00:00Z"));
        let next = schedule.next_after(anchor, latest).unwrap();
        assert_eq!(text(next), "2027-06-03T02:00:00Z");
    }

    #[test]
    fn serialization_round_trip_preserves_schedule() {
        let schedule = weekly(&[0, 2, 4], 7, 5, "America/New_York");
        let restored: Schedule =
            serde_json::from_str(&serde_json::to_string(&schedule).unwrap()).unwrap();
        assert_eq!(restored, schedule);
    }
}
