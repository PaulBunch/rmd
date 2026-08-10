#[cfg(test)]
mod tests {
    use crate::IdAllocator;
    use crate::Reminder;
    use crate::ReminderId;
    use crate::Status;
    use crate::sync_on_startup;

    // Helper for quick creation of mock reminders in tests
    fn mock_reminder(id: ReminderId, trigger_at: i64, status: Status) -> Reminder {
        Reminder {
            id,
            message: "Test".to_string(),
            trigger_at,
            status,
        }
    }

    // =========================================================================
    // 1. ReminderId parsing and formatting tests
    // =========================================================================
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

    // =========================================================================
    // 2. ID allocation algorithm tests
    // =========================================================================
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

    // =========================================================================
    // 3. Integration test: Active -> History transition
    // =========================================================================
    #[test]
    fn test_transition_active_to_history() {
        let now = chrono::Utc::now().timestamp();

        let mut reminders = vec![
            // Overdue active reminder (should be transitioned to history)
            mock_reminder(ReminderId::Active(1), now - 1000, Status::Active),
            // Up-to-date active reminder (should remain unchanged)
            mock_reminder(ReminderId::Active(2), now + 1000, Status::Active),
            // Existing history
            mock_reminder(ReminderId::History(1), now - 2000, Status::Triggered),
        ];

        // Invoke the lifecycle function (which should transition Active 1 into Missed/History 2)
        sync_on_startup(&mut reminders);

        // Check that the first reminder updated status and received the smallest h-ID (h2)
        assert_eq!(reminders[0].status, Status::Missed);
        assert_eq!(reminders[0].id, ReminderId::History(2));

        // Check that the second reminder was left untouched
        assert_eq!(reminders[1].status, Status::Active);
        assert_eq!(reminders[1].id, ReminderId::Active(2));

        // Check that old history was left untouched
        assert_eq!(reminders[2].status, Status::Triggered);
        assert_eq!(reminders[2].id, ReminderId::History(1));
    }
}
