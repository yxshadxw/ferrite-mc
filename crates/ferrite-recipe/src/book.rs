//! Loading a generated recipe data pack and building lookup indexes.

use crate::files::{json_files_flat, read_json, relative_stem};
use crate::recipe::{Ingredient, Recipe, RecipeKind, normalize_id, parse_recipe};
use crate::tags::ItemTags;
use crate::{Error, Result};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// All recipes and item tags loaded from one generated `data/<namespace>`
/// directory.
///
/// Crafting, cooking and stonecutting lookups use an index over the most
/// selective ingredient of each recipe (and over recipe results), so matching
/// does not scan all 1,763 recipes.
#[derive(Debug)]
pub struct RecipeBook {
    recipes: Vec<Recipe>,
    tags: ItemTags,
    by_id: HashMap<String, usize>,
    crafting_by_item: HashMap<String, Vec<usize>>,
    cooking_by_item: HashMap<String, Vec<usize>>,
    stonecutting_by_item: HashMap<String, Vec<usize>>,
    by_result: HashMap<String, Vec<usize>>,
}

impl RecipeBook {
    /// Loads recipes and tags from a generated data directory.
    ///
    /// `path` may be either `.../data/minecraft` or the generated root
    /// `.../generated`; the latter is detected automatically.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let data_dir = if path.join("recipe").is_dir() {
            path.to_path_buf()
        } else if path.join("data/minecraft/recipe").is_dir() {
            path.join("data/minecraft")
        } else {
            path.to_path_buf()
        };
        let namespace = data_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("minecraft")
            .to_owned();
        Self::load_named(&data_dir, &namespace)
    }

    /// Loads from explicit recipe and item-tag directories in the
    /// `minecraft` namespace.
    pub fn load_parts(recipe_dir: impl AsRef<Path>, tags_dir: impl AsRef<Path>) -> Result<Self> {
        Self::load_dirs(recipe_dir.as_ref(), tags_dir.as_ref(), "minecraft")
    }

    fn load_named(data_dir: &Path, namespace: &str) -> Result<Self> {
        Self::load_dirs(
            &data_dir.join("recipe"),
            &data_dir.join("tags/item"),
            namespace,
        )
    }

    fn load_dirs(recipe_dir: &Path, tags_dir: &Path, namespace: &str) -> Result<Self> {
        let tags = ItemTags::load(tags_dir, namespace)?;
        let files = json_files_flat(recipe_dir)?;
        let mut recipes = Vec::with_capacity(files.len());
        for path in files {
            let relative = relative_stem(&path, recipe_dir).ok_or_else(|| {
                Error::recipe(
                    "?",
                    format!("path {} is outside the recipe dir", path.display()),
                )
            })?;
            let id = format!("{namespace}:{relative}");
            let value = read_json(&path)?;
            recipes.push(parse_recipe(&id, &value)?);
        }
        recipes.sort_by(|left, right| left.id.cmp(&right.id));

        let mut by_id = HashMap::with_capacity(recipes.len());
        for (index, recipe) in recipes.iter().enumerate() {
            if by_id.insert(recipe.id.clone(), index).is_some() {
                return Err(Error::recipe(&recipe.id, "duplicate recipe id"));
            }
        }

        let mut book = Self {
            recipes,
            tags,
            by_id,
            crafting_by_item: HashMap::new(),
            cooking_by_item: HashMap::new(),
            stonecutting_by_item: HashMap::new(),
            by_result: HashMap::new(),
        };
        book.build_indexes();
        Ok(book)
    }

    /// Every loaded recipe, sorted by id.
    pub fn recipes(&self) -> &[Recipe] {
        &self.recipes
    }

    /// Number of loaded recipes.
    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    /// Whether no recipes were loaded.
    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }

    /// The resolved item tags.
    pub fn tags(&self) -> &ItemTags {
        &self.tags
    }

    /// Looks up a recipe by id. A missing `minecraft:` namespace is accepted.
    pub fn recipe(&self, id: &str) -> Option<&Recipe> {
        let id = normalize_id(id);
        self.by_id
            .get(id.as_ref())
            .map(|index| &self.recipes[*index])
    }

    /// Recipe counts keyed by the full 26.3 type name.
    pub fn counts_by_type(&self) -> BTreeMap<&str, usize> {
        let mut counts = BTreeMap::new();
        for recipe in &self.recipes {
            *counts.entry(recipe.type_name()).or_insert(0) += 1;
        }
        counts
    }

    /// Recipes that produce `item`.
    pub fn recipes_with_result(&self, item: &str) -> Vec<&Recipe> {
        let item = normalize_id(item);
        self.by_result
            .get(item.as_ref())
            .map(|indices| indices.iter().map(|index| &self.recipes[*index]).collect())
            .unwrap_or_default()
    }

    fn build_indexes(&mut self) {
        let mut crafting_by_item: HashMap<String, Vec<usize>> = HashMap::new();
        let mut cooking_by_item: HashMap<String, Vec<usize>> = HashMap::new();
        let mut stonecutting_by_item: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_result: HashMap<String, Vec<usize>> = HashMap::new();

        for (index, recipe) in self.recipes.iter().enumerate() {
            if let Some(result) = recipe.result() {
                by_result
                    .entry(result.item.clone())
                    .or_default()
                    .push(index);
            }

            match &recipe.kind {
                RecipeKind::CraftingShaped(shaped) => {
                    let ingredients = shaped
                        .pattern
                        .iter()
                        .flatten()
                        .filter_map(Option::as_ref)
                        .collect::<Vec<_>>();
                    if let Some(items) = best_anchor(ingredients, &self.tags) {
                        for item in items {
                            crafting_by_item.entry(item).or_default().push(index);
                        }
                    }
                }
                RecipeKind::CraftingShapeless(shapeless) => {
                    let ingredients = shapeless.ingredients.iter().collect::<Vec<_>>();
                    if let Some(items) = best_anchor(ingredients, &self.tags) {
                        for item in items {
                            crafting_by_item.entry(item).or_default().push(index);
                        }
                    }
                }
                RecipeKind::CraftingTransmute(transmute) => {
                    let ingredients = vec![&transmute.input, &transmute.material];
                    if let Some(items) = best_anchor(ingredients, &self.tags) {
                        for item in items {
                            crafting_by_item.entry(item).or_default().push(index);
                        }
                    }
                }
                RecipeKind::Cooking(cooking) => {
                    for item in cooking.ingredient.resolved_items(&self.tags) {
                        cooking_by_item.entry(item).or_default().push(index);
                    }
                }
                RecipeKind::Stonecutting(stonecutting) => {
                    for item in stonecutting.ingredient.resolved_items(&self.tags) {
                        stonecutting_by_item.entry(item).or_default().push(index);
                    }
                }
                RecipeKind::SmithingTransform(_)
                | RecipeKind::SmithingTrim(_)
                | RecipeKind::Special(_) => {}
            }
        }

        finish_index(&mut crafting_by_item);
        finish_index(&mut cooking_by_item);
        finish_index(&mut stonecutting_by_item);
        finish_index(&mut by_result);

        self.crafting_by_item = crafting_by_item;
        self.cooking_by_item = cooking_by_item;
        self.stonecutting_by_item = stonecutting_by_item;
        self.by_result = by_result;
    }

    pub(crate) fn crafting_candidates(&self, item: &str) -> Option<&[usize]> {
        lookup(&self.crafting_by_item, item)
    }

    pub(crate) fn cooking_candidates(&self, item: &str) -> Option<&[usize]> {
        lookup(&self.cooking_by_item, item)
    }

    pub(crate) fn stonecutting_candidates(&self, item: &str) -> Option<&[usize]> {
        lookup(&self.stonecutting_by_item, item)
    }
}

fn best_anchor<'a>(
    ingredients: impl IntoIterator<Item = &'a Ingredient>,
    tags: &ItemTags,
) -> Option<Vec<String>> {
    let mut best: Option<Vec<String>> = None;
    let mut any = false;
    for ingredient in ingredients {
        any = true;
        let items = ingredient.resolved_items(tags);
        if items.is_empty() {
            // This ingredient can never be satisfied, so the recipe cannot match.
            return None;
        }
        let replace = match &best {
            None => true,
            Some(best) => items.len() < best.len(),
        };
        if replace {
            best = Some(items);
        }
    }
    if any { best } else { None }
}

fn finish_index(index: &mut HashMap<String, Vec<usize>>) {
    for recipes in index.values_mut() {
        recipes.sort_unstable();
        recipes.dedup();
    }
}

fn lookup<'a>(map: &'a HashMap<String, Vec<usize>>, item: &str) -> Option<&'a [usize]> {
    if let Some(value) = map.get(item) {
        return Some(value.as_slice());
    }
    if item.contains(':') {
        return None;
    }
    let namespaced = format!("minecraft:{item}");
    map.get(&namespaced).map(Vec::as_slice)
}
