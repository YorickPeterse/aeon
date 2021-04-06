use std::time::Duration;

/// Converts a Duration to the time in seconds as an f64.
pub fn to_f64(value: Option<Duration>) -> f64 {
    value
        .map(|duration| {
            duration.as_secs() as f64
                + (f64::from(duration.subsec_nanos()) / 1_000_000_000.0)
        })
        .unwrap_or(0.0)
}

/// Converts an f64 (in seconds) to a Duration.
pub fn from_f64(value: f64) -> Option<Duration> {
    if value <= 0.0 {
        None
    } else {
        let secs = value.trunc() as u64;
        let nanos = (value.fract() * 1_000_000_000.0) as u32;

        Some(Duration::new(secs, nanos))
    }
}
