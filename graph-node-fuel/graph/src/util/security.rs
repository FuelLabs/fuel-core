use std::fmt;
use url::Url;

/// Helper function to redact passwords from URLs
fn display_url(url: &str) -> String {
    let mut url = match Url::parse(url) {
        Ok(url) => url,
        Err(_) => return String::from(url),
    };

    if url.password().is_some() {
        url.set_password(Some("HIDDEN_PASSWORD"))
            .expect("failed to redact password");
    }

    String::from(url)
}

pub struct SafeDisplay<T>(pub T);

impl<T: fmt::Display> fmt::Display for SafeDisplay<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // First, format the inner value
        let inner = format!("{}", self.0);

        // Then, make sure to redact passwords from the inner string
        write!(f, "{}", display_url(inner.as_str()))
    }
}

impl<T: fmt::Display> slog::Value for SafeDisplay<T> {
    fn serialize(
        &self,
        _rec: &slog::Record,
        key: slog::Key,
        serializer: &mut dyn slog::Serializer,
    ) -> slog::Result {
        serializer.emit_str(key, format!("{}", self).as_str())
    }
}
