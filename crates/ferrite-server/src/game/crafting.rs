//! Crafting recipes for container menus.

use ferrite_inventory::{Crafter, NoCrafting};

/// The crafter every menu uses.
pub fn crafter() -> &'static dyn Crafter {
    &NoCrafting
}
