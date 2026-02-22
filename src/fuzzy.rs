pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0; b.len() + 1];

    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

pub fn close_match<'a>(name: &str, candidates: &'a [String]) -> Option<&'a str> {
    let threshold = (name.len() as f64 * 0.3).ceil().max(2.0) as usize;
    candidates
        .iter()
        .filter(|c| c.as_str() != name)
        .filter_map(|c| {
            let dist = levenshtein(name, c);
            (dist <= threshold).then_some((dist, c.as_str()))
        })
        .min_by_key(|(dist, _)| *dist)
        .map(|(_, name)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_strings() {
        assert_eq!(levenshtein("abc", "abc"), 0);
    }

    #[test]
    fn single_edit() {
        assert_eq!(levenshtein("feat/login", "feat/logni"), 2);
    }

    #[test]
    fn empty_strings() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", "abc"), 3);
    }

    #[test]
    fn close_match_finds_typo() {
        let branches = vec!["feat/login".into(), "fix/bug".into()];
        assert_eq!(close_match("feat/logni", &branches), Some("feat/login"));
    }

    #[test]
    fn close_match_none_when_distant() {
        let branches = vec!["feat/login".into()];
        assert_eq!(close_match("fix/something-else", &branches), None);
    }

    #[test]
    fn close_match_skips_exact() {
        let branches = vec!["feat/login".into()];
        assert_eq!(close_match("feat/login", &branches), None);
    }
}
