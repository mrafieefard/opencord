//! HTTP Range header parsing (RFC 9110 §14.1.2).

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid range")]
pub struct RangeError;

/// Parses an optional HTTP `Range` header for a resource of `len` bytes.
///
/// Returns:
/// - `Ok(None)` if no header was supplied.
/// - `Ok(Some((start, end)))` with inclusive start and end offsets.
/// - `Err(RangeError)` if `len == 0`, the header is malformed, has multiple ranges,
///   or requests an unsatisfiable range.
pub fn parse(header: Option<&str>, len: u64) -> Result<Option<(u64, u64)>, RangeError> {
    let header = match header {
        Some(h) => h.trim(),
        None => return Ok(None),
    };

    if len == 0 || header.contains(',') {
        return Err(RangeError);
    }

    let spec = header.strip_prefix("bytes=").ok_or(RangeError)?;
    let (start_str, end_str) = spec.split_once('-').ok_or(RangeError)?;

    if start_str.is_empty() {
        // Suffix range: bytes=-n
        if end_str.is_empty() {
            return Err(RangeError);
        }
        let n: u64 = end_str.parse().map_err(|_| RangeError)?;
        if n == 0 {
            return Err(RangeError);
        }
        if n >= len {
            Ok(Some((0, len - 1)))
        } else {
            Ok(Some((len - n, len - 1)))
        }
    } else if end_str.is_empty() {
        // Range from start: bytes=a-
        let a: u64 = start_str.parse().map_err(|_| RangeError)?;
        if a >= len {
            return Err(RangeError);
        }
        Ok(Some((a, len - 1)))
    } else {
        // Range: bytes=a-b
        let a: u64 = start_str.parse().map_err(|_| RangeError)?;
        let b: u64 = end_str.parse().map_err(|_| RangeError)?;
        if a > b || a >= len {
            return Err(RangeError);
        }
        let b = b.min(len - 1);
        Ok(Some((a, b)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_edges() {
        assert_eq!(parse(None, 10), Ok(None));
        assert_eq!(parse(Some("bytes=0-0"), 0), Err(RangeError));
        assert_eq!(parse(Some("bytes=0-0"), 5), Ok(Some((0, 0))));
        assert_eq!(parse(Some("bytes=-5"), 3), Ok(Some((0, 2))));
        assert_eq!(parse(Some("bytes=5-"), 5), Err(RangeError));
        assert_eq!(parse(Some("bytes=0-1,3-4"), 10), Err(RangeError));
        assert_eq!(parse(Some("bytes=0-10"), 5), Ok(Some((0, 4))));
        assert_eq!(parse(Some("bytes=2-1"), 5), Err(RangeError));
        assert_eq!(parse(Some("not-bytes=0-0"), 5), Err(RangeError));
        assert_eq!(parse(Some("bytes=-"), 5), Err(RangeError));
        assert_eq!(parse(Some("bytes=-0"), 5), Err(RangeError));
        assert_eq!(parse(Some("bytes=2-"), 5), Ok(Some((2, 4))));
    }
}
