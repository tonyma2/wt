pub fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0; b.len() + 1];

    for (i, a_char) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, &b_char) in b.iter().enumerate() {
            let cost = usize::from(a_char != b_char);
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

pub fn filter_score(query: &str, candidate: &str) -> Option<usize> {
    if query.is_empty() {
        return Some(0);
    }
    let query_lc: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    let cand_lc: Vec<char> = candidate.chars().flat_map(char::to_lowercase).collect();

    let mut best = score_from(&query_lc, &cand_lc, 0);

    for i in 1..cand_lc.len() {
        if cand_lc[i] == query_lc[0]
            && is_boundary(cand_lc[i - 1])
            && let Some(s) = score_from(&query_lc, &cand_lc, i)
        {
            best = Some(best.map_or(s, |b| b.min(s)));
        }
    }

    best
}

fn is_boundary(c: char) -> bool {
    matches!(c, '/' | '-' | '_' | ' ' | '.')
}

fn score_from(query: &[char], candidate: &[char], start: usize) -> Option<usize> {
    let mut qi = 0;
    let mut gap_score = 0usize;
    let mut first_pos = 0usize;
    let mut last_match: Option<usize> = None;

    for ci in start..candidate.len() {
        if candidate[ci] == query[qi] {
            match last_match {
                None => first_pos = ci,
                Some(prev) => {
                    let gap = ci - prev - 1;
                    if gap > 0 {
                        let boundary = is_boundary(candidate[ci - 1]);
                        gap_score += if boundary { 1 } else { gap + 1 };
                    }
                }
            }
            last_match = Some(ci);
            qi += 1;
            if qi == query.len() {
                break;
            }
        }
    }

    (qi == query.len()).then_some(gap_score * 1000 + first_pos)
}

pub fn close_match<'a>(name: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let len = name.chars().count();
    if len == 0 {
        return None;
    }
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

    #[test]
    fn filter_score_query_longer_than_candidate() {
        assert_eq!(filter_score("abcdef", "ab"), None);
    }

    #[test]
    fn filter_score_empty_candidate() {
        assert_eq!(filter_score("a", ""), None);
    }

    #[test]
    fn filter_score_boundary_start_beats_greedy() {
        let score = filter_score("main", "my-app main").unwrap();
        assert_eq!(score, 7, "should find 'main' at word boundary position 7");
    }

    #[test]
    fn filter_score_greedy_fails_but_boundary_succeeds() {
        let score = filter_score("log", "long-log").unwrap();
        assert_eq!(score, 5);
    }

    #[test]
    fn filter_score_best_boundary_wins_among_multiple() {
        let earlier = filter_score("ab", "xx-ab.ab").unwrap();
        let later_only = filter_score("ab", "xx-xx.ab").unwrap();
        assert!(earlier < later_only);
    }

    #[test]
    fn filter_score_single_char_query() {
        assert_eq!(filter_score("f", "feat/login"), Some(0));
        assert_eq!(filter_score("l", "feat/login"), Some(5));
    }

    #[test]
    fn filter_score_full_candidate() {
        assert_eq!(filter_score("feat/login", "feat/login"), Some(0));
    }

    #[test]
    fn filter_score_gap_penalty_scales_with_distance() {
        let small_gap = filter_score("fn", "fxn").unwrap();
        let large_gap = filter_score("fn", "fxxxxn").unwrap();
        assert!(small_gap < large_gap);
    }

    #[test]
    fn filter_score_multiple_boundary_gaps() {
        let one_boundary = filter_score("fl", "feat/login").unwrap();
        let two_boundaries = filter_score("flb", "feat/login-bar").unwrap();
        assert!(two_boundaries > one_boundary);
    }
}
