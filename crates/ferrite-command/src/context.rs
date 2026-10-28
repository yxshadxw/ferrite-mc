//! Parse state handed to commands, redirect modifiers and suggestion providers.

use std::collections::HashMap;
use std::sync::Arc;

use crate::arguments::ArgumentValue;
use crate::suggestion::StringRange;
use crate::tree::{Command, NodeId, RedirectModifier};

/// An argument value with the input range it was parsed from.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedArgument {
    pub range: StringRange,
    pub value: ArgumentValue,
}

/// A node matched while parsing, with its input range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedNode {
    pub node: NodeId,
    pub range: StringRange,
}

/// One level of a parsed command (Brigadier's `CommandContext`); a redirect continues in
/// [`CommandContext::child`].
pub struct CommandContext<S> {
    pub(crate) source: S,
    pub(crate) input: Arc<str>,
    pub(crate) arguments: HashMap<String, ParsedArgument>,
    pub(crate) command: Option<Command<S>>,
    pub(crate) root: NodeId,
    pub(crate) nodes: Vec<ParsedNode>,
    pub(crate) range: StringRange,
    pub(crate) child: Option<Box<CommandContext<S>>>,
    pub(crate) modifier: Option<RedirectModifier<S>>,
    pub(crate) forks: bool,
}

impl<S: Clone> Clone for CommandContext<S> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            input: self.input.clone(),
            arguments: self.arguments.clone(),
            command: self.command.clone(),
            root: self.root,
            nodes: self.nodes.clone(),
            range: self.range,
            child: self.child.clone(),
            modifier: self.modifier.clone(),
            forks: self.forks,
        }
    }
}

impl<S: std::fmt::Debug> std::fmt::Debug for CommandContext<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandContext")
            .field("source", &self.source)
            .field("input", &self.input)
            .field("arguments", &self.arguments)
            .field("nodes", &self.nodes)
            .field("range", &self.range)
            .field("child", &self.child)
            .field("forks", &self.forks)
            .finish_non_exhaustive()
    }
}

impl<S> CommandContext<S> {
    pub(crate) fn new(source: S, input: Arc<str>, root: NodeId, start: usize) -> Self {
        Self {
            source,
            input,
            arguments: HashMap::new(),
            command: None,
            root,
            nodes: Vec::new(),
            range: StringRange::at(start),
            child: None,
            modifier: None,
            forks: false,
        }
    }

    pub fn source(&self) -> &S {
        &self.source
    }

    /// The full command text (truncated to the cursor during suggestion).
    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn argument(&self, name: &str) -> Option<&ArgumentValue> {
        self.arguments.get(name).map(|a| &a.value)
    }

    pub fn parsed_argument(&self, name: &str) -> Option<&ParsedArgument> {
        self.arguments.get(name)
    }

    pub fn arguments(&self) -> impl Iterator<Item = (&str, &ParsedArgument)> {
        self.arguments.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn integer(&self, name: &str) -> Option<i32> {
        self.argument(name)?.as_i32()
    }

    pub fn string(&self, name: &str) -> Option<&str> {
        self.argument(name)?.as_str()
    }

    /// The node this level started at (the root, or a redirect target).
    pub fn root_node(&self) -> NodeId {
        self.root
    }

    pub fn nodes(&self) -> &[ParsedNode] {
        &self.nodes
    }

    pub fn range(&self) -> StringRange {
        self.range
    }

    pub fn child(&self) -> Option<&CommandContext<S>> {
        self.child.as_deref()
    }

    /// The innermost context of the redirect chain.
    pub fn last_child(&self) -> &CommandContext<S> {
        let mut ctx = self;
        while let Some(child) = ctx.child.as_deref() {
            ctx = child;
        }
        ctx
    }

    pub fn has_nodes(&self) -> bool {
        !self.nodes.is_empty()
    }

    pub fn is_forked(&self) -> bool {
        self.forks
    }

    pub fn command(&self) -> Option<&Command<S>> {
        self.command.as_ref()
    }

    pub(crate) fn with_node(
        &mut self,
        node: NodeId,
        range: StringRange,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
    ) {
        self.nodes.push(ParsedNode { node, range });
        self.range = StringRange::encompassing(self.range, range);
        self.modifier = modifier;
        self.forks = forks;
    }
}

impl<S: Clone> CommandContext<S> {
    /// This context with a different source (Brigadier's `copyFor`).
    pub fn copy_for(&self, source: S) -> Self {
        let mut copy = self.clone();
        copy.source = source;
        copy
    }
}
