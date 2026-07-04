//! Sliding-window materialization.
//!
//! Ported from `c_engine/src/sliding.c`. Produces the set of `[t, t+within]`
//! windows spaced by `slide` inside the range `[start, end]`.
//!
//! ## Semantics
//!
//! For each `t` in `start, start+slide, start+2*slide, …` such that
//! `t + within <= end`, one window `(t, t+within)` is emitted. This matches
//! the C engine's condition (`sliding.c:5`) so downstream consumers see the
//! same tuples.
//!
//! ## Bug fixes over the C engine
//!
//! - **Non-positive `slide` is rejected.** The C code would `t += slide`
//!   forever on `slide == 0` and produce garbage on `slide < 0`. Here we
//!   return [`WindowError::NonPositiveSlide`] explicitly.
//! - **Non-positive `within` is rejected** for symmetry — a zero-length
//!   window is not a useful downstream input, and negative values would
//!   produce reversed tuples.

use thiserror::Error;

/// Errors returned by [`materialize_windows`].
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum WindowError {
    /// `slide` was zero or negative.
    #[error("slide must be positive; got {0}")]
    NonPositiveSlide(i64),
    /// `within` was zero or negative.
    #[error("within must be positive; got {0}")]
    NonPositiveWithin(i64),
}

/// Materialize the set of `(t, t + within)` sliding windows over
/// `[start, end]`.
///
/// Returns an empty vector when `end < start + within` (no window fits).
///
/// `cap` caps the number of emitted windows. Pass [`usize::MAX`] for no cap.
pub fn materialize_windows(
    start: i64,
    end: i64,
    within: i64,
    slide: i64,
    cap: usize,
) -> Result<Vec<(i64, i64)>, WindowError> {
    if slide <= 0 {
        return Err(WindowError::NonPositiveSlide(slide));
    }
    if within <= 0 {
        return Err(WindowError::NonPositiveWithin(within));
    }

    let mut out: Vec<(i64, i64)> = Vec::new();
    if end < start {
        return Ok(out);
    }

    let mut t = start;
    while out.len() < cap {
        // Use `checked_add` so we don't UB on i64 overflow near i64::MAX.
        let stop = match t.checked_add(within) {
            Some(x) => x,
            None => break,
        };
        if stop > end {
            break;
        }
        out.push((t, stop));
        t = match t.checked_add(slide) {
            Some(x) => x,
            None => break,
        };
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_case_matches_c_engine() {
        // The C test in c_engine/tests/test_main.c uses (0, 100, 20, 10)
        // and expects windows starting with (0, 20). Assert full behavior.
        let w = materialize_windows(0, 100, 20, 10, 64).unwrap();
        assert!(!w.is_empty());
        assert_eq!(w[0], (0, 20));
        // Last window must fit.
        let &(_t, e) = w.last().unwrap();
        assert!(e <= 100);
        // Spacing is slide.
        for pair in w.windows(2) {
            assert_eq!(pair[1].0 - pair[0].0, 10);
        }
    }

    #[test]
    fn non_positive_slide_rejected() {
        assert_eq!(materialize_windows(0, 100, 20, 0, 64), Err(WindowError::NonPositiveSlide(0)));
        assert_eq!(materialize_windows(0, 100, 20, -1, 64), Err(WindowError::NonPositiveSlide(-1)));
    }

    #[test]
    fn non_positive_within_rejected() {
        assert_eq!(materialize_windows(0, 100, 0, 10, 64), Err(WindowError::NonPositiveWithin(0)));
        assert_eq!(
            materialize_windows(0, 100, -5, 10, 64),
            Err(WindowError::NonPositiveWithin(-5))
        );
    }

    #[test]
    fn slide_larger_than_within_produces_gaps() {
        let w = materialize_windows(0, 100, 5, 20, 64).unwrap();
        // Windows: (0,5), (20,25), (40,45), (60,65), (80,85). No overlap.
        assert_eq!(w, vec![(0, 5), (20, 25), (40, 45), (60, 65), (80, 85)]);
    }

    #[test]
    fn slide_equal_to_within_produces_adjacent_windows() {
        let w = materialize_windows(0, 100, 25, 25, 64).unwrap();
        assert_eq!(w, vec![(0, 25), (25, 50), (50, 75), (75, 100)]);
    }

    #[test]
    fn slide_smaller_than_within_produces_overlap() {
        let w = materialize_windows(0, 100, 40, 10, 64).unwrap();
        // First window (0,40), stride 10, last window must have stop <= 100.
        assert_eq!(w[0], (0, 40));
        assert_eq!(w[1], (10, 50));
        let &(_, e) = w.last().unwrap();
        assert!(e <= 100);
    }

    #[test]
    fn empty_when_within_exceeds_range() {
        let w = materialize_windows(0, 10, 50, 5, 64).unwrap();
        assert!(w.is_empty());
    }

    #[test]
    fn empty_when_end_less_than_start() {
        let w = materialize_windows(100, 0, 10, 5, 64).unwrap();
        assert!(w.is_empty());
    }

    #[test]
    fn cap_is_honored() {
        let w = materialize_windows(0, 1000, 5, 5, 3).unwrap();
        assert_eq!(w.len(), 3);
    }

    #[test]
    fn i64_overflow_does_not_panic() {
        // t + within would overflow near i64::MAX; the loop must simply stop
        // (using checked_add) instead of overflowing or panicking.
        let w = materialize_windows(i64::MAX - 5, i64::MAX, 3, 2, 64).unwrap();
        // First window (max-5, max-2). Next t = max-3, stop = max, still fits.
        // Next t = max-1, stop would overflow — must terminate cleanly.
        assert!(!w.is_empty(), "expected at least one window before overflow");
        for &(t, stop) in &w {
            assert!(t >= i64::MAX - 5);
            assert!(stop - t == 3);
        }
    }
}
