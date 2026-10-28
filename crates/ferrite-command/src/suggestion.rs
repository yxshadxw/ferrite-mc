//! Tab-completion results, mirroring Brigadier's `Suggestion`, `Suggestions` and
//! `SuggestionsBuilder`. Ranges are byte offsets into the command text.

use std::collections::HashSet;

/// A half-open byte range `[start, end)` of the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct StringRange {
    pub start: usize,
    pub end: usize,
}

impl StringRange {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn at(pos: usize) -> Self {
        Self::new(pos, pos)
    }

    pub fn encompassing(a: Self, b: Self) -> Self {
        Self::new(a.start.min(b.start), a.end.max(b.end))
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn get<'a>(&self, s: &'a str) -> &'a str {
        &s[self.start..self.end]
    }
}

/// One completion: replace `range` with `text`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Suggestion {
    pub range: StringRange,
    pub text: String,
    /// Plain-text tooltip (vanilla sends a component; callers can wrap this).
    pub tooltip: Option<String>,
}

impl Suggestion {
    /// The input with this suggestion applied.
    pub fn apply(&self, input: &str) -> String {
        let mut out = String::with_capacity(input.len() + self.text.len());
        out.push_str(&input[..self.range.start]);
        out.push_str(&self.text);
        out.push_str(&input[self.range.end.min(input.len())..]);
        out
    }

    /// Widens to `range`, copying the surrounding command text into the suggestion.
    pub fn expand(&self, command: &str, range: StringRange) -> Self {
        if range == self.range {
            return self.clone();
        }
        let mut text = String::new();
        if range.start < self.range.start {
            text.push_str(&command[range.start..self.range.start]);
        }
        text.push_str(&self.text);
        if range.end > self.range.end {
            text.push_str(&command[self.range.end..range.end]);
        }
        Self {
            range,
            text,
            tooltip: self.tooltip.clone(),
        }
    }
}

/// A set of suggestions sharing one replacement range.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Suggestions {
    pub range: StringRange,
    pub list: Vec<Suggestion>,
}

impl Suggestions {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Texts only, for convenience.
    pub fn texts(&self) -> Vec<&str> {
        self.list.iter().map(|s| s.text.as_str()).collect()
    }

    /// Brigadier's `Suggestions.merge`.
    pub fn merge(command: &str, input: Vec<Suggestions>) -> Self {
        match input.len() {
            0 => Self::empty(),
            1 => input.into_iter().next().unwrap(),
            _ => {
                let mut seen = HashSet::new();
                let all = input
                    .into_iter()
                    .flat_map(|s| s.list)
                    .filter(|s| seen.insert(s.clone()))
                    .collect();
                Self::create(command, all)
            }
        }
    }

    /// Brigadier's `Suggestions.create`: widen every entry to a common range, dedupe and
    /// sort case-insensitively.
    pub fn create(command: &str, suggestions: Vec<Suggestion>) -> Self {
        let Some(first) = suggestions.first() else {
            return Self::empty();
        };
        let range = suggestions
            .iter()
            .fold(first.range, |r, s| StringRange::encompassing(r, s.range));
        let mut seen = HashSet::new();
        let mut list: Vec<Suggestion> = suggestions
            .iter()
            .map(|s| s.expand(command, range))
            .filter(|s| seen.insert(s.clone()))
            .collect();
        list.sort_by(|a, b| compare_ignore_case(&a.text, &b.text));
        Self { range, list }
    }
}

/// Java's `String.compareToIgnoreCase` (per-char lowercase comparison).
fn compare_ignore_case(a: &str, b: &str) -> std::cmp::Ordering {
    let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
    a.chars().map(fold).cmp(b.chars().map(fold))
}

/// Collects suggestions for the text starting at [`Self::start`].
#[derive(Debug, Clone)]
pub struct SuggestionsBuilder {
    input: String,
    input_lower: String,
    start: usize,
    result: Vec<Suggestion>,
}

impl SuggestionsBuilder {
    /// `start` must be a char boundary of `input`.
    pub fn new(input: impl Into<String>, start: usize) -> Self {
        let input = input.into();
        let input_lower = input.to_lowercase();
        Self {
            input,
            input_lower,
            start,
            result: Vec::new(),
        }
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn start(&self) -> usize {
        self.start
    }

    /// The (partial) text being completed.
    pub fn remaining(&self) -> &str {
        &self.input[self.start..]
    }

    pub fn remaining_lowercase(&self) -> &str {
        // Lowercasing can change byte lengths; fall back to lowercasing the slice.
        if self.input_lower.len() == self.input.len() {
            &self.input_lower[self.start..]
        } else {
            &self.input_lower[self.input_lower.len() - self.remaining().to_lowercase().len()..]
        }
    }

    /// Adds `text` (skipped if it equals what is already typed, like Brigadier).
    pub fn suggest(&mut self, text: impl Into<String>) -> &mut Self {
        self.push(text.into(), None)
    }

    pub fn suggest_with_tooltip(
        &mut self,
        text: impl Into<String>,
        tooltip: impl Into<String>,
    ) -> &mut Self {
        self.push(text.into(), Some(tooltip.into()))
    }

    /// Suggests `text` only if it starts with the (case-insensitive) remaining input.
    pub fn suggest_matching(&mut self, text: impl Into<String>) -> &mut Self {
        let text = text.into();
        if text.to_lowercase().starts_with(self.remaining_lowercase()) {
            self.suggest(text);
        }
        self
    }

    fn push(&mut self, text: String, tooltip: Option<String>) -> &mut Self {
        if text == self.remaining() {
            return self;
        }
        self.result.push(Suggestion {
            range: StringRange::new(self.start, self.input.len()),
            text,
            tooltip,
        });
        self
    }

    /// A fresh builder over the same input starting at `start`.
    pub fn create_offset(&self, start: usize) -> Self {
        Self {
            input: self.input.clone(),
            input_lower: self.input_lower.clone(),
            start,
            result: Vec::new(),
        }
    }

    /// Appends another builder's results.
    pub fn add(&mut self, other: SuggestionsBuilder) -> &mut Self {
        self.result.extend(other.result);
        self
    }

    pub fn build(self) -> Suggestions {
        Suggestions::create(&self.input, self.result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_expands_to_common_range() {
        let mut a = SuggestionsBuilder::new("tp @", 3);
        a.suggest("@a");
        let mut b = SuggestionsBuilder::new("tp @", 4);
        b.suggest("p");
        let merged = Suggestions::merge("tp @", vec![a.build(), b.build()]);
        assert_eq!(merged.range, StringRange::new(3, 4));
        assert_eq!(merged.texts(), vec!["@a", "@p"]);
    }

    #[test]
    fn skips_exact_match_and_sorts_ignoring_case() {
        let mut b = SuggestionsBuilder::new("x ab", 2);
        b.suggest("ab").suggest("Abc").suggest("abd").suggest("ABa");
        assert_eq!(b.build().texts(), vec!["ABa", "Abc", "abd"]);
    }

    #[test]
    fn apply_replaces_range() {
        let s = Suggestion {
            range: StringRange::new(3, 5),
            text: "creative".into(),
            tooltip: None,
        };
        assert_eq!(s.apply("gm cr x"), "gm creative x");
    }
}
