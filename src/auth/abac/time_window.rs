//! Time-based access restrictions

use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};

/// Day of the week (0 = Sunday, 6 = Saturday)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum DayOfWeek {
    Sunday = 0,
    Monday = 1,
    Tuesday = 2,
    Wednesday = 3,
    Thursday = 4,
    Friday = 5,
    Saturday = 6,
}

impl DayOfWeek {
    /// Convert from chrono Weekday
    pub fn from_chrono(wd: chrono::Weekday) -> Self {
        match wd {
            chrono::Weekday::Sun => DayOfWeek::Sunday,
            chrono::Weekday::Mon => DayOfWeek::Monday,
            chrono::Weekday::Tue => DayOfWeek::Tuesday,
            chrono::Weekday::Wed => DayOfWeek::Wednesday,
            chrono::Weekday::Thu => DayOfWeek::Thursday,
            chrono::Weekday::Fri => DayOfWeek::Friday,
            chrono::Weekday::Sat => DayOfWeek::Saturday,
        }
    }

    /// Get numeric value (0-6)
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Time window for access control
#[derive(Debug, Clone)]
pub struct TimeWindow {
    /// Access allowed after this time
    pub start: Option<DateTime<Utc>>,
    /// Access allowed before this time
    pub end: Option<DateTime<Utc>>,
    /// Allowed days of week (0 = Sunday, 6 = Saturday)
    pub days_of_week: Option<Vec<u8>>,
    /// Allowed hours of day (0-23)
    pub hours_of_day: Option<Vec<u8>>,
}

impl TimeWindow {
    /// Create a new empty time window (no restrictions)
    pub fn new() -> Self {
        Self {
            start: None,
            end: None,
            days_of_week: None,
            hours_of_day: None,
        }
    }

    /// Set start and end times
    pub fn with_range(start: Option<DateTime<Utc>>, end: Option<DateTime<Utc>>) -> Self {
        Self {
            start,
            end,
            days_of_week: None,
            hours_of_day: None,
        }
    }

    /// Set allowed days of week
    pub fn with_days(mut self, days: Vec<u8>) -> Self {
        self.days_of_week = Some(days);
        self
    }

    /// Set allowed hours of day
    pub fn with_hours(mut self, hours: Vec<u8>) -> Self {
        self.hours_of_day = Some(hours);
        self
    }

    /// Check if a given time is within the allowed window
    pub fn is_allowed(&self, time: DateTime<Utc>) -> bool {
        // Check time range
        if let Some(start) = self.start {
            if time < start {
                return false;
            }
        }

        if let Some(end) = self.end {
            if time > end {
                return false;
            }
        }

        // Check day of week
        if let Some(ref allowed_days) = self.days_of_week {
            let day = DayOfWeek::from_chrono(time.weekday()).as_u8();
            if !allowed_days.contains(&day) {
                return false;
            }
        }

        // Check hour of day
        if let Some(ref allowed_hours) = self.hours_of_day {
            let hour = time.hour() as u8;
            if !allowed_hours.contains(&hour) {
                return false;
            }
        }

        true
    }

    /// Create a business hours window (Monday-Friday, 9 AM - 5 PM UTC)
    pub fn business_hours() -> Self {
        Self {
            start: None,
            end: None,
            days_of_week: Some(vec![1, 2, 3, 4, 5]), // Mon-Fri
            hours_of_day: Some((9..17).collect()),   // 9 AM - 4 PM (17 is exclusive)
        }
    }

    /// Create a weekend-only window
    pub fn weekends_only() -> Self {
        Self {
            start: None,
            end: None,
            days_of_week: Some(vec![0, 6]), // Sun, Sat
            hours_of_day: None,
        }
    }
}

impl Default for TimeWindow {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_time_window_no_restrictions() {
        let window = TimeWindow::new();
        let time = Utc::now();
        assert!(window.is_allowed(time));
    }

    #[test]
    fn test_time_window_range() {
        let start = Utc
            .with_ymd_and_hms(2024, 1, 1, 0, 0, 0)
            .single()
            .expect("Failed to create start datetime for time window range test");
        let end = Utc
            .with_ymd_and_hms(2024, 12, 31, 23, 59, 59)
            .single()
            .expect("Failed to create end datetime for time window range test");
        let window = TimeWindow::with_range(Some(start), Some(end));

        let valid_time = Utc
            .with_ymd_and_hms(2024, 6, 15, 12, 0, 0)
            .single()
            .expect("Failed to create valid datetime for time window range test");
        assert!(window.is_allowed(valid_time));

        let invalid_time = Utc
            .with_ymd_and_hms(2025, 1, 1, 0, 0, 0)
            .single()
            .expect("Failed to create invalid datetime for time window range test");
        assert!(!window.is_allowed(invalid_time));
    }

    #[test]
    fn test_time_window_days_of_week() {
        let window = TimeWindow::new().with_days(vec![1, 2, 3, 4, 5]); // Mon-Fri

        // Monday
        let monday = Utc
            .with_ymd_and_hms(2024, 1, 1, 12, 0, 0)
            .single()
            .expect("Failed to create Monday datetime for days of week test");
        assert!(window.is_allowed(monday));

        // Saturday
        let saturday = Utc
            .with_ymd_and_hms(2024, 1, 6, 12, 0, 0)
            .single()
            .expect("Failed to create Saturday datetime for days of week test");
        assert!(!window.is_allowed(saturday));
    }

    #[test]
    fn test_time_window_hours() {
        let window = TimeWindow::new().with_hours(vec![9, 10, 11, 12, 13, 14, 15, 16]);

        // 10 AM - allowed
        let morning = Utc
            .with_ymd_and_hms(2024, 1, 1, 10, 0, 0)
            .single()
            .expect("Failed to create 10 AM datetime for hours test");
        assert!(window.is_allowed(morning));

        // 8 AM - not allowed
        let early = Utc
            .with_ymd_and_hms(2024, 1, 1, 8, 0, 0)
            .single()
            .expect("Failed to create 8 AM datetime for hours test");
        assert!(!window.is_allowed(early));

        // 5 PM (17:00) - not allowed
        let late = Utc
            .with_ymd_and_hms(2024, 1, 1, 17, 0, 0)
            .single()
            .expect("Failed to create 5 PM datetime for hours test");
        assert!(!window.is_allowed(late));
    }

    #[test]
    fn test_business_hours() {
        let window = TimeWindow::business_hours();

        // Monday at 10 AM - allowed
        let monday_morning = Utc
            .with_ymd_and_hms(2024, 1, 1, 10, 0, 0)
            .single()
            .expect("Failed to create Monday 10 AM datetime for business hours test");
        assert!(window.is_allowed(monday_morning));

        // Saturday at 10 AM - not allowed
        let saturday_morning = Utc
            .with_ymd_and_hms(2024, 1, 6, 10, 0, 0)
            .single()
            .expect("Failed to create Saturday 10 AM datetime for business hours test");
        assert!(!window.is_allowed(saturday_morning));

        // Monday at 6 PM - not allowed
        let monday_evening = Utc
            .with_ymd_and_hms(2024, 1, 1, 18, 0, 0)
            .single()
            .expect("Failed to create Monday 6 PM datetime for business hours test");
        assert!(!window.is_allowed(monday_evening));
    }

    #[test]
    fn test_weekends_only() {
        let window = TimeWindow::weekends_only();

        // Saturday - allowed
        let saturday = Utc
            .with_ymd_and_hms(2024, 1, 6, 12, 0, 0)
            .single()
            .expect("Failed to create Saturday datetime for weekends only test");
        assert!(window.is_allowed(saturday));

        // Monday - not allowed
        let monday = Utc
            .with_ymd_and_hms(2024, 1, 1, 12, 0, 0)
            .single()
            .expect("Failed to create Monday datetime for weekends only test");
        assert!(!window.is_allowed(monday));
    }
}
