//! Fuzzy-sök, samma regler som `fuzzy_score` i Puppa och Muppa, så att sökningar ger samma träffar överallt:
//! alla ord i frågan måste finnas i texten, som delsträng (bäst, extra poäng i början av ett ord) eller som
//! bokstäver i ordning (minst tre bokstäver, inte för utspritt).

/// Högre = bättre. None = ingen träff.
pub fn score(query: &str, text: &str) -> Option<i64> {
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let mut total = 0i64;
    for word in query.to_lowercase().split_whitespace() {
        let w: Vec<char> = word.chars().collect();
        if let Some(i) = find(&text, &w, 0) {
            let start = i == 0 || !text[i - 1].is_alphanumeric();
            total += 100 + if start { 40 } else { 0 } - (i.min(30) as i64);
            continue;
        }
        if w.len() < 3 {
            return None;
        }
        let (mut pos, mut gaps, mut last): (usize, usize, Option<usize>) = (0, 0, None);
        for ch in &w {
            let j = text[pos..].iter().position(|c| c == ch)? + pos;
            if let Some(l) = last {
                gaps += j - l - 1;
            }
            last = Some(j);
            pos = j + 1;
        }
        if gaps > 2 * w.len() {
            return None;
        }
        total += (60 - gaps as i64 * 3).max(1);
    }
    Some(total)
}

fn find(hay: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    (from..=hay.len() - needle.len()).find(|&i| hay[i..i + needle.len()] == *needle)
}

/// Rangordna `items` efter fuzzy-poäng (stabil ordning vid lika poäng). Tom fråga = allt i ursprunglig ordning.
pub fn rank<'a, T>(query: &str, items: &'a [T], key: impl Fn(&T) -> String) -> Vec<&'a T> {
    if query.trim().is_empty() {
        return items.iter().collect();
    }
    let mut scored: Vec<(i64, usize, &T)> =
        items.iter().enumerate().filter_map(|(n, x)| score(query, &key(x)).map(|s| (s, n, x))).collect();
    scored.sort_by_key(|(s, n, _)| (-s, *n));
    scored.into_iter().map(|(_, _, x)| x).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_like_puppa() {
        assert!(score("ann", "Anna Svensson").is_some());
        assert!(score("asv", "Anna Svensson").is_some(), "bokstäver i ordning");
        assert!(score("xyz", "Anna Svensson").is_none());
        assert!(score("an sv", "Anna Svensson").is_some(), "flera ord");
        assert!(score("an zz", "Anna Svensson").is_none());
        assert!(score("svensson", "anna svensson").unwrap() > score("svs", "anna svensson").unwrap());
        assert!(score("lic", "Licensöversikt").unwrap() > score("lic", "Vilka har en viss licens?").unwrap());
        let items = ["Bo Berg", "Anna Svensson", "Annika Ek"];
        let r: Vec<&&str> = rank("ann", &items, |s| s.to_string());
        assert_eq!(r.len(), 2);
        assert_eq!(rank("", &items, |s| s.to_string()).len(), 3);
    }
}
