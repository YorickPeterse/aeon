//! Virtual Machine Configuration
//!
//! Various virtual machine settings that can be changed by the user, such as
//! the number of threads to run.
use std::cmp::min;
use std::env;

/// Sets a configuration field based on an environment variable.
macro_rules! set_from_env {
    ($config:expr, $field:ident, $key:expr, $value_type:ty) => {{
        if let Ok(raw_value) = env::var(concat!("INKO_", $key)) {
            if let Ok(value) = raw_value.parse::<$value_type>() {
                $config.$field = value;
            }
        };
    }};
}

const DEFAULT_REDUCTIONS: usize = 1000;

/// The maximum time (in nanoseconds) we want to spend on scanning blocks in a
/// single recycling scan.
///
/// This number is quite conservative, as at the time of writing it wasn't clear
/// yet if it would be noticeable if we increased this number.
const MAXIMUM_TIME_PER_RECYCLING_SCAN: usize = 20_000;

/// The estimated time (in nanoseconds) spent on scanning a single block.
///
/// This number is a rough estimate based on some simple measurements of
/// iterating over 4096 blocks and checking their reusable line counts.
const ESTIMATED_TIME_PER_BLOCK: usize = 20;

/// The maximum number of blocks to scan in a single reclycing scan.
///
/// The time spent per block is expected to be roughly 20-30 nanoseconds. With a
/// limit of 10 microseconds per scan, we should be able to scan up to 500
/// blocks (about 15 MB of memory) in a single recycling scan cycle.
///
/// Note that this is just an upper limit. If a scan manages to free up enough
/// lines before reaching this limit, it will stop earlier.
const DEFAULT_RECYCLE_BLOCKS_PER_SCAN: usize =
    MAXIMUM_TIME_PER_RECYCLING_SCAN / ESTIMATED_TIME_PER_BLOCK;

/// Structure containing the configuration settings for the virtual machine.
pub struct Config {
    /// The number of primary process threads to run.
    ///
    /// This defaults to the number of CPU cores.
    pub primary_threads: u16,

    /// The number of blocking process threads to run.
    ///
    /// This defaults to the number of CPU cores.
    pub blocking_threads: u16,

    /// The number of threads to use for parsing bytecode images.
    ///
    /// This defaults to 4 cores, or the number of cores itself if this is lower
    /// than or equal to 4.
    ///
    /// When implementing parallel parsing of bytecode images, we investigated
    /// what default would be best. For this test we loaded the image used to
    /// run all of Inko's standard library tests. The CPUs used for this test
    /// were:
    ///
    /// * Ryzen 1600X, with 6 cores and 12 threads
    /// * Intel i5-8265U, with 4 cores and 8 threads
    ///
    /// On both systems we found that 4 cores gives the best performance, with 8
    /// cores performing slightly worse (but not much). For programs that
    /// involve images containing thousands of modules, increasing the number of
    /// cores may reduce the time it takes to parse the image.
    pub bytecode_threads: u16,

    /// The number of reductions a process can perform before being suspended.
    /// Defaults to 1000.
    pub reductions: usize,

    /// The maximum number of blocks to scan in a single recycling scan.
    pub recycle_blocks_per_scan: usize,
}

impl Config {
    pub fn new() -> Config {
        let cpu_count = num_cpus::get();

        Config {
            primary_threads: cpu_count as u16,
            blocking_threads: cpu_count as u16,
            bytecode_threads: min(4, cpu_count) as u16,
            reductions: DEFAULT_REDUCTIONS,
            recycle_blocks_per_scan: DEFAULT_RECYCLE_BLOCKS_PER_SCAN,
        }
    }

    /// Populates configuration settings based on environment variables.
    pub fn populate_from_env(&mut self) {
        set_from_env!(self, primary_threads, "PRIMARY_THREADS", u16);
        set_from_env!(self, blocking_threads, "BLOCKING_THREADS", u16);
        set_from_env!(self, bytecode_threads, "BYTECODE_THREADS", u16);
        set_from_env!(self, reductions, "REDUCTIONS", usize);
        set_from_env!(
            self,
            recycle_blocks_per_scan,
            "RECYCLE_BLOCKS_PER_SCAN",
            usize
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_new() {
        let config = Config::new();

        assert!(config.primary_threads >= 1);
        assert_eq!(config.reductions, 1000);
    }

    #[test]
    fn test_populate_from_env() {
        env::set_var("INKO_PRIMARY_THREADS", "42");

        let mut config = Config::new();

        config.populate_from_env();

        // Unset before any assertions may fail.
        env::remove_var("INKO_PRIMARY_THREADS");

        assert_eq!(config.primary_threads, 42);
    }
}
