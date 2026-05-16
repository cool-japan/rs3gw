/// Sanitize a key or version string into a filesystem-safe filename component.
///
/// Replaces characters that are problematic on common filesystems with `_`.
pub(crate) fn sanitize_key_for_fs(key: &str) -> String {
    key.replace(['/', '\\', ':', '?', '*', '"', '<', '>', '|'], "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_key_for_fs_replaces_slash() {
        assert_eq!(sanitize_key_for_fs("foo/bar/baz"), "foo_bar_baz");
    }

    #[test]
    fn test_sanitize_key_for_fs_replaces_special_chars() {
        assert_eq!(sanitize_key_for_fs("a:b?c*d"), "a_b_c_d");
    }

    #[test]
    fn test_sanitize_key_for_fs_plain_key() {
        assert_eq!(sanitize_key_for_fs("simple-key"), "simple-key");
    }
}
