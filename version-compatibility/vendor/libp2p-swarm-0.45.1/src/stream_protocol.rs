use either::Either;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Identifies a protocol for a stream.
///
/// libp2p nodes use stream protocols to negotiate what to do with a newly opened stream.
/// Stream protocols are string-based and must start with a forward slash: `/`.
#[derive(Clone, Eq)]
pub struct StreamProtocol {
    inner: Either<&'static str, Arc<str>>,
}

impl StreamProtocol {
    /// Construct a new protocol from a static string slice.
    ///
    /// # Panics
    ///
    /// This function panics if the protocol does not start with a forward slash: `/`.
    pub const fn new(s: &'static str) -> Self {
        match s.as_bytes() {
            [b'/', ..] => {}
            _ => panic!("Protocols should start with a /"),
        }

        StreamProtocol {
            inner: Either::Left(s),
        }
    }

    /// Attempt to construct a protocol from an owned string.
    ///
    /// This function will fail if the protocol does not start with a forward slash: `/`.
    /// Where possible, you should use [`StreamProtocol::new`] instead to avoid allocations.
    pub fn try_from_owned(protocol: String) -> Result<Self, InvalidProtocol> {
        if !protocol.starts_with('/') {
            return Err(InvalidProtocol::missing_forward_slash());
        }

        Ok(StreamProtocol {
            inner: Either::Right(Arc::from(protocol)), // FIXME: Can we somehow reuse the allocation from the owned string?
        })
    }
}

impl AsRef<str> for StreamProtocol {
    fn as_ref(&self) -> &str {
        either::for_both!(&self.inner, s => s)
    }
}

impl fmt::Debug for StreamProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        either::for_both!(&self.inner, s => s.fmt(f))
    }
}

impl fmt::Display for StreamProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl PartialEq<&str> for StreamProtocol {
    fn eq(&self, other: &&str) -> bool {
        self.as_ref() == *other
    }
}

impl PartialEq<StreamProtocol> for &str {
    fn eq(&self, other: &StreamProtocol) -> bool {
        *self == other.as_ref()
    }
}

impl PartialEq for StreamProtocol {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl Hash for StreamProtocol {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state)
    }
}

#[derive(Debug)]
pub struct InvalidProtocol {
    // private field to prevent construction outside of this module
    _private: (),
}

impl InvalidProtocol {
    pub(crate) fn missing_forward_slash() -> Self {
        InvalidProtocol { _private: () }
    }
}

impl fmt::Display for InvalidProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid protocol: string does not start with a forward slash"
        )
    }
}

impl std::error::Error for InvalidProtocol {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_protocol_print() {
        let protocol = StreamProtocol::new("/foo/bar/1.0.0");

        let debug = format!("{protocol:?}");
        let display = format!("{protocol}");

        assert_eq!(
            debug, r#""/foo/bar/1.0.0""#,
            "protocol to debug print as string with quotes"
        );
        assert_eq!(
            display, "/foo/bar/1.0.0",
            "protocol to display print as string without quotes"
        );
    }
}
