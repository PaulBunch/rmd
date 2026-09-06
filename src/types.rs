use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum Status {
    #[default]
    Active,
    Missed,
    Triggered,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TimeFormat {
    #[default]
    Iso, // "2026-08-09 8:45"
    Human, // " 8:45 Sun 9 Aug"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReminderId {
    Active(u64),
    History(u64),
}

impl fmt::Display for ReminderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReminderId::Active(id) => write!(f, "{}", id),
            ReminderId::History(id) => write!(f, "h{}", id),
        }
    }
}

// Static parse error (0 bytes in memory, no allocations)
#[derive(Debug, Clone, Copy)]
pub struct ParseReminderIdError;

impl fmt::Display for ParseReminderIdError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Invalid ID format. Use '1' for active or 'h1' for history"
        )
    }
}
impl std::error::Error for ParseReminderIdError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdOrRange(pub Vec<ReminderId>);

impl FromStr for IdOrRange {
    type Err = ParseReminderIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err(ParseReminderIdError);
        }

        if let Some(pos) = s.find(':').or_else(|| s.find('-')) {
            let (left_str, right_str) = s.split_at(pos);
            let right_str = &right_str[1..]; // skip the delimiter

            let left = left_str.trim().parse::<ReminderId>()?;
            let right_parsed = right_str.trim().parse::<ReminderId>();

            let right = match (left, right_parsed) {
                (ReminderId::History(_), Ok(ReminderId::Active(num))) => ReminderId::History(num),
                (_, Ok(rid)) => rid,
                (_, Err(_)) => return Err(ParseReminderIdError),
            };

            match (left, right) {
                (ReminderId::Active(start), ReminderId::Active(end)) => {
                    let min = start.min(end);
                    let max = start.max(end);
                    Ok(IdOrRange((min..=max).map(ReminderId::Active).collect()))
                }
                (ReminderId::History(start), ReminderId::History(end)) => {
                    let min = start.min(end);
                    let max = start.max(end);
                    Ok(IdOrRange((min..=max).map(ReminderId::History).collect()))
                }
                _ => Err(ParseReminderIdError),
            }
        } else {
            let id = s.parse::<ReminderId>()?;
            Ok(IdOrRange(vec![id]))
        }
    }
}

impl FromStr for ReminderId {
    type Err = ParseReminderIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if let Some(stripped) = s.strip_prefix('h').or_else(|| s.strip_prefix('H')) {
            let num = stripped.parse::<u64>().map_err(|_| ParseReminderIdError)?;
            if num == 0 {
                return Err(ParseReminderIdError);
            }
            Ok(ReminderId::History(num))
        } else {
            let num = s.parse::<u64>().map_err(|_| ParseReminderIdError)?;
            if num == 0 {
                return Err(ParseReminderIdError);
            }
            Ok(ReminderId::Active(num))
        }
    }
}

impl Serialize for ReminderId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ReminderId::Active(id) => serializer.serialize_u64(*id),
            ReminderId::History(id) => serializer.serialize_str(&format!("h{}", id)),
        }
    }
}

impl<'de> Deserialize<'de> for ReminderId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ReminderIdVisitor;
        impl<'de> de::Visitor<'de> for ReminderIdVisitor {
            type Value = ReminderId;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an integer or 'h'-prefixed string")
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                if value == 0 {
                    return Err(de::Error::custom("ID cannot be 0"));
                }
                Ok(ReminderId::Active(value))
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                ReminderId::from_str(value).map_err(de::Error::custom)
            }
        }
        deserializer.deserialize_any(ReminderIdVisitor)
    }
}

/// Structure representing a single reminder
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Reminder {
    pub id: ReminderId,
    pub message: String,
    pub trigger_at: i64, // Unix timestamp in seconds

    #[serde(default)]
    pub status: Status,
}
pub struct IdAllocator {
    used: Vec<bool>,
}

impl IdAllocator {
    pub fn new(reminders: &[Reminder], is_history: bool) -> Self {
        let mut max_id = 0;
        // 1. Find the maximum ID used (O(N))
        for r in reminders {
            match (is_history, r.id) {
                (false, ReminderId::Active(id)) => max_id = max_id.max(id as usize),
                (true, ReminderId::History(id)) => max_id = max_id.max(id as usize),
                _ => {}
            }
        }

        // 2. Create a bitmap and mark occupied slots (O(N), one allocation)
        let mut used = vec![false; max_id + 1];
        used[0] = true; // Zero ID is forbidden
        for r in reminders {
            match (is_history, r.id) {
                (false, ReminderId::Active(id)) => used[id as usize] = true,
                (true, ReminderId::History(id)) => used[id as usize] = true,
                _ => {}
            }
        }
        Self { used }
    }

    pub fn next_id(&mut self) -> u64 {
        // Find the first `false` (O(N) worst case, but amortized O(1))
        for (i, &is_used) in self.used.iter().enumerate().skip(1) {
            if !is_used {
                self.used[i] = true;
                return i as u64;
            }
        }
        self.used.push(true);
        (self.used.len() - 1) as u64
    }
}

// =========================================================================
// TESTS
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Helper for quick creation of mock reminders in tests
    fn mock_reminder(id: ReminderId, trigger_at: i64, status: Status) -> Reminder {
        Reminder {
            id,
            message: "Test".to_string(),
            trigger_at,
            status,
        }
    }

    // -------------------------------------------------------------------------
    // ReminderId parsing and formatting tests
    // -------------------------------------------------------------------------
    #[test]
    fn test_reminder_id_parsing() {
        // Successful parsing of active IDs
        assert_eq!("1".parse::<ReminderId>().unwrap(), ReminderId::Active(1));
        assert_eq!("42".parse::<ReminderId>().unwrap(), ReminderId::Active(42));
        assert_eq!(
            "  5  ".parse::<ReminderId>().unwrap(),
            ReminderId::Active(5)
        ); // Trim check

        // Successful parsing of history IDs
        assert_eq!("h1".parse::<ReminderId>().unwrap(), ReminderId::History(1));
        assert_eq!(
            "H99".parse::<ReminderId>().unwrap(),
            ReminderId::History(99)
        );

        // Errors: zero IDs are invalid
        assert!("0".parse::<ReminderId>().is_err());
        assert!("h0".parse::<ReminderId>().is_err());

        // Errors: malformed strings
        assert!("h".parse::<ReminderId>().is_err());
        assert!("abc".parse::<ReminderId>().is_err());
        assert!("-5".parse::<ReminderId>().is_err());
    }

    #[test]
    fn test_reminder_id_display() {
        assert_eq!(ReminderId::Active(42).to_string(), "42");
        assert_eq!(ReminderId::History(7).to_string(), "h7");
    }

    // -------------------------------------------------------------------------
    // IdOrRange parsing tests
    // -------------------------------------------------------------------------
    #[test]
    fn test_id_or_range_parsing() {
        // Single active ID
        assert_eq!(
            "5".parse::<IdOrRange>().unwrap(),
            IdOrRange(vec![ReminderId::Active(5)])
        );

        // Single history ID
        assert_eq!(
            "h5".parse::<IdOrRange>().unwrap(),
            IdOrRange(vec![ReminderId::History(5)])
        );

        // Active range with colon
        assert_eq!(
            "5:8".parse::<IdOrRange>().unwrap(),
            IdOrRange(vec![
                ReminderId::Active(5),
                ReminderId::Active(6),
                ReminderId::Active(7),
                ReminderId::Active(8)
            ])
        );

        // Active range with dash
        assert_eq!(
            "5-8".parse::<IdOrRange>().unwrap(),
            IdOrRange(vec![
                ReminderId::Active(5),
                ReminderId::Active(6),
                ReminderId::Active(7),
                ReminderId::Active(8)
            ])
        );

        // Reverse range
        assert_eq!(
            "8-5".parse::<IdOrRange>().unwrap(),
            IdOrRange(vec![
                ReminderId::Active(5),
                ReminderId::Active(6),
                ReminderId::Active(7),
                ReminderId::Active(8)
            ])
        );

        // History range both with prefixes
        assert_eq!(
            "h5-h8".parse::<IdOrRange>().unwrap(),
            IdOrRange(vec![
                ReminderId::History(5),
                ReminderId::History(6),
                ReminderId::History(7),
                ReminderId::History(8)
            ])
        );

        // History range with inherited prefix
        assert_eq!(
            "h5-8".parse::<IdOrRange>().unwrap(),
            IdOrRange(vec![
                ReminderId::History(5),
                ReminderId::History(6),
                ReminderId::History(7),
                ReminderId::History(8)
            ])
        );

        // Capital H history range with inherited prefix
        assert_eq!(
            "H5:8".parse::<IdOrRange>().unwrap(),
            IdOrRange(vec![
                ReminderId::History(5),
                ReminderId::History(6),
                ReminderId::History(7),
                ReminderId::History(8)
            ])
        );

        // Errors: invalid single ID
        assert!("abc".parse::<IdOrRange>().is_err());
        // Errors: invalid bounds
        assert!("5-abc".parse::<IdOrRange>().is_err());
        assert!("abc-5".parse::<IdOrRange>().is_err());
        // Errors: mixed active/history range where history is second but active is first
        assert!("5-h8".parse::<IdOrRange>().is_err());
    }

    // -------------------------------------------------------------------------
    // ID allocation algorithm tests
    // -------------------------------------------------------------------------
    #[test]
    fn test_id_allocator_empty() {
        let reminders: Vec<Reminder> = vec![];

        let mut alloc = IdAllocator::new(&reminders, false);
        assert_eq!(alloc.next_id(), 1); // First clean ID

        let mut history_alloc = IdAllocator::new(&reminders, true);
        assert_eq!(history_alloc.next_id(), 1); // First historical ID
    }

    #[test]
    fn test_id_allocator_with_gaps() {
        // Simulating list: active [1, 3, 4], history [h1, h4]
        let reminders = vec![
            mock_reminder(ReminderId::Active(1), 0, Status::Active),
            mock_reminder(ReminderId::Active(3), 0, Status::Active),
            mock_reminder(ReminderId::Active(4), 0, Status::Active),
            mock_reminder(ReminderId::History(1), 0, Status::Triggered),
            mock_reminder(ReminderId::History(4), 0, Status::Missed),
        ];

        // Check active IDs (gap at 2, then 5)
        let mut active_alloc = IdAllocator::new(&reminders, false);
        assert_eq!(active_alloc.next_id(), 2);
        assert_eq!(active_alloc.next_id(), 5);
        assert_eq!(active_alloc.next_id(), 6);

        // Check history IDs (gaps at h2, h3, then h5)
        let mut history_alloc = IdAllocator::new(&reminders, true);
        assert_eq!(history_alloc.next_id(), 2);
        assert_eq!(history_alloc.next_id(), 3);
        assert_eq!(history_alloc.next_id(), 5);
    }
}
