//! A player's inventory and open container menu on the server (vanilla `ServerPlayer`'s
//! `inventoryMenu`/`containerMenu` and the container handlers of
//! `ServerGamePacketListenerImpl`).

use ferrite_inventory::{ClickInput, Clicker, Crafter, Menu, PlayerInventory, Sync};
use ferrite_item::{HashedSlot, ItemStack};
use ferrite_protocol::VarInt;
use ferrite_protocol::packets::play::{
    ClientboundContainerClose, ContainerClick, ContainerContent, ContainerSlot, CreativeSlot,
    OpenScreen, SetCursorItem,
};

use super::block_container::{self, Opened};
use super::region::{Player, send};

/// The player's menus: the inventory menu (container 0) and the open container, if any.
#[derive(Debug)]
pub struct Containers {
    /// Vanilla `inventoryMenu`.
    pub inventory: Menu,
    /// Vanilla `containerMenu` when it is not the inventory menu.
    pub open: Option<Menu>,
    /// Vanilla `containerCounter`, the id of the last opened container (1..=100).
    counter: i32,
    /// Stacks the player dropped, spawned as item entities by the region this tick.
    pub thrown: Vec<ItemStack>,
    /// The block the open menu belongs to, for vanilla `stillValid` and shared contents.
    pub open_at: Option<OpenAt>,
    /// Contents of broken inventory blocks, spilled as item entities by the region this tick.
    pub spilled: Vec<([i32; 3], ItemStack)>,
}

/// What an open menu is attached to.
#[derive(Clone, Debug, PartialEq)]
pub enum OpenAt {
    /// A crafting table, whose grid belongs to the menu.
    Table([i32; 3]),
    /// A chest, trapped chest or barrel, whose items live in its block entities.
    Inventory(Opened),
}

impl Default for Containers {
    fn default() -> Self {
        Self {
            inventory: Menu::inventory(),
            open: None,
            counter: 0,
            thrown: Vec::new(),
            open_at: None,
            spilled: Vec::new(),
        }
    }
}

impl Containers {
    /// Vanilla `containerMenu`.
    pub fn current(&mut self) -> &mut Menu {
        self.open.as_mut().unwrap_or(&mut self.inventory)
    }

    /// Vanilla `nextContainerCounter`.
    fn next_id(&mut self) -> i32 {
        self.counter = self.counter % 100 + 1;
        self.counter
    }
}

/// The crafting recipes in effect for every menu.
pub fn crafter() -> &'static dyn Crafter {
    super::crafting::crafter()
}

/// Sends menu sync packets to a player.
fn emit(player: &Player, out: Vec<Sync>) {
    for sync in out {
        match sync {
            Sync::Content {
                container_id,
                state_id,
                items,
                carried,
            } => send(
                player,
                &ContainerContent {
                    window: VarInt(container_id),
                    state: VarInt(state_id),
                    slots: items,
                    carried,
                },
            ),
            Sync::Slot {
                container_id,
                state_id,
                slot,
                item,
            } => send(
                player,
                &ContainerSlot {
                    window: VarInt(container_id),
                    state: VarInt(state_id),
                    slot,
                    item,
                },
            ),
            Sync::Carried(item) => send(player, &SetCursorItem { item }),
        }
    }
}

impl Player {
    fn infinite_materials(&self) -> bool {
        self.gamemode == 1
    }

    /// Sends the whole inventory menu (vanilla `initInventoryMenu`), on join.
    pub fn init_inventory_menu(&mut self) {
        let full = self.menus.inventory.broadcast_full_state(&self.inventory);
        emit(self, vec![full]);
    }

    /// Broadcasts the current menu's changed slots (vanilla `broadcastChanges`).
    pub fn sync_containers(&mut self) {
        let out = self.menus.current().broadcast_changes(&self.inventory);
        emit(self, out);
    }

    /// Once per tick, like `ServerPlayer.tick`: `containerMenu.broadcastChanges()`, then the
    /// open menu closes if it is no longer valid.
    pub fn tick_containers(&mut self) {
        self.load_shared_contents();
        let out = self.menus.current().broadcast_changes(&self.inventory);
        emit(self, out);
        if !self.container_still_valid() {
            self.close_open_container();
        }
    }

    /// Vanilla `AbstractContainerMenu.stillValid` for a block menu: the block is still there
    /// and within the player's block interaction range plus 4 (`canInteractWithBlock(pos, 4)`).
    fn container_still_valid(&self) -> bool {
        let Some(source) = self.source.as_ref() else {
            return true;
        };
        match &self.menus.open_at {
            None => true,
            Some(OpenAt::Table([x, y, z])) => {
                let table = ferrite_data::blocks::default_state("crafting_table").map(u32::from);
                let present = source.block(*x, *y, *z);
                present.is_some() && present == table && self.within_reach([*x, *y, *z])
            }
            // A double chest stays usable while both halves are there and in reach.
            Some(OpenAt::Inventory(opened)) => {
                block_container::still_there(source, opened)
                    && opened.parts.iter().all(|&pos| self.within_reach(pos))
            }
        }
    }

    /// Vanilla `Player.canInteractWithBlock(pos, 4.0)`: the block box is closer to the eyes
    /// than the block interaction range plus 4.
    fn within_reach(&self, [x, y, z]: [i32; 3]) -> bool {
        let eye = [self.pos[0], self.pos[1] + 1.62, self.pos[2]];
        let distance_sq: f64 = [x, y, z]
            .iter()
            .zip(eye)
            .map(|(&min, e)| {
                let min = f64::from(min);
                let d = (min - e).max(e - (min + 1.0)).max(0.0);
                d * d
            })
            .sum();
        let range = if self.gamemode == 1 { 5.0 } else { 4.5 } + 4.0;
        distance_sq < range * range
    }

    /// Vanilla `closeContainer`: tells the client, then closes on the server.
    fn close_open_container(&mut self) {
        if let Some(window) = self.menus.open.as_ref().map(Menu::container_id) {
            send(
                self,
                &ClientboundContainerClose {
                    window: VarInt(window),
                },
            );
            self.close_container();
        }
    }

    /// Vanilla `handleContainerClick`.
    pub fn handle_container_click(&mut self, p: ContainerClick<HashedSlot>) {
        let infinite = self.infinite_materials();
        let menu = self
            .menus
            .open
            .as_mut()
            .unwrap_or(&mut self.menus.inventory);
        if menu.container_id() != p.window.0 {
            return;
        }
        if self.gamemode == 3 {
            let full = menu.broadcast_full_state(&self.inventory);
            emit(self, vec![full]);
            return;
        }
        let Some(input) = ClickInput::from_id(p.input.0) else {
            return;
        };
        if !self.container_still_valid() {
            return;
        }
        let menu = self
            .menus
            .open
            .as_mut()
            .unwrap_or(&mut self.menus.inventory);
        // Another viewer may have changed a shared container since this player last saw it.
        if let (Some(OpenAt::Inventory(opened)), Some(source)) = (&self.menus.open_at, &self.source)
        {
            let items = block_container::load(source, opened);
            menu.container_mut().clone_from_slice(&items);
        }
        let mut clicker = Clicker::new(&mut self.inventory, infinite, crafter());
        let out = menu.handle_click(
            &mut clicker,
            p.state.0,
            p.slot,
            p.button,
            input,
            &p.changed,
            p.carried,
        );
        let drops = clicker.drops;
        if let (Some(OpenAt::Inventory(opened)), Some(source)) = (&self.menus.open_at, &self.source)
        {
            block_container::store(source, opened, menu.container());
        }
        self.menus.thrown.extend(drops);
        emit(self, out);
    }

    /// Copies a shared container's current items into the open menu.
    fn load_shared_contents(&mut self) {
        if let (Some(OpenAt::Inventory(opened)), Some(source), Some(menu)) =
            (&self.menus.open_at, &self.source, self.menus.open.as_mut())
        {
            let items = block_container::load(source, opened);
            if items.len() == menu.container().len() {
                menu.container_mut().clone_from_slice(&items);
            }
        }
    }

    /// Vanilla `doCloseContainer`: the open menu (or the inventory screen's grid and cursor)
    /// returns its items, and the inventory menu takes over what the client was shown.
    pub fn close_container(&mut self) {
        let infinite = self.infinite_materials();
        let mut clicker = Clicker::new(&mut self.inventory, infinite, crafter());
        self.menus.open_at = None;
        match self.menus.open.take() {
            Some(mut open) => {
                open.close(&mut clicker);
                self.menus.inventory.transfer_state(&open);
            }
            None => self.menus.inventory.close(&mut clicker),
        }
        let drops = clicker.drops;
        self.menus.thrown.extend(drops);
    }

    /// Opens a container screen (vanilla `ServerPlayer.openMenu`): any open menu closes first,
    /// then the client gets `open_screen` and the menu's full contents. `at` is the block the
    /// menu belongs to.
    pub fn open_menu(&mut self, make: impl FnOnce(i32) -> Menu, title: &str, at: Option<OpenAt>) {
        self.close_open_container();
        let id = self.menus.next_id();
        let mut menu = make(id);
        let Some(kind) = menu.kind().menu_type() else {
            return;
        };
        let Some(menu_type) = ferrite_data::registries::id("minecraft:menu", &kind) else {
            tracing::warn!(kind, "unknown menu type");
            return;
        };
        send(
            self,
            &OpenScreen {
                window: VarInt(id),
                menu: VarInt(menu_type),
                title: ferrite_protocol::types::Translatable(title.into()),
            },
        );
        let full = menu.broadcast_full_state(&self.inventory);
        self.menus.open = Some(menu);
        self.menus.open_at = at;
        emit(self, vec![full]);
    }

    /// Vanilla `useWithoutItem` of blocks with a menu: a crafting table opens `CraftingMenu`
    /// (`CraftingTableBlock`), a chest or barrel a `ChestMenu` over its items (`ChestBlock`,
    /// `BarrelBlock`). Using them opens the menu instead of placing the held item; returns
    /// whether the block was used.
    pub fn use_block(&mut self, location: i64) -> bool {
        // Doors, buttons, beds, signs, buckets and cauldrons first; menus follow.
        if super::blocks::use_block(self, location) {
            return true;
        }
        let pos = ferrite_protocol::types::BlockPos::unpack(location);
        let Some(source) = self.source.clone() else {
            return false;
        };
        let table = ferrite_data::blocks::default_state("crafting_table").map(u32::from);
        let state = source.block(pos.x, pos.y, pos.z);
        if state.is_some() && state == table {
            self.open_menu(
                Menu::crafting,
                "container.crafting",
                Some(OpenAt::Table([pos.x, pos.y, pos.z])),
            );
            return true;
        }
        let Some(opened) = block_container::at(&source, [pos.x, pos.y, pos.z]) else {
            return false;
        };
        let items = block_container::load(&source, &opened);
        let (rows, title) = (opened.rows, opened.title);
        self.open_menu(
            |id| {
                let mut menu = Menu::generic(id, rows);
                menu.container_mut().clone_from_slice(&items);
                menu
            },
            title,
            Some(OpenAt::Inventory(opened)),
        );
        true
    }

    /// Vanilla `handleSetCreativeModeSlot`: slots 1..=45 of the inventory menu, or a drop for a
    /// negative slot.
    pub fn handle_creative_slot(&mut self, p: CreativeSlot<ItemStack>) {
        if !self.infinite_materials() {
            return;
        }
        let stack = p.item;
        let valid = stack.is_empty() || stack.count <= stack.max_stack_size();
        if !valid {
            return;
        }
        if (1..=45).contains(&p.slot) {
            let slot = p.slot as usize;
            self.menus
                .inventory
                .set_slot(&mut self.inventory, crafter(), slot, stack.clone());
            self.menus.inventory.set_remote_slot(slot, &stack);
            let out = self.menus.inventory.broadcast_changes(&self.inventory);
            emit(self, out);
        } else if p.slot < 0 && !stack.is_empty() {
            self.menus.thrown.push(stack);
        }
    }

    /// Vanilla `ServerPlayer.drop(all)` for the Q key: drops one or all of the selected stack.
    /// The client already removed it, so the slot is not sent back.
    pub fn drop_selected(&mut self, all: bool) {
        if self.gamemode == 3 {
            return;
        }
        let selected = self.inventory.selected_slot();
        let held = self.inventory.get(selected).count;
        if held <= 0 {
            return;
        }
        let dropped = self.inventory.remove(selected, if all { held } else { 1 });
        let left = self.inventory.get(selected).clone();
        let menu = self.menus.current();
        if let Some(slot) = menu.menu_slot_of(selected) {
            menu.set_remote_slot(slot, &left);
        }
        self.menus.thrown.push(dropped);
    }

    /// Vanilla `SWAP_ITEM_WITH_OFFHAND`.
    pub fn swap_hands(&mut self) {
        if self.gamemode == 3 {
            return;
        }
        let selected = self.inventory.selected_slot();
        let main = self.inventory.get(selected).clone();
        let off = self.inventory.get(ferrite_inventory::OFFHAND).clone();
        self.inventory.set(selected, off);
        self.inventory.set(ferrite_inventory::OFFHAND, main);
    }

    /// The stack in a hand: 0 the main hand, 1 the offhand.
    pub fn hand_slot(&self, hand: i32) -> usize {
        if hand == 0 {
            self.inventory.selected_slot()
        } else {
            ferrite_inventory::OFFHAND
        }
    }
}

/// The player's inventory as stored and restored by [`super::playerdata`].
pub fn restore(saved: Option<&super::playerdata::Saved>) -> PlayerInventory {
    saved.map_or_else(PlayerInventory::new, super::playerdata::Saved::inventory)
}
