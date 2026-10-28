//! Runtime configuration: the heap budget and the stack depth limit.
//!
//! The `java` launcher spells these `-Xms`, `-Xmx`, and `-Xss`; the CLI parses them into a
//! [`VmOptions`], and an embedder can build the same value by hand.

/// The heap size at which the collector starts working, in bytes (64 MiB).
///
/// This is the default for `-Xms`: below it the heap grows without collecting, above it the
/// collector keeps the live set small enough for the budget to hold.
pub const DEFAULT_INITIAL_HEAP_BYTES: u64 = 64 * 1024 * 1024;

/// The maximum heap size the launcher uses when `-Xmx` is not given, in bytes (1 GiB).
///
/// HotSpot derives an ergonomic maximum from the machine's memory; oxjvm uses a fixed value so a
/// test run behaves the same on every host. An explicit `-Xmx` overrides it, and embedders that
/// construct a [`VmOptions`] themselves keep the unbounded default.
pub const DEFAULT_MAX_HEAP_BYTES: u64 = 1024 * 1024 * 1024;

/// The default maximum number of Java frames on a thread.
pub const DEFAULT_MAX_STACK_FRAMES: usize = 65_536;

/// The stack space one Java frame is charged, in bytes.
///
/// The real `-Xss` is a byte count while oxjvm's limit is a frame count, so the launcher
/// converts between the two with this constant.
pub const STACK_FRAME_BYTES: u64 = 1024;

/// The runtime configuration of a [`Vm`](crate::Vm).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmOptions {
    /// The heap size at which the collector starts working (`-Xms`).
    ///
    /// `None` means the default, [`DEFAULT_INITIAL_HEAP_BYTES`]. The value is capped by
    /// `max_heap_bytes`, so a maximum below the default still collects at the maximum. The
    /// collector's working size grows as the live set demands, up to the maximum.
    pub initial_heap_bytes: Option<u64>,
    /// The maximum heap size (`-Xmx`); `None` means unbounded.
    pub max_heap_bytes: Option<u64>,
    /// The maximum number of Java frames on a thread (`-Xss`).
    pub max_stack_frames: usize,
}

impl Default for VmOptions {
    fn default() -> Self {
        Self {
            initial_heap_bytes: None,
            max_heap_bytes: None,
            max_stack_frames: DEFAULT_MAX_STACK_FRAMES,
        }
    }
}

impl VmOptions {
    /// Options with the given maximum heap size, in bytes.
    #[must_use]
    pub fn with_max_heap_bytes(max_heap_bytes: u64) -> Self {
        Self {
            max_heap_bytes: Some(max_heap_bytes),
            ..Self::default()
        }
    }

    /// Options with the given initial and maximum heap sizes, in bytes.
    #[must_use]
    pub fn with_heap_sizes(initial_heap_bytes: u64, max_heap_bytes: u64) -> Self {
        Self {
            initial_heap_bytes: Some(initial_heap_bytes),
            max_heap_bytes: Some(max_heap_bytes),
            ..Self::default()
        }
    }

    /// The heap size at which the collector starts working.
    ///
    /// This is `-Xms`, capped by `-Xmx`: a maximum below the initial size means the collector
    /// runs as soon as the heap is full.
    #[must_use]
    pub fn commit_bytes(&self) -> u64 {
        let initial = self
            .initial_heap_bytes
            .unwrap_or(DEFAULT_INITIAL_HEAP_BYTES);
        match self.max_heap_bytes {
            Some(max) => initial.min(max),
            None => initial,
        }
    }

    /// The maximum heap size, or `None` when the heap is unbounded.
    #[must_use]
    pub const fn max_heap_bytes(&self) -> Option<u64> {
        self.max_heap_bytes
    }
}

/// Parse a memory size such as `512`, `64k`, `256m`, or `2g` into bytes.
///
/// The suffixes are the ones the `java` launcher accepts: `k`/`K` is 1024 bytes, `m`/`M` is
/// 1024 × 1024, and `g`/`G` is 1024 × 1024 × 1024. A bare number is a byte count.
///
/// Returns `None` when the text is not a size the launcher would accept.
#[must_use]
pub fn parse_memory_size(text: &str) -> Option<u64> {
    let text = text.trim();
    let (digits, multiplier) = match text.as_bytes().last() {
        Some(b'k' | b'K') => (&text[..text.len() - 1], 1024u64),
        Some(b'm' | b'M') => (&text[..text.len() - 1], 1024 * 1024),
        Some(b'g' | b'G') => (&text[..text.len() - 1], 1024 * 1024 * 1024),
        _ => (text, 1),
    };
    let value: u64 = digits.trim().parse().ok()?;
    value.checked_mul(multiplier)
}

#[cfg(test)]
mod tests {
    use super::{VmOptions, parse_memory_size};

    #[test]
    fn parses_plain_byte_counts() {
        assert_eq!(parse_memory_size("512"), Some(512));
        assert_eq!(parse_memory_size(" 512 "), Some(512));
    }

    #[test]
    fn parses_the_kilo_mega_and_giga_suffixes() {
        assert_eq!(parse_memory_size("64k"), Some(64 * 1024));
        assert_eq!(parse_memory_size("64K"), Some(64 * 1024));
        assert_eq!(parse_memory_size("256m"), Some(256 * 1024 * 1024));
        assert_eq!(parse_memory_size("256M"), Some(256 * 1024 * 1024));
        assert_eq!(parse_memory_size("2g"), Some(2 * 1024 * 1024 * 1024));
        assert_eq!(parse_memory_size("2G"), Some(2 * 1024 * 1024 * 1024));
    }

    #[test]
    fn rejects_sizes_the_launcher_would_not_accept() {
        assert_eq!(parse_memory_size(""), None);
        assert_eq!(parse_memory_size("m"), None);
        assert_eq!(parse_memory_size("12x"), None);
        assert_eq!(parse_memory_size("-1"), None);
        assert_eq!(parse_memory_size("1.5m"), None);
    }

    #[test]
    fn the_committed_size_is_capped_by_the_maximum() {
        let options = VmOptions::with_heap_sizes(1024, 512);
        assert_eq!(options.commit_bytes(), 512);
        let options = VmOptions::with_heap_sizes(512, 1024);
        assert_eq!(options.commit_bytes(), 512);
        let options = VmOptions::with_max_heap_bytes(512);
        assert_eq!(options.commit_bytes(), 512);
    }
}
