//! Crafting recipes for container menus, matched with `ferrite-recipe`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ferrite_inventory::{Crafter, NoCrafting};
use ferrite_item::ItemStack;
use ferrite_recipe::RecipeBook;

/// Vanilla `CraftingMenu.slotChangedCraftingGrid`: the grid's result from the recipe book.
///
/// Special recipes (`crafting_special_*`, such as firework rockets or banner copying) have no
/// fixed result in `ferrite-recipe` and craft nothing. No 26.3 crafting result carries a
/// component patch, so a result is its item and count.
struct RecipeCrafter(RecipeBook);

impl Crafter for RecipeCrafter {
    fn craft(&self, grid: &[ItemStack], width: usize) -> ItemStack {
        if width == 0 || !grid.len().is_multiple_of(width) {
            return ItemStack::empty();
        }
        let names: Vec<Option<&str>> = grid
            .iter()
            .map(|stack| if stack.is_empty() { None } else { stack.name() })
            .collect();
        self.0
            .match_crafting(&names, width, grid.len() / width)
            .and_then(|recipe| recipe.result())
            .and_then(|result| {
                let count = i32::try_from(result.count).ok()?;
                Some(ItemStack::from_name(&result.item)?.copy_with_count(count))
            })
            .unwrap_or_default()
    }
}

/// The generated vanilla data pack: `vanilla/26.3/generated` under the working directory, or
/// the repository checkout this binary was built from.
fn data_dir() -> Option<PathBuf> {
    let relative = Path::new("vanilla/26.3/generated/data/minecraft");
    [
        relative.to_path_buf(),
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(relative),
    ]
    .into_iter()
    .find(|dir| dir.join("recipe").is_dir())
}

fn load() -> Option<RecipeCrafter> {
    let Some(dir) = data_dir() else {
        tracing::warn!("no vanilla recipe data found; crafting is disabled");
        return None;
    };
    match RecipeBook::load(&dir) {
        Ok(book) => {
            tracing::info!(recipes = book.len(), "loaded crafting recipes");
            Some(RecipeCrafter(book))
        }
        Err(error) => {
            tracing::error!(%error, "failed to load recipes; crafting is disabled");
            None
        }
    }
}

static BOOK: OnceLock<Option<RecipeCrafter>> = OnceLock::new();

/// Loads the recipe book now, so the first crafting click does not stall a region tick.
pub fn preload() {
    BOOK.get_or_init(load);
}

/// The crafter every menu uses.
pub fn crafter() -> &'static dyn Crafter {
    match BOOK.get_or_init(load) {
        Some(book) => book,
        None => &NoCrafting,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(name: &str, count: i32) -> ItemStack {
        ItemStack::from_name(name).unwrap().copy_with_count(count)
    }

    #[test]
    fn crafts_planks_sticks_and_tables_in_both_grids() {
        let crafter = crafter();
        let empty = ItemStack::empty;
        // A log anywhere in the 2x2 grid makes four planks.
        let grid = [empty(), empty(), stack("oak_log", 3), empty()];
        assert_eq!(crafter.craft(&grid, 2), stack("oak_planks", 4));
        // Two planks stacked vertically make four sticks, in any column of the 3x3 grid.
        let mut grid = vec![empty(); 9];
        grid[2] = stack("oak_planks", 1);
        grid[5] = stack("oak_planks", 1);
        assert_eq!(crafter.craft(&grid, 3), stack("stick", 4));
        let grid: [ItemStack; 4] = std::array::from_fn(|_| stack("birch_planks", 1));
        assert_eq!(crafter.craft(&grid, 2), stack("crafting_table", 1));
        // Side by side they are a pressure plate; dirt and stone are no recipe.
        let grid = [
            stack("oak_planks", 1),
            stack("oak_planks", 1),
            empty(),
            empty(),
        ];
        assert_eq!(crafter.craft(&grid, 2), stack("oak_pressure_plate", 1));
        let grid = [stack("dirt", 1), stack("stone", 1), empty(), empty()];
        assert_eq!(crafter.craft(&grid, 2), empty());
        // A three-wide recipe (a chest) needs the 3x3 grid.
        let mut grid: Vec<ItemStack> = (0..9).map(|_| stack("oak_planks", 1)).collect();
        grid[4] = empty();
        assert_eq!(crafter.craft(&grid, 3), stack("chest", 1));
        assert_eq!(crafter.craft(&[], 2), empty());
    }
}
