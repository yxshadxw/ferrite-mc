//! The command graph: nodes stored in an arena owned by the dispatcher, and tree-shaped
//! builders (Brigadier's `LiteralArgumentBuilder` / `RequiredArgumentBuilder`).

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::arguments::ArgumentType;
use crate::context::CommandContext;
use crate::error::CommandError;
use crate::suggestion::SuggestionsBuilder;

/// Index of a node in its dispatcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub(crate) u32);

impl NodeId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A command body. Returns Brigadier's integer result.
pub type Command<S> = Arc<dyn Fn(&CommandContext<S>) -> Result<i32, CommandError> + Send + Sync>;
/// Whether a source may use a node.
pub type Requirement<S> = Arc<dyn Fn(&S) -> bool + Send + Sync>;
/// Maps the source when following a redirect; a fork may yield many sources.
pub type RedirectModifier<S> =
    Arc<dyn Fn(&CommandContext<S>) -> Result<Vec<S>, CommandError> + Send + Sync>;
/// Fills suggestions for an argument.
pub type SuggestionProvider<S> =
    Arc<dyn Fn(&CommandContext<S>, &mut SuggestionsBuilder) + Send + Sync>;

/// Wire identifiers for the `Commands` packet suggestion type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SuggestionKind {
    /// `minecraft:ask_server`: the client asks the server on every keystroke.
    AskServer,
    AllRecipes,
    AvailableSounds,
    SummonableEntities,
    Other(String),
}

impl SuggestionKind {
    pub fn id(&self) -> &str {
        match self {
            Self::AskServer => "minecraft:ask_server",
            Self::AllRecipes => "minecraft:all_recipes",
            Self::AvailableSounds => "minecraft:available_sounds",
            Self::SummonableEntities => "minecraft:summonable_entities",
            Self::Other(id) => id,
        }
    }
}

/// Custom suggestions for an argument: what the client is told (`kind`) and, optionally,
/// a server-side provider answering `ask_server` requests.
pub struct Suggester<S> {
    pub kind: SuggestionKind,
    pub provider: Option<SuggestionProvider<S>>,
}

impl<S> Clone for Suggester<S> {
    fn clone(&self) -> Self {
        Self {
            kind: self.kind.clone(),
            provider: self.provider.clone(),
        }
    }
}

impl<S> fmt::Debug for Suggester<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Suggester")
            .field("kind", &self.kind)
            .field("provider", &self.provider.is_some())
            .finish()
    }
}

/// What a node matches.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    Root,
    Literal(String),
    Argument { name: String, ty: ArgumentType },
}

/// One node of the graph.
pub struct Node<S> {
    pub(crate) kind: NodeKind,
    pub(crate) children: Vec<NodeId>,
    pub(crate) by_name: HashMap<String, NodeId>,
    pub(crate) literals: HashMap<String, NodeId>,
    pub(crate) arguments: Vec<NodeId>,
    pub(crate) command: Option<Command<S>>,
    pub(crate) requirement: Option<Requirement<S>>,
    pub(crate) restricted: bool,
    pub(crate) redirect: Option<NodeId>,
    pub(crate) modifier: Option<RedirectModifier<S>>,
    pub(crate) forks: bool,
    pub(crate) suggester: Option<Suggester<S>>,
}

impl<S> Node<S> {
    pub(crate) fn new(kind: NodeKind) -> Self {
        Self {
            kind,
            children: Vec::new(),
            by_name: HashMap::new(),
            literals: HashMap::new(),
            arguments: Vec::new(),
            command: None,
            requirement: None,
            restricted: false,
            redirect: None,
            modifier: None,
            forks: false,
            suggester: None,
        }
    }

    pub fn kind(&self) -> &NodeKind {
        &self.kind
    }

    /// Literal text or argument name (empty for the root).
    pub fn name(&self) -> &str {
        match &self.kind {
            NodeKind::Root => "",
            NodeKind::Literal(l) => l,
            NodeKind::Argument { name, .. } => name,
        }
    }

    /// `literal` or `<argument>` as shown in usage strings.
    pub fn usage_text(&self) -> String {
        match &self.kind {
            NodeKind::Root => String::new(),
            NodeKind::Literal(l) => l.clone(),
            NodeKind::Argument { name, .. } => format!("<{name}>"),
        }
    }

    pub fn argument_type(&self) -> Option<&ArgumentType> {
        match &self.kind {
            NodeKind::Argument { ty, .. } => Some(ty),
            _ => None,
        }
    }

    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    pub fn child(&self, name: &str) -> Option<NodeId> {
        self.by_name.get(name).copied()
    }

    pub fn is_executable(&self) -> bool {
        self.command.is_some()
    }

    pub fn command(&self) -> Option<&Command<S>> {
        self.command.as_ref()
    }

    pub fn redirect(&self) -> Option<NodeId> {
        self.redirect
    }

    pub fn is_fork(&self) -> bool {
        self.forks
    }

    /// Marks the node as needing elevated permissions (packet flag `0x20`).
    pub fn is_restricted(&self) -> bool {
        self.restricted
    }

    pub fn suggester(&self) -> Option<&Suggester<S>> {
        self.suggester.as_ref()
    }

    /// Whether `source` passes this node's requirement.
    pub fn can_use(&self, source: &S) -> bool {
        self.requirement.as_ref().is_none_or(|r| r(source))
    }
}

impl<S> fmt::Debug for Node<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Node")
            .field("kind", &self.kind)
            .field("children", &self.children)
            .field("executable", &self.command.is_some())
            .field("redirect", &self.redirect)
            .field("forks", &self.forks)
            .field("restricted", &self.restricted)
            .finish()
    }
}

/// A subtree under construction; [`crate::CommandDispatcher::register`] inserts it.
pub struct NodeBuilder<S> {
    pub(crate) kind: NodeKind,
    pub(crate) children: Vec<NodeBuilder<S>>,
    pub(crate) command: Option<Command<S>>,
    pub(crate) requirement: Option<Requirement<S>>,
    pub(crate) restricted: bool,
    pub(crate) redirect: Option<NodeId>,
    pub(crate) modifier: Option<RedirectModifier<S>>,
    pub(crate) forks: bool,
    pub(crate) suggester: Option<Suggester<S>>,
}

/// Starts a literal node.
pub fn literal<S>(name: impl Into<String>) -> NodeBuilder<S> {
    NodeBuilder::new(NodeKind::Literal(name.into()))
}

/// Starts an argument node.
pub fn argument<S>(name: impl Into<String>, ty: ArgumentType) -> NodeBuilder<S> {
    NodeBuilder::new(NodeKind::Argument {
        name: name.into(),
        ty,
    })
}

impl<S> NodeBuilder<S> {
    fn new(kind: NodeKind) -> Self {
        Self {
            kind,
            children: Vec::new(),
            command: None,
            requirement: None,
            restricted: false,
            redirect: None,
            modifier: None,
            forks: false,
            suggester: None,
        }
    }

    /// Adds a child. Panics if the node already redirects (like Brigadier).
    pub fn then(mut self, child: NodeBuilder<S>) -> Self {
        assert!(
            self.redirect.is_none(),
            "cannot add children to a redirected node"
        );
        self.children.push(child);
        self
    }

    pub fn executes(
        mut self,
        command: impl Fn(&CommandContext<S>) -> Result<i32, CommandError> + Send + Sync + 'static,
    ) -> Self {
        self.command = Some(Arc::new(command));
        self
    }

    pub fn executes_command(mut self, command: Command<S>) -> Self {
        self.command = Some(command);
        self
    }

    pub fn requires(mut self, requirement: impl Fn(&S) -> bool + Send + Sync + 'static) -> Self {
        self.requirement = Some(Arc::new(requirement));
        self
    }

    pub fn requires_arc(mut self, requirement: Requirement<S>) -> Self {
        self.requirement = Some(requirement);
        self
    }

    /// Sets the packet's "restricted" flag (vanilla: permission level above 0).
    pub fn restricted(mut self, restricted: bool) -> Self {
        self.restricted = restricted;
        self
    }

    /// Continues parsing at `target` with the same source.
    pub fn redirect(self, target: NodeId) -> Self {
        self.forward(target, None, false)
    }

    /// Continues at `target` with a source mapped by `modifier`.
    pub fn redirect_with(
        self,
        target: NodeId,
        modifier: impl Fn(&CommandContext<S>) -> Result<S, CommandError> + Send + Sync + 'static,
    ) -> Self {
        self.forward(
            target,
            Some(Arc::new(move |ctx| modifier(ctx).map(|s| vec![s]))),
            false,
        )
    }

    /// Continues at `target` once per source `modifier` returns (`execute as @a ...`).
    pub fn fork(
        self,
        target: NodeId,
        modifier: impl Fn(&CommandContext<S>) -> Result<Vec<S>, CommandError> + Send + Sync + 'static,
    ) -> Self {
        self.forward(target, Some(Arc::new(modifier)), true)
    }

    /// Brigadier's `forward`. Panics if the node has children.
    pub fn forward(
        mut self,
        target: NodeId,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
    ) -> Self {
        assert!(
            self.children.is_empty(),
            "cannot forward a node with children"
        );
        self.redirect = Some(target);
        self.modifier = modifier;
        self.forks = forks;
        self
    }

    /// Server-side suggestions, advertised to the client as `minecraft:ask_server`.
    pub fn suggests(
        self,
        provider: impl Fn(&CommandContext<S>, &mut SuggestionsBuilder) + Send + Sync + 'static,
    ) -> Self {
        self.suggester(Suggester {
            kind: SuggestionKind::AskServer,
            provider: Some(Arc::new(provider)),
        })
    }

    /// Sets the suggestion type and provider. Panics on literal nodes.
    pub fn suggester(mut self, suggester: Suggester<S>) -> Self {
        assert!(
            matches!(self.kind, NodeKind::Argument { .. }),
            "only argument nodes take suggestions"
        );
        self.suggester = Some(suggester);
        self
    }
}
