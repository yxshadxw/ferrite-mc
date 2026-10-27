//! Tab-completion results: candidate strings plus the input range they replace.
//!
//! Modelled on Brigadier's `Suggestions` / `SuggestionsBuilder` (MIT). The range is what goes
//! into the Command Suggestions Response packet: each candidate replaces `input[start..end]`.

/// Collects completion candidates for one input range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionsBuilder {
    /// Whole input the suggestions are computed for.
    input: String,
    /// Character offset where the completed word starts.
    start: usize,
    candidates: Vec<String>,
}

impl SuggestionsBuilder {
    /// Starts a builder for the word that begins at character offset `start`.
    pub fn new(input: impl Into<String>, start: usize) -> Self {
        Self {
            input: input.into(),
            start,
            candidates: Vec::new(),
        }
    }

    /// The whole input.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Character offset where the completed word starts.
    pub fn start(&self) -> usize {
        self.start
    }

    /// The part of the input that is still being typed.
    pub fn remaining(&self) -> String {
        self.input.chars().skip(self.start).collect()
    }

    /// Lower-cased form of [`Self::remaining`], for case-insensitive matching.
    pub fn remaining_lowercase(&self) -> String {
        self.remaining().to_lowercase()
    }

    /// Suggests `text` if it starts with what has been typed and is not already complete.
    pub fn suggest(&mut self, text: impl Into<String>) -> &mut Self {
        let text = text.into();
        if text != self.remaining() && text.to_lowercase().starts_with(&self.remaining_lowercase())
        {
            self.candidates.push(text);
        }
        self
    }

    /// Suggests the decimal form of `value`.
    pub fn suggest_int(&mut self, value: i64) -> &mut Self {
        self.suggest(value.to_string())
    }

    /// Finishes the builder for a cursor at character offset `cursor`.
    pub fn build(&self, cursor: usize) -> Suggestions {
        Suggestions {
            range: (self.start, cursor.max(self.start)),
            list: self.candidates.clone(),
        }
    }
}

/// Completion candidates for one input range.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Suggestions {
    /// `(start, end)` in characters of the text the candidates replace.
    pub range: (usize, usize),
    /// Candidate strings, in insertion order and without duplicates.
    pub list: Vec<String>,
}

impl Suggestions {
    /// No suggestions at all.
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Merges candidates from sibling nodes: the range widens to cover every input range.
    pub fn merge(others: impl IntoIterator<Item = Suggestions>) -> Self {
        let others: Vec<Suggestions> = others.into_iter().filter(|s| !s.is_empty()).collect();
        if others.is_empty() {
            return Self::empty();
        }
        if others.len() == 1 {
            return others.into_iter().next().unwrap();
        }
        let mut start = usize::MAX;
        let mut end = 0;
        let mut list = Vec::new();
        for suggestions in others {
            start = start.min(suggestions.range.0);
            end = end.max(suggestions.range.1);
            for text in suggestions.list {
                if !list.contains(&text) {
                    list.push(text);
                }
            }
        }
        Self {
            range: (start.min(end), end),
            list,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builders_filter_by_prefix() {
        let mut builder = SuggestionsBuilder::new("/gamem", 1);
        builder.suggest("gamemode").suggest("gamemode").suggest("give").suggest_int(42);
        let built = builder.build(6);
        assert_eq!(built.list, ["gamemode"]);
        assert_eq!(built.range, (1, 6));
    }

    #[test]
    fn exact_matches_are_not_suggested() {
        let mut builder = SuggestionsBuilder::new("/time", 1);
        builder.suggest("time");
        assert!(builder.build(5).is_empty());
    }

    #[test]
    fn merge_unions_ranges_and_candidates() {
        let mut a = SuggestionsBuilder::new("/ti", 1);
        a.suggest("time");
        let mut b = SuggestionsBuilder::new("/ti", 1);
        b.suggest("tick");
        let merged = Suggestions::merge([a.build(3), b.build(3)]);
        assert_eq!(merged.range, (1, 3));
        assert_eq!(merged.list, ["time", "tick"]);
    }
}
