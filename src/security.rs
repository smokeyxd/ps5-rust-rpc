pub fn is_valid_title_id(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 9
        && b[..4].iter().all(u8::is_ascii_uppercase)
        && b[4..].iter().all(u8::is_ascii_digit)
}

pub fn is_retail_title_id(s: &str) -> bool {
    is_valid_title_id(s) && (s.starts_with("CUSA") || s.starts_with("PPSA"))
}

pub fn is_safe_https_url(s: &str) -> bool {
    const PREFIX: &str = "https://";
    if s.len() < PREFIX.len() + 3 || s.len() > 1024 {
        return false;
    }
    if !s.starts_with(PREFIX) {
        return false;
    }
    if s.bytes().any(|c| c < 0x20 || c == 0x7f) {
        return false;
    }
    let host = s[PREFIX.len()..].split(['/', '?', '#']).next().unwrap_or("");
    !host.is_empty() && !host.contains('@') && !host.contains(' ') && !host.contains('\\')
}

pub fn sanitize_text(s: &str, max_chars: usize) -> String {
    let cleaned: String = s.chars().filter(|c| !c.is_control()).collect();
    cleaned.trim().chars().take(max_chars).collect()
}

pub fn is_valid_app_id(s: &str) -> bool {
    (17..=20).contains(&s.len()) && s.bytes().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_ids() {
        assert!(is_valid_title_id("CUSA00265"));
        assert!(is_valid_title_id("NPXS39041"));
        assert!(!is_valid_title_id("cusa00265"));
        assert!(!is_valid_title_id("CUSA0026"));
        assert!(!is_valid_title_id("CUSA0026X"));
        assert!(!is_valid_title_id("../../etc"));
        assert!(is_retail_title_id("PPSA01234"));
        assert!(!is_retail_title_id("NPXS39041"));
    }

    #[test]
    fn urls() {
        assert!(is_safe_https_url("https://orbispatches.com/CUSA00265"));
        assert!(!is_safe_https_url("http://x.com"));
        assert!(!is_safe_https_url("https://evil.com@good.com/a"));
        assert!(!is_safe_https_url("javascript:alert(1)"));
        assert!(!is_safe_https_url("https://x.com/\n"));
    }

    #[test]
    fn app_ids() {
        assert!(is_valid_app_id("123456789012345678"));
        assert!(!is_valid_app_id("12345"));
        assert!(!is_valid_app_id("12345678901234567a"));
    }
}
