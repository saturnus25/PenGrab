use crate::catalog::{Catalog, Language, search_text};
use std::{cmp::Reverse, ops::Range};
use unicode_normalization::char::is_combining_mark;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Category(usize),
    Tool(usize),
}

pub struct Match {
    pub target: Target,
    pub title: String,
    pub highlights: Vec<Range<usize>>,
    pub evidence: Vec<String>,
}

pub fn find(catalog: &Catalog, language: Language, input: &str) -> Vec<Match> {
    let normalized = search_text(input);
    let terms = words(&normalized);
    if terms.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();
    for (index, category) in catalog.categories.iter().enumerate() {
        let title = category.name.get(language);
        let mut fields = vec![
            (category.name.en.as_str(), 4, true),
            (category.name.es.as_str(), 4, true),
            (category.id.as_str(), 8, false),
        ];
        fields.extend(
            category
                .aliases
                .iter()
                .map(|alias| (alias.as_str(), 2, true)),
        );
        if let Some(hit) = rank(Target::Category(index), title, &fields, &terms) {
            results.push(hit);
        }
    }
    for (index, tool) in catalog.tools.iter().enumerate() {
        let path = catalog.category_line(index, language);
        let mut fields = vec![
            (tool.id.as_str(), 5, true),
            (tool.winget_id.as_deref().unwrap_or_default(), 7, false),
            (tool.description.get(language), 38, false),
            (path.as_str(), 44, false),
        ];
        fields.extend(tool.aliases.iter().map(|alias| (alias.as_str(), 2, true)));
        if let Some(hit) = rank(Target::Tool(index), &tool.name, &fields, &terms) {
            results.push(hit);
        }
    }
    results.sort_by(|(a_score, a), (b_score, b)| {
        a_score.cmp(b_score).then_with(|| a.title.cmp(&b.title))
    });
    results.into_iter().map(|(_, hit)| hit).collect()
}

fn words(text: &str) -> Vec<(usize, &str)> {
    let mut result = Vec::new();
    let mut start = None;
    for (index, ch) in text.char_indices().chain([(text.len(), '\0')]) {
        if ch.is_alphanumeric()
            || (start.is_some() && (is_combining_mark(ch) || matches!(ch, '#' | '+')))
        {
            start.get_or_insert(index);
        } else if let Some(offset) = start.take() {
            result.push((offset, &text[offset..index]));
        }
    }
    result
}

fn rank(
    target: Target,
    title: &str,
    fields: &[(&str, u16, bool)],
    terms: &[(usize, &str)],
) -> Option<((u8, u16), Match)> {
    let mut result = Match {
        target,
        title: title.to_owned(),
        highlights: Vec::new(),
        evidence: Vec::new(),
    };
    let mut score = 0u16;
    for &(_, term) in terms {
        let (penalty, location, evidence) = [(title, 0, true)]
            .into_iter()
            .chain(fields.iter().copied())
            .enumerate()
            .filter(|(_, (_, weight, _))| *weight < 38 || term.chars().count() >= 3)
            .filter_map(|(field, (text, weight, fuzzy))| {
                let (cost, range, word) = best_word(text, term, fuzzy)?;
                Some((cost + weight, (field == 0).then_some(range), word))
            })
            .min_by_key(|(cost, title_match, _)| (*cost, Reverse(title_match.is_some())))?;
        score = score.saturating_add(penalty);
        if let Some(location) = location {
            result.highlights.push(location);
        } else if !result.evidence.iter().any(|seen| seen == evidence) {
            result.evidence.push(evidence.to_owned());
        }
    }
    let exact_phrase = |text: &str| {
        let normalized = search_text(text);
        words(&normalized)
            .iter()
            .map(|(_, word)| word)
            .eq(terms.iter().map(|(_, word)| word))
    };
    let priority = if exact_phrase(title) {
        0
    } else if fields
        .iter()
        .any(|(text, weight, _)| *weight <= 8 && exact_phrase(text))
    {
        1
    } else {
        2
    };
    Some(((priority, score), result))
}

fn best_word<'a>(text: &'a str, term: &str, fuzzy: bool) -> Option<(u16, Range<usize>, &'a str)> {
    // Search only word interiors, so short aliases do not match across separators.
    words(text)
        .into_iter()
        .filter_map(|(offset, word)| {
            let normalized = search_text(word);
            let (cost, range) = if normalized == term {
                (0, 0..normalized.len())
            } else if normalized.starts_with(term) {
                (8, 0..term.len())
            } else if term.chars().count() >= 3
                && let Some(start) = normalized.find(term)
            {
                (16, start..start + term.len())
            } else if fuzzy && let Some(changes) = typo_distance(term, &normalized) {
                (24 + changes as u16 * 4, 0..normalized.len())
            } else {
                return None;
            };
            let mut matched = original_range(word, range);
            matched.start += offset;
            matched.end += offset;
            Some((cost, matched, word))
        })
        .min_by_key(|(cost, range, _)| (*cost, range.start))
}

fn original_range(word: &str, normalized: Range<usize>) -> Range<usize> {
    let mut position = 0;
    let mut start = None;
    let mut end = 0;
    for (offset, ch) in word.char_indices() {
        let next = position + search_text(ch.encode_utf8(&mut [0; 4])).len();
        if position < normalized.end && next > normalized.start {
            start.get_or_insert(offset);
            end = offset + ch.len_utf8();
        }
        position = next;
    }
    start.unwrap_or(0)..end
}

fn typo_distance(term: &str, word: &str) -> Option<usize> {
    const MAX_WORD: usize = 32;
    let term: Vec<_> = term.chars().take(MAX_WORD + 1).collect();
    let word: Vec<_> = word.chars().take(MAX_WORD + 1).collect();
    if !(4..=MAX_WORD).contains(&term.len()) || word.len() > MAX_WORD {
        return None;
    }
    let limit = if term.len() >= 8 { 2 } else { 1 };
    if term.len().abs_diff(word.len()) > limit {
        return None;
    }

    let mut before_previous = [0; MAX_WORD + 1];
    let mut previous = std::array::from_fn::<_, { MAX_WORD + 1 }, _>(|column| column);
    let mut row = [0; MAX_WORD + 1];
    for (i, left) in term.iter().enumerate() {
        row[0] = i + 1;
        for (j, right) in word.iter().enumerate() {
            row[j + 1] = (row[j] + 1)
                .min(previous[j + 1] + 1)
                .min(previous[j] + usize::from(left != right));
            if i > 0 && j > 0 && *left == word[j - 1] && term[i - 1] == *right {
                row[j + 1] = row[j + 1].min(before_previous[j - 1] + 1);
            }
        }
        std::mem::swap(&mut before_previous, &mut previous);
        std::mem::swap(&mut previous, &mut row);
    }
    let distance = previous[word.len()];
    (distance <= limit).then_some(distance)
}
