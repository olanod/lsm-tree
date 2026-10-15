// Copyright (c) 2024-present, fjall-rs
// This source code is licensed under both the Apache 2.0 and MIT License
// (found in the LICENSE-* files in the repository)

/// Gets the unix timestamp as a duration
pub fn unix_timestamp() -> std::time::Duration {
    #[cfg(test)]
    if let Some(override_val) = NOW_OVERRIDE.with(std::cell::Cell::get) {
        return override_val;
    }

    let now = std::time::SystemTime::now();

    #[expect(clippy::expect_used, reason = "trivial")]
    now.duration_since(std::time::SystemTime::UNIX_EPOCH)
        .expect("time went backwards")
}

// NOTE: Per thread, because tests run in parallel, and one test's clock
// must not leak into another test's tables
#[cfg(test)]
thread_local! {
    static NOW_OVERRIDE: std::cell::Cell<Option<std::time::Duration>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub fn set_unix_timestamp_for_test(value: Option<std::time::Duration>) {
    NOW_OVERRIDE.with(|cell| cell.set(value));
}

#[cfg(test)]
#[expect(clippy::expect_used)]
mod tests {
    use super::*;
    use std::time::Duration;
    use test_log::test;

    #[test]
    fn unix_timestamp_override_is_per_thread() {
        let frozen = Duration::from_secs(1_000);

        set_unix_timestamp_for_test(Some(frozen));
        assert_eq!(frozen, unix_timestamp());

        let other_thread = std::thread::spawn(unix_timestamp)
            .join()
            .expect("thread should not panic");
        assert_ne!(frozen, other_thread);

        set_unix_timestamp_for_test(None);
        assert_ne!(frozen, unix_timestamp());
    }
}
