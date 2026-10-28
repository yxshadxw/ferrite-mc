//! Indexed matching for crafting, cooking and stonecutting.

use crate::book::RecipeBook;
use crate::recipe::{
    CookingKind, CraftingShaped, CraftingShapeless, CraftingTransmute, Recipe, RecipeKind,
};
use crate::tags::ItemTags;

impl RecipeBook {
    /// Matches a crafting grid.
    ///
    /// `grid` is row-major with `width` columns; only the first
    /// `width * height` entries are read. Use [`RecipeBook::match_crafting_2x2`]
    /// or [`RecipeBook::match_crafting_3x3`] for the usual square grids.
    ///
    /// Shaped recipes are tried in every position and in mirrored form, as
    /// vanilla allows. Shapeless and transmute recipes compare the non-empty
    /// items as a multiset, with tag alternatives resolved through item tags.
    /// An empty grid never matches.
    pub fn match_crafting(
        &self,
        grid: &[Option<&str>],
        width: usize,
        height: usize,
    ) -> Option<&Recipe> {
        if width == 0 || height == 0 || width > 3 || height > 3 {
            return None;
        }
        let cells = width.checked_mul(height)?;
        if grid.len() < cells {
            return None;
        }

        let mut items = Vec::with_capacity(cells);
        for y in 0..height {
            for x in 0..width {
                if let Some(item) = grid[y * width + x] {
                    items.push(item);
                }
            }
        }
        if items.is_empty() {
            return None;
        }

        let mut candidates = Vec::new();
        for item in &items {
            if let Some(recipes) = self.crafting_candidates(item) {
                candidates.extend_from_slice(recipes);
            }
        }
        candidates.sort_unstable();
        candidates.dedup();

        for index in candidates {
            let recipe = &self.recipes()[index];
            let matched = match &recipe.kind {
                RecipeKind::CraftingShaped(shaped) => {
                    shaped.matches(grid, width, height, self.tags())
                }
                RecipeKind::CraftingShapeless(shapeless) => shapeless.matches(&items, self.tags()),
                RecipeKind::CraftingTransmute(transmute) => transmute.matches(&items, self.tags()),
                _ => false,
            };
            if matched {
                return Some(recipe);
            }
        }
        None
    }

    /// Matches a 2x2 crafting grid.
    pub fn match_crafting_2x2(&self, grid: &[Option<&str>; 4]) -> Option<&Recipe> {
        self.match_crafting(grid, 2, 2)
    }

    /// Matches a 3x3 crafting grid.
    pub fn match_crafting_3x3(&self, grid: &[Option<&str>; 9]) -> Option<&Recipe> {
        self.match_crafting(grid, 3, 3)
    }

    /// Matches a furnace-family input.
    pub fn match_cooking(&self, kind: CookingKind, item: &str) -> Option<&Recipe> {
        let candidates = self.cooking_candidates(item)?;
        for index in candidates {
            let recipe = &self.recipes()[*index];
            if let RecipeKind::Cooking(cooking) = &recipe.kind
                && cooking.kind == kind
                && cooking.ingredient.matches(item, self.tags())
            {
                return Some(recipe);
            }
        }
        None
    }

    /// Every stonecutter recipe accepting `item`, in recipe-id order.
    pub fn stonecutting_options(&self, item: &str) -> Vec<&Recipe> {
        let Some(candidates) = self.stonecutting_candidates(item) else {
            return Vec::new();
        };
        let mut recipes = Vec::with_capacity(candidates.len());
        for index in candidates {
            let recipe = &self.recipes()[*index];
            if let RecipeKind::Stonecutting(stonecutting) = &recipe.kind
                && stonecutting.ingredient.matches(item, self.tags())
            {
                recipes.push(recipe);
            }
        }
        recipes
    }
}

impl CraftingShaped {
    pub(crate) fn matches(
        &self,
        grid: &[Option<&str>],
        width: usize,
        height: usize,
        tags: &ItemTags,
    ) -> bool {
        if width < self.width || height < self.height {
            return false;
        }
        if fits_pattern(
            &self.pattern,
            self.width,
            self.height,
            grid,
            width,
            height,
            tags,
        ) {
            return true;
        }
        self.pattern != self.mirrored_pattern
            && fits_pattern(
                &self.mirrored_pattern,
                self.width,
                self.height,
                grid,
                width,
                height,
                tags,
            )
    }
}

fn fits_pattern(
    pattern: &[Vec<Option<crate::recipe::Ingredient>>],
    pattern_width: usize,
    pattern_height: usize,
    grid: &[Option<&str>],
    width: usize,
    height: usize,
    tags: &ItemTags,
) -> bool {
    for offset_y in 0..=height - pattern_height {
        for offset_x in 0..=width - pattern_width {
            if matches_at(
                pattern,
                pattern_width,
                pattern_height,
                offset_x,
                offset_y,
                grid,
                width,
                height,
                tags,
            ) {
                return true;
            }
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn matches_at(
    pattern: &[Vec<Option<crate::recipe::Ingredient>>],
    pattern_width: usize,
    pattern_height: usize,
    offset_x: usize,
    offset_y: usize,
    grid: &[Option<&str>],
    width: usize,
    height: usize,
    tags: &ItemTags,
) -> bool {
    for y in 0..height {
        for x in 0..width {
            let item = grid[y * width + x];
            let inside = x >= offset_x
                && x < offset_x + pattern_width
                && y >= offset_y
                && y < offset_y + pattern_height;
            if inside {
                let cell = &pattern[y - offset_y][x - offset_x];
                match (cell, item) {
                    (Some(ingredient), Some(item)) => {
                        if !ingredient.matches(item, tags) {
                            return false;
                        }
                    }
                    (None, None) => {}
                    _ => return false,
                }
            } else if item.is_some() {
                return false;
            }
        }
    }
    true
}

impl CraftingShapeless {
    pub(crate) fn matches(&self, items: &[&str], tags: &ItemTags) -> bool {
        if self.ingredients.len() != items.len() || items.is_empty() {
            return false;
        }
        let mut matched_item = vec![usize::MAX; items.len()];
        for ingredient in 0..self.ingredients.len() {
            let mut visited = vec![false; items.len()];
            if !augment(
                ingredient,
                &self.ingredients,
                items,
                tags,
                &mut visited,
                &mut matched_item,
            ) {
                return false;
            }
        }
        true
    }
}

fn augment(
    ingredient: usize,
    ingredients: &[crate::recipe::Ingredient],
    items: &[&str],
    tags: &ItemTags,
    visited: &mut [bool],
    matched_item: &mut [usize],
) -> bool {
    for item in 0..items.len() {
        if visited[item] || !ingredients[ingredient].matches(items[item], tags) {
            continue;
        }
        visited[item] = true;
        let previous = matched_item[item];
        if previous == usize::MAX
            || augment(previous, ingredients, items, tags, visited, matched_item)
        {
            matched_item[item] = ingredient;
            return true;
        }
    }
    false
}

impl CraftingTransmute {
    pub(crate) fn matches(&self, items: &[&str], tags: &ItemTags) -> bool {
        let (min, max) = match self.material_count {
            Some(count) => (count.min as usize, count.max as usize),
            None => (1, 1),
        };
        if items.len() < 1 + min || items.len() > 1 + max {
            return false;
        }

        for input in 0..items.len() {
            if !self.input.matches(items[input], tags) {
                continue;
            }
            if items
                .iter()
                .enumerate()
                .all(|(index, item)| index == input || self.material.matches(item, tags))
            {
                return true;
            }
        }
        false
    }
}
