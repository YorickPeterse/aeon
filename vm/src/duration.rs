use std::time::Duration;

/// Converts a Duration to the time in seconds as an f64.
pub fn to_f64(duration: Duration) -> f64 {
    duration.as_secs() as f64
        + (f64::from(duration.subsec_nanos()) / 1_000_000_000.0)
}

/// Converts an f64 (in seconds) to a Duration.
pub fn from_f64(value: f64) -> Duration {
    let secs = value.trunc() as u64;
    let nanos = (value.fract() * 1_000_000_000.0) as u32;

    Duration::new(secs, nanos)
}
