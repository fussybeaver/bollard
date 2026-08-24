use regex::Regex;

#[derive(Clone, Debug)]
pub(crate) struct PatternMatcher {
    patterns: Vec<Pattern>,
}

#[derive(Clone, Debug)]
struct Pattern {
    exclusion: bool,
    raw: String,
    regex: Regex,
}

impl PatternMatcher {
    pub(crate) fn new(patterns: &[String]) -> Result<Option<Self>, String> {
        let mut compiled = Vec::new();
        for value in patterns {
            let mut value = value.trim().to_owned();
            if value.is_empty() {
                continue;
            }
            let exclusion = value.starts_with('!');
            if exclusion {
                value.remove(0);
                if value.is_empty() {
                    return Err("illegal exclusion pattern: \"!\"".to_owned());
                }
            }
            value = clean_pattern(&value)?;
            if value.is_empty() {
                return Err("empty pattern after exclusion marker".to_owned());
            }
            compiled.push(Pattern {
                exclusion,
                regex: compile_pattern(&value)?,
                raw: value,
            });
        }
        Ok((!compiled.is_empty()).then_some(Self { patterns: compiled }))
    }

    pub(crate) fn matches_or_parent(&self, path: &str) -> bool {
        let mut matched = false;
        for pattern in &self.patterns {
            if pattern.exclusion != matched {
                continue;
            }
            let mut current = path;
            let mut match_found = false;
            loop {
                if pattern.regex.is_match(current) {
                    match_found = true;
                    break;
                }
                if current.is_empty() {
                    break;
                }
                current = current.rsplit_once('/').map_or("", |(parent, _)| parent);
            }
            if match_found {
                matched = !pattern.exclusion;
            }
        }
        matched
    }

    pub(crate) fn patterns(&self) -> impl Iterator<Item = (&str, bool)> {
        self.patterns
            .iter()
            .map(|pattern| (pattern.raw.as_str(), pattern.exclusion))
    }
}

pub(crate) fn match_component(pattern: &str, value: &str) -> bool {
    compile_component(pattern).is_some_and(|pattern| pattern.matches(value))
}

pub(crate) fn compile_component(pattern: &str) -> Option<glob::Pattern> {
    let pattern = if cfg!(not(windows)) {
        let mut normalized = String::new();
        let mut characters = pattern.chars();
        while let Some(character) = characters.next() {
            if character == '\\' {
                if let Some(escaped) = characters.next() {
                    normalized.push_str(&glob::Pattern::escape(&escaped.to_string()));
                } else {
                    normalized.push_str(&glob::Pattern::escape("\\"));
                }
            } else {
                normalized.push(character);
            }
        }
        normalized
    } else {
        pattern.to_owned()
    };
    glob::Pattern::new(&pattern).ok()
}

fn clean_pattern(value: &str) -> Result<String, String> {
    let mut result = Vec::new();
    let normalized = if cfg!(windows) {
        value.replace('\\', "/")
    } else {
        value.to_owned()
    };
    for component in normalized.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                result.pop();
            }
            component => result.push(component),
        }
    }
    Ok(result.join("/"))
}

fn compile_pattern(pattern: &str) -> Result<Regex, String> {
    let mut expression = String::from("^");
    let chars: Vec<char> = pattern.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            '*' if chars.get(index + 1) == Some(&'*') => {
                index += 2;
                if chars.get(index) == Some(&'/') {
                    index += 1;
                    expression.push_str("(?:.*/)?");
                } else {
                    expression.push_str(".*");
                }
            }
            '*' => {
                expression.push_str("[^/]*");
                index += 1;
            }
            '?' => {
                expression.push_str("[^/]");
                index += 1;
            }
            '[' => {
                let start = index;
                index += 1;
                if chars.get(index) == Some(&'!') || chars.get(index) == Some(&'^') {
                    index += 1;
                }
                if chars.get(index) == Some(&']') {
                    index += 1;
                }
                while chars.get(index).is_some_and(|ch| *ch != ']') {
                    index += 1;
                }
                if chars.get(index) != Some(&']') {
                    return Err(format!("invalid pattern {:?}", pattern));
                }
                let mut class: String = chars[start..=index].iter().collect();
                if class.contains('/') {
                    return Err(format!("invalid pattern {:?}", pattern));
                }
                if class.starts_with("[!") {
                    class.replace_range(1..2, "^");
                }
                expression.push_str(&class);
                index += 1;
            }
            '\\' if chars.get(index + 1).is_some() => {
                expression.push_str(&regex::escape(&chars[index + 1].to_string()));
                index += 2;
            }
            character => {
                expression.push_str(&regex::escape(&character.to_string()));
                index += 1;
            }
        }
    }
    expression.push('$');
    Regex::new(&expression).map_err(|error| format!("invalid pattern {:?}: {error}", pattern))
}

#[cfg(test)]
mod tests {
    use super::{match_component, PatternMatcher};

    fn matcher(patterns: &[&str]) -> PatternMatcher {
        PatternMatcher::new(
            &patterns
                .iter()
                .map(|value| (*value).to_owned())
                .collect::<Vec<_>>(),
        )
        .expect("patterns compile")
        .expect("matcher exists")
    }

    #[test]
    fn supports_ancestors_and_doublestar() {
        let matcher = matcher(&["**/bar/foo"]);
        assert!(matcher.matches_or_parent("a/bar/foo"));
        assert!(!matcher.matches_or_parent("a/bar"));
        assert!(!matcher.matches_or_parent("a/baz"));
    }

    #[test]
    fn preserves_ordered_exceptions() {
        let matcher = matcher(&["foo*", "!foo/bar"]);
        assert!(matcher.matches_or_parent("foo"));
        assert!(!matcher.matches_or_parent("foo/bar"));
        assert!(matcher.matches_or_parent("foo/other"));
    }

    #[test]
    fn rejects_bare_exclusion() {
        assert!(PatternMatcher::new(&["!".to_owned()]).is_err());
    }

    #[test]
    fn component_matching_uses_glob_pattern() {
        assert!(match_component("l*", "link"));
        assert!(match_component("l[ia]nk", "link"));
        assert!(!match_component("l[ia]nk", "look"));
        assert!(!match_component("[", "link"));

        #[cfg(not(windows))]
        assert!(match_component(r"\*", "*"));
    }
}
