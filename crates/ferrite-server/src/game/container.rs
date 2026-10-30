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
}

impl Default for Containers {
    fn default() -> Self {
        Self {
            inventory: Menu::inventory(),
            open: None,
            counter: 0,
            thrown: Vec::new(),
        }
    }
}

impl Containers {
    /// Vanilla `containerMenu`.
    pub fn current(&mut self) -> &mut Menu {
        self.open.as_mut().unwrap_or(&mut self.inventory)
    }

    /// Vanilla `nextContainerCounter`.
    #[allow(dead_code)]
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

    /// Vanilla `containerMenu.broadcastChanges()`, once per tick.
    pub fn broadcast_inventory_changes(&mut self) {
        let out = self.menus.current().broadcast_changes(&self.inventory);
        emit(self, out);
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
        self.menus.thrown.extend(drops);
        emit(self, out);
    }

    /// Vanilla `doCloseContainer`: the open menu (or the inventory screen's grid and cursor)
    /// returns its items, and the inventory menu takes over what the client was shown.
    pub fn close_container(&mut self) {
        let infinite = self.infinite_materials();
        let mut clicker = Clicker::new(&mut self.inventory, infinite, crafter());
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
    /// then the client gets `open_screen` and the menu's full contents.
    #[allow(dead_code)] // Opened by crafting tables (I3).
    pub fn open_menu(&mut self, make: impl FnOnce(i32) -> Menu, title: &str) {
        // Vanilla `closeContainer`: tell the client, then close on the server.
        if let Some(window) = self.menus.open.as_ref().map(Menu::container_id) {
            send(
                self,
                &ClientboundContainerClose {
                    window: VarInt(window),
                },
            );
            self.close_container();
        }
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
        emit(self, vec![full]);
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
