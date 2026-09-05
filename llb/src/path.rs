//! POSIX path helpers matching Go's `path` package semantics.

pub(crate) fn is_abs(path: &str) -> bool {
    path.starts_with('/')
}

pub(crate) fn clean(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }

    let absolute = is_abs(path);
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|part| *part != "..") => {
                parts.pop();
            }
            ".." if !absolute => parts.push(".."),
            ".." => {}
            part => parts.push(part),
        }
    }

    let result = parts.join("/");
    if absolute {
        if result.is_empty() {
            "/".to_string()
        } else {
            format!("/{result}")
        }
    } else if result.is_empty() {
        ".".to_string()
    } else {
        result
    }
}

pub(crate) fn join(parts: &[&str]) -> String {
    let joined = parts
        .iter()
        .filter(|part| !part.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("/");
    if joined.is_empty() {
        String::new()
    } else {
        clean(&joined)
    }
}

pub(crate) fn normalize(parent: &str, path: &str, keep_slash: bool) -> String {
    let original = path;
    let path = clean(path);
    let path = if is_abs(&path) {
        path
    } else {
        join(&["/", parent, &path])
    };

    if keep_slash {
        if original.ends_with('/') && !path.ends_with('/') {
            format!("{path}/")
        } else if original.ends_with("/.") {
            if path == "/" {
                "/.".to_string()
            } else {
                format!("{path}/.")
            }
        } else {
            path
        }
    } else {
        path
    }
}

pub(crate) fn copy_source(source_cwd: Option<&str>, source: &str) -> String {
    let source = clean(source);
    if is_abs(&source) {
        join(&["/", &source])
    } else {
        join(&[source_cwd.unwrap_or(""), &source])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_matches_go_path_clean_for_common_inputs() {
        let cases = [
            ("", "."),
            (".", "."),
            ("..", ".."),
            ("a//b/../c", "a/c"),
            ("/a/../../b", "/b"),
            ("//a///b", "/a/b"),
        ];
        for (input, expected) in cases {
            assert_eq!(clean(input), expected, "input {input:?}");
        }
    }

    #[test]
    fn join_matches_go_path_join_for_common_inputs() {
        assert_eq!(join(&[]), "");
        assert_eq!(join(&["", "foo"]), "foo");
        assert_eq!(join(&["/", "/work", "foo"]), "/work/foo");
        assert_eq!(join(&["/work", "../foo"]), "/foo");
    }

    #[test]
    fn normalize_preserves_copy_destination_suffixes() {
        assert_eq!(normalize("/work", "..", false), "/");
        assert_eq!(normalize("/work", "../foo", true), "/foo");
        assert_eq!(normalize("/work", "a//b/", true), "/work/a/b/");
        assert_eq!(normalize("/work", "dest/", true), "/work/dest/");
        assert_eq!(normalize("/work", "dest/.", true), "/work/dest/.");
        assert_eq!(normalize("/work", "./.", true), "/work/.");
    }

    #[test]
    fn copy_source_preserves_unset_cwd_as_relative() {
        assert_eq!(copy_source(None, "foo"), "foo");
        assert_eq!(copy_source(Some("/ced"), "foo"), "/ced/foo");
        assert_eq!(copy_source(None, "/foo"), "/foo");
    }
}
