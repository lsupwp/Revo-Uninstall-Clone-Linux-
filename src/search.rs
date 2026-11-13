//! Fuzzy application search.
//!
//! A single query is split on whitespace. Every token must match the display
//! name, the package ID, or (weakly) the summary. Scores combine prefix,
//! word-prefix, acronym and subsequence matching so launcher-style queries
//! like "vsc" or "vscode" find "Visual Studio Code".

fn normalize(s: &str) -> String {
    s.trim().to_lowercase()
}

fn words(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
}

/// Longest run of consecutive query characters found in order in `field`.
/// Returns None when the query is not a subsequence.
fn subsequence(field: &str, query: &str) -> Option<i32> {
    let field_chars: Vec<char> = field.chars().collect();
    let query_chars: Vec<char> = query.chars().collect();
    if query_chars.is_empty() {
        return None;
    }

    let mut index = 0usize;
    let mut run = 0i32;
    let mut best_run = 0i32;
    let mut gaps = 0i32;
    let mut last_match: Option<usize> = None;

    for &ch in &query_chars {
        let mut found = None;
        while index < field_chars.len() {
            let current = index;
            index += 1;
            if field_chars[current] == ch {
                found = Some(current);
                break;
            }
        }
        let pos = found?;
        match last_match {
            Some(prev) if pos == prev + 1 => run += 1,
            Some(_) => {
                gaps += 1;
                run = 1;
            }
            None => run = 1,
        }
        best_run = best_run.max(run);
        last_match = Some(pos);
    }

    Some((best_run * 15 - gaps * 3).clamp(0, 60))
}

/// Score one query token against one field.
fn score_field(query: &str, field: &str) -> Option<i32> {
    if query.is_empty() || field.is_empty() {
        return None;
    }
    if field == query {
        return Some(1000);
    }
    if field.starts_with(query) {
        let extra = field.chars().count().saturating_sub(query.chars().count());
        return Some(900 - extra.min(100) as i32);
    }
    if words(field).any(|w| w.starts_with(query)) {
        return Some(800);
    }

    let acronym: String = words(field).filter_map(|w| w.chars().next()).collect();
    if acronym == query {
        return Some(850);
    }
    if query.len() >= 2 && acronym.starts_with(query) {
        return Some(650);
    }

    if query.chars().count() >= 3 {
        if field.contains(query) {
            return Some(550);
        }
        if let Some(bonus) = subsequence(field, query) {
            return Some(500 + bonus);
        }
    }
    None
}

/// Score a search query against one application. Higher is better.
pub fn score(query: &str, name: &str, id: &str, summary: &str) -> Option<i32> {
    let query = normalize(query);
    if query.is_empty() {
        return Some(0);
    }
    let name = normalize(name);
    let id = normalize(id);
    let summary = normalize(summary);

    let mut total = 0;
    for token in query.split_whitespace() {
        let mut best = score_field(token, &name)
            .into_iter()
            .chain(score_field(token, &id))
            .max()?;
        // Summaries are only a weak fallback to avoid noisy matches.
        if token.chars().count() >= 3 && summary.contains(token) {
            best = best.max(150);
        }
        total += best;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acronym_finds_vsc() {
        let s = score("vsc", "Visual Studio Code", "code", "").unwrap();
        assert!(s >= 700, "score was {s}");
    }

    #[test]
    fn subsequence_finds_vscode() {
        assert!(score("vscode", "Visual Studio Code", "code", "").is_some());
    }

    #[test]
    fn word_prefix() {
        assert!(score("studio", "Visual Studio Code", "code", "").is_some());
    }

    #[test]
    fn subsequence_finds_firefox_as_ffx() {
        assert!(score("ffx", "Firefox", "firefox", "").is_some());
    }

    #[test]
    fn short_queries_do_not_subsequence() {
        assert!(score("ae", "Firefox", "firefox", "").is_none());
        assert!(score("vlc", "VLC media player", "vlc", "").is_some());
    }

    #[test]
    fn exact_and_prefix_rank_above_fuzzy() {
        let exact = score("code", "Code", "code", "").unwrap();
        let fuzzy = score("code", "Visual Studio Code", "code", "").unwrap();
        assert!(exact >= fuzzy);
    }

    #[test]
    fn unrelated_query_misses() {
        assert!(score("zzzz", "Visual Studio Code", "code", "").is_none());
    }
}
