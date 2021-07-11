//! Basic platform detection.
//!
//! This module provides basic functionality for detecting the underlying
//! platform.

/// Returns an identifier for the underlying operating system.
pub fn operating_system() -> i64 {
    if cfg!(target_os = "windows") {
        0
    } else if cfg!(target_os = "macos") {
        1
    } else if cfg!(target_os = "ios") {
        2
    } else if cfg!(target_os = "linux") {
        3
    } else if cfg!(target_os = "android") {
        4
    } else if cfg!(target_os = "freebsd") {
        5
    } else if cfg!(target_os = "dragonfly") {
        6
    } else if cfg!(target_os = "bitrig") {
        7
    } else if cfg!(target_os = "openbsd") {
        8
    } else if cfg!(target_os = "netbsd") {
        9
    } else if cfg!(unix) {
        10
    } else {
        11
    }
}

#[cfg(test)]
mod tests {
    macro_rules! test_operating_system {
        ($platform: expr, $code: expr) => {
            #[cfg(target_os = $platform)]
            #[test]
            fn test_operating_system() {
                assert_eq!(super::operating_system(), $code);
            }
        };
    }

    test_operating_system!("windows", 0);
    test_operating_system!("macos", 1);
    test_operating_system!("ios", 2);
    test_operating_system!("linux", 3);
    test_operating_system!("android", 4);
    test_operating_system!("freebsd", 5);
    test_operating_system!("dragonfly", 6);
    test_operating_system!("bitrig", 7);
    test_operating_system!("openbsd", 8);
    test_operating_system!("netbsd", 9);

    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "bitrig",
        target_os = "openbsd",
        target_os = "netbsd"
    )))]
    #[test]
    fn test_operating_system() {
        assert_eq!(super::operating_system(), 11);
    }
}
