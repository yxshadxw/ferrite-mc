//! Inlining named density-function references.

use std::collections::HashMap;

use thiserror::Error;

use crate::WorldgenData;
use crate::density::DensityFunction;
use crate::id::ResourceId;
use crate::noise_settings::NoiseRouter;

/// Failure while inlining density-function references.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ResolveError {
    #[error("unknown density function `{0}`")]
    Unknown(ResourceId),
    #[error("cycle in density function references: {0}")]
    Cycle(String),
}

impl DensityFunction {
    /// Returns an owned copy of this tree with every named reference replaced by its
    /// definition.
    ///
    /// Definitions are resolved once and cached per name. A reference cycle is reported as
    /// [`ResolveError::Cycle`] instead of recursing forever.
    pub fn resolve_references(&self, data: &WorldgenData) -> Result<Self, ResolveError> {
        Resolver::new(data).resolve(self.clone())
    }
}

impl NoiseRouter {
    /// Resolves every density function of the router, sharing one reference cache.
    pub fn resolve_references(&self, data: &WorldgenData) -> Result<Self, ResolveError> {
        let mut resolver = Resolver::new(data);
        Ok(Self {
            chunk_surface_level: resolver.resolve(self.chunk_surface_level.clone())?,
            continents: resolver.resolve(self.continents.clone())?,
            depth: resolver.resolve(self.depth.clone())?,
            erosion: resolver.resolve(self.erosion.clone())?,
            final_density: resolver.resolve(self.final_density.clone())?,
            ridges: resolver.resolve(self.ridges.clone())?,
            temperature: resolver.resolve(self.temperature.clone())?,
            vegetation: resolver.resolve(self.vegetation.clone())?,
        })
    }
}

struct Resolver<'a> {
    data: &'a WorldgenData,
    cache: HashMap<ResourceId, DensityFunction>,
    stack: Vec<ResourceId>,
}

impl<'a> Resolver<'a> {
    fn new(data: &'a WorldgenData) -> Self {
        Self {
            data,
            cache: HashMap::new(),
            stack: Vec::new(),
        }
    }

    fn resolve(&mut self, function: DensityFunction) -> Result<DensityFunction, ResolveError> {
        match function {
            DensityFunction::Constant(value) => Ok(DensityFunction::Constant(value)),
            DensityFunction::Reference(id) => self.resolve_reference(&id),
            DensityFunction::Node(node) => Ok(DensityFunction::Node(
                node.try_map_children(&mut |child| self.resolve(child))?,
            )),
        }
    }

    fn resolve_reference(&mut self, id: &ResourceId) -> Result<DensityFunction, ResolveError> {
        if let Some(cached) = self.cache.get(id) {
            return Ok(cached.clone());
        }
        if let Some(start) = self.stack.iter().position(|current| current == id) {
            let mut path: Vec<String> = self.stack[start..]
                .iter()
                .map(ToString::to_string)
                .collect();
            path.push(id.to_string());
            return Err(ResolveError::Cycle(path.join(" -> ")));
        }
        let definition = self
            .data
            .density_functions
            .get(id)
            .cloned()
            .ok_or_else(|| ResolveError::Unknown(id.clone()))?;

        self.stack.push(id.clone());
        let resolved = self.resolve(definition);
        self.stack.pop();
        let resolved = resolved?;
        self.cache.insert(id.clone(), resolved.clone());
        Ok(resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::ResolveError;
    use crate::WorldgenData;
    use crate::density::{DensityFunction, DensityNode};
    use crate::id::ResourceId;

    fn id(value: &str) -> ResourceId {
        ResourceId::parse(value).unwrap()
    }

    #[test]
    fn inlines_and_caches_shared_references() {
        let mut data = WorldgenData::default();
        data.density_functions.insert(
            id("minecraft:a"),
            DensityFunction::Node(DensityNode::Add {
                left: Box::new(DensityFunction::Reference(id("b"))),
                right: Box::new(DensityFunction::Reference(id("b"))),
            }),
        );
        data.density_functions
            .insert(id("b"), DensityFunction::Constant(2.0));

        let resolved = DensityFunction::Reference(id("a"))
            .resolve_references(&data)
            .unwrap();
        assert_eq!(resolved.reference_count(), 0);
        assert_eq!(resolved.node_count(), 3);
    }

    #[test]
    fn detects_reference_cycles() {
        let mut data = WorldgenData::default();
        data.density_functions
            .insert(id("a"), DensityFunction::Reference(id("b")));
        data.density_functions.insert(
            id("b"),
            DensityFunction::Node(DensityNode::Add {
                left: Box::new(DensityFunction::Reference(id("a"))),
                right: Box::new(DensityFunction::Constant(1.0)),
            }),
        );

        let error = DensityFunction::Reference(id("a"))
            .resolve_references(&data)
            .unwrap_err();
        let ResolveError::Cycle(path) = error else {
            panic!("expected a cycle error");
        };
        assert!(path.contains("minecraft:a"), "{path}");
        assert!(path.contains("minecraft:b"), "{path}");
    }

    #[test]
    fn reports_unknown_references() {
        let data = WorldgenData::default();
        let error = DensityFunction::Reference(id("missing"))
            .resolve_references(&data)
            .unwrap_err();
        assert_eq!(error, ResolveError::Unknown(id("missing")));
    }
}
