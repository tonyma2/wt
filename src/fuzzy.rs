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

pub fn filter_score(query: &str, candidate: &str) -> Option<usize> {
    if query.is_empty() {
        return Some(0);
    }
    let mut query_chars = query.chars().flat_map(char::to_lowercase).peekable();
    let mut gap_score = 0usize;
    let mut first_pos = 0usize;
    let mut last_match: Option<usize> = None;
    let mut prev_char: Option<char> = None;

    for (ci, cc) in candidate.chars().flat_map(char::to_lowercase).enumerate() {
        if query_chars.peek() == Some(&cc) {
            match last_match {
                None => first_pos = ci,
                Some(prev) => {
                    let gap = ci - prev - 1;
                    if gap > 0 {
                        let boundary =
                            prev_char.is_some_and(|c| matches!(c, '/' | '-' | '_' | ' ' | '.'));
                        gap_score += if boundary { 1 } else { gap + 1 };
                    }
                }
            }
            last_match = Some(ci);
            query_chars.next();
        }
        prev_char = Some(cc);
    }

    query_chars.peek().is_none().then_some(gap_score * 1000 + first_pos)
}

pub fn close_match<'a>(name: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let len = name.chars().count();
    if len == 0 {
        return None;
    }
    // ~1 edit per 3 chars, min 2 to avoid false positives on short names, capped at half the input length
    let threshold = (len as f64 * 0.3).ceil().max(2.0).min(len as f64 / 2.0) as usize;
    candidates
        .iter()
        .filter(|c| **c != name)
        .filter_map(|c| {
            let dist = levenshtein(name, c);
            (dist <= threshold).then_some((dist, *c))
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
    fn transposition() {
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
        assert_eq!(
            close_match("feat/logni", &["feat/login", "fix/bug"]),
            Some("feat/login")
        );
    }

    #[test]
    fn close_match_none_when_distant() {
        assert_eq!(close_match("fix/something-else", &["feat/login"]), None);
    }

    #[test]
    fn close_match_skips_exact() {
        assert_eq!(close_match("feat/login", &["feat/login"]), None);
    }

    #[test]
    fn close_match_no_false_positive_short_names() {
        assert_eq!(close_match("foo", &["bar"]), None);
    }

    #[test]
    fn close_match_empty_candidates() {
        assert_eq!(close_match("feat/login", &[]), None);
    }

    #[test]
    fn close_match_empty_name() {
        assert_eq!(close_match("", &["a"]), None);
    }

    #[test]
    fn close_match_picks_closest() {
        assert_eq!(
            close_match("feat/login", &["feat/logxxx", "feat/logim"]),
            Some("feat/logim"),
        );
    }

    #[test]
    fn filter_score_empty_query() {
        assert_eq!(filter_score("", "anything"), Some(0));
    }

    #[test]
    fn filter_score_subsequence_match() {
        assert!(filter_score("fl", "feat/login").is_some());
        assert!(filter_score("flog", "feat/login").is_some());
    }

    #[test]
    fn filter_score_no_match() {
        assert_eq!(filter_score("xyz", "feat/login"), None);
        assert_eq!(filter_score("lf", "feat/login"), None);
    }

    #[test]
    fn filter_score_case_insensitive() {
        assert!(filter_score("FL", "feat/login").is_some());
        assert!(filter_score("feat", "FEAT/LOGIN").is_some());
    }

    #[test]
    fn filter_score_consecutive_beats_gapped() {
        let consecutive = filter_score("feat", "feat/login").unwrap();
        let gapped = filter_score("flog", "feat/login").unwrap();
        assert!(consecutive < gapped);
    }

    #[test]
    fn filter_score_exact_match() {
        assert_eq!(filter_score("main", "main"), Some(0));
    }

    #[test]
    fn filter_score_earlier_first_match_wins_tiebreak() {
        let early = filter_score("m", "my-app main").unwrap();
        let late = filter_score("m", "other-repo main").unwrap();
        assert!(early < late);
    }

    #[test]
    fn filter_score_boundary_match_cheaper_than_mid_word() {
        let boundary = filter_score("fl", "feat/login").unwrap();
        let mid_word = filter_score("fi", "flair").unwrap();
        assert!(boundary < mid_word);
    }
}
