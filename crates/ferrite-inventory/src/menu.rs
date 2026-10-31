//! Container menus and `container_click` semantics (vanilla `AbstractContainerMenu`).

use ferrite_item::{HashedSlot, ItemStack};

use crate::inventory::{
    CHEST, FEET, HEAD, HOTBAR, ITEMS, LEGS, OFFHAND, PlayerInventory, equipment_slot, is_stackable,
    max_in_slot, prevents_armor_change,
};

/// The slot number of a click outside the window, which drops the carried stack.
pub const SLOT_OUTSIDE: i16 = -999;

/// Vanilla `ContainerInput` (formerly `ClickType`), the `container_click` mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickInput {
    Pickup,
    QuickMove,
    Swap,
    Clone,
    Throw,
    QuickCraft,
    PickupAll,
}

impl ClickInput {
    /// The input with a network id.
    pub fn from_id(id: i32) -> Option<Self> {
        Some(match id {
            0 => Self::Pickup,
            1 => Self::QuickMove,
            2 => Self::Swap,
            3 => Self::Clone,
            4 => Self::Throw,
            5 => Self::QuickCraft,
            6 => Self::PickupAll,
            _ => return None,
        })
    }

    /// The network id.
    pub fn id(self) -> i32 {
        self as i32
    }
}

/// The furnace family (vanilla `AbstractFurnaceMenu` and its three block menus).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FurnaceKind {
    Furnace,
    BlastFurnace,
    Smoker,
}

impl FurnaceKind {
    /// The `minecraft:menu` registry name.
    pub const fn menu_type(self) -> &'static str {
        match self {
            Self::Furnace => "minecraft:furnace",
            Self::BlastFurnace => "minecraft:blast_furnace",
            Self::Smoker => "minecraft:smoker",
        }
    }

    /// The translation key of the screen title.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Furnace => "container.furnace",
            Self::BlastFurnace => "container.blast_furnace",
            Self::Smoker => "container.smoker",
        }
    }

    /// The block this menu belongs to.
    pub const fn block(self) -> &'static str {
        match self {
            Self::Furnace => "minecraft:furnace",
            Self::BlastFurnace => "minecraft:blast_furnace",
            Self::Smoker => "minecraft:smoker",
        }
    }
}

/// Vanilla `AbstractFurnaceMenu` container data slots.
pub const FURNACE_LIT: usize = 0;
pub const FURNACE_COOK_PROGRESS: usize = 1;
pub const FURNACE_COOK_TOTAL: usize = 2;
pub const FURNACE_BURN_LEFT: usize = 3;
pub const FURNACE_BURN_TOTAL: usize = 4;
/// Number of container data slots of a furnace family menu.
pub const FURNACE_DATA_SLOTS: usize = 5;

/// One furnace-family recipe result, as [`Crafter::cook`] reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct CookingOutput {
    pub result: ItemStack,
    /// Ticks the recipe takes (vanilla `AbstractCookingRecipe.getCookingTime`).
    pub cook_time: i32,
    /// Experience awarded when the result is taken.
    pub experience: f32,
}

/// Furnace-family burn and cook timers (vanilla `AbstractFurnaceBlockEntity`'s
/// `litTimeRemaining`, `litTotalTime`, `cookingProgress` and `cookingTotalTime`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FurnaceState {
    pub cook: i32,
    pub cook_total: i32,
    pub burn: i32,
    pub burn_total: i32,
    /// Experience of the recipes completed but not yet taken from the output slot
    /// (vanilla `AbstractFurnaceBlockEntity.recipesUsed`).
    pub pending_experience: f32,
}

impl FurnaceState {
    /// Vanilla `isLit`.
    pub fn lit(&self) -> bool {
        self.burn > 0
    }
}

/// The menus this crate implements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKind {
    /// The player's own inventory, container 0 (vanilla `InventoryMenu`).
    Inventory,
    /// A crafting table (vanilla `CraftingMenu`).
    Crafting,
    /// A chest-like grid of `rows` rows of nine slots (vanilla `ChestMenu`).
    Generic { rows: u8 },
    /// A furnace, blast furnace or smoker (vanilla `AbstractFurnaceMenu`).
    Furnace(FurnaceKind),
}

impl MenuKind {
    /// The `minecraft:menu` registry name sent in `open_screen`; the inventory has none.
    pub fn menu_type(self) -> Option<String> {
        match self {
            Self::Inventory => None,
            Self::Crafting => Some("minecraft:crafting".into()),
            Self::Generic { rows } => Some(format!("minecraft:generic_9x{rows}")),
            Self::Furnace(kind) => Some(kind.menu_type().into()),
        }
    }
}

/// Matches crafting grids against the recipe book.
pub trait Crafter {
    /// The result of a row-major grid with `width` columns, or an empty stack.
    fn craft(&self, grid: &[ItemStack], width: usize) -> ItemStack;

    /// The furnace-family recipe for `input`, if one exists (vanilla `RecipeManager.getRecipeFor`).
    fn cook(&self, _kind: FurnaceKind, _input: &ItemStack) -> Option<CookingOutput> {
        None
    }

    /// Burn ticks of a fuel item (vanilla `FuelValues.burnDuration`), or 0 when it is no fuel.
    fn burn_time(&self, _item: &ItemStack) -> i32 {
        0
    }
}

/// A [`Crafter`] that never crafts anything.
pub struct NoCrafting;

impl Crafter for NoCrafting {
    fn craft(&self, _: &[ItemStack], _: usize) -> ItemStack {
        ItemStack::empty()
    }
}

/// What an ingredient leaves in the grid after crafting (vanilla `Item.Properties.craftRemainder`;
/// in 26.3 only these five items have one).
pub fn craft_remainder(item: u16) -> Option<ItemStack> {
    let remainder = match ferrite_item::item_name(item)? {
        "minecraft:water_bucket" | "minecraft:lava_bucket" | "minecraft:milk_bucket" => {
            "minecraft:bucket"
        }
        "minecraft:dragon_breath" | "minecraft:honey_bottle" => "minecraft:glass_bottle",
        _ => return None,
    };
    ItemStack::from_name(remainder)
}

/// The player a click acts for, plus everything the click drops.
pub struct Clicker<'a> {
    pub inventory: &'a mut PlayerInventory,
    /// Vanilla `Player.hasInfiniteMaterials` (creative): enables `CLONE`, the creative drag and
    /// taking cursed armor off.
    pub infinite_materials: bool,
    pub crafter: &'a dyn Crafter,
    /// Stacks the player drops (vanilla `player.drop`), in order.
    pub drops: Vec<ItemStack>,
    /// Experience points the click awards (furnace output, later grindstone and anvil costs).
    pub xp: f32,
}

impl<'a> Clicker<'a> {
    pub fn new(
        inventory: &'a mut PlayerInventory,
        infinite_materials: bool,
        crafter: &'a dyn Crafter,
    ) -> Self {
        Self {
            inventory,
            infinite_materials,
            crafter,
            drops: Vec::new(),
            xp: 0.0,
        }
    }

    fn drop(&mut self, stack: ItemStack) {
        if !stack.is_empty() {
            self.drops.push(stack);
        }
    }
}

/// A packet the server sends to keep the client's copy of a menu in sync.
#[derive(Clone, Debug, PartialEq)]
pub enum Sync {
    /// `container_set_content`.
    Content {
        container_id: i32,
        state_id: i32,
        items: Vec<ItemStack>,
        carried: ItemStack,
    },
    /// `container_set_slot`.
    Slot {
        container_id: i32,
        state_id: i32,
        slot: i16,
        item: ItemStack,
    },
    /// `set_cursor_item`.
    Carried(ItemStack),
    /// `container_set_data`: a menu property (vanilla `AbstractContainerMenu.setData`).
    Data {
        container_id: i32,
        id: i16,
        value: i16,
    },
}

/// Where a menu slot keeps its stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Store {
    Inventory(usize),
    Grid(usize),
    Result,
    Container(usize),
}

/// Slot behaviour: a plain `Slot`, an `ArmorSlot` for one equipment slot, or a `ResultSlot`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rule {
    Plain,
    Armor(&'static str),
    Result,
    /// A furnace input slot (vanilla `AbstractFurnaceMenu` slot 0).
    FurnaceInput,
    /// A furnace fuel slot (vanilla `AbstractFurnaceMenu` slot 1, `FuelSlot`).
    Fuel,
    /// A menu output slot: may not be placed into, and taking from it awards experience or
    /// consumes the menu's ingredients (vanilla `ResultSlot` for stations).
    Output,
}

#[derive(Clone, Copy, Debug)]
struct SlotDef {
    store: Store,
    rule: Rule,
}

/// What the server believes the client shows in a slot (vanilla `RemoteSlot`).
#[derive(Clone, Debug, PartialEq)]
enum Remote {
    /// A stack the server sent.
    Known(ItemStack),
    /// The client's own prediction from a click, as a hashed slot.
    Hashed(Option<HashedSlot>),
}

impl Remote {
    fn empty() -> Self {
        Self::Known(ItemStack::empty())
    }

    /// Vanilla `HashedStack.matches`. The component hashes are not recomputed, so a predicted
    /// stack matches when the item, count and the set of patched component types agree.
    fn matches(&self, stack: &ItemStack) -> bool {
        match self {
            Self::Known(known) => {
                (known.is_empty() && stack.is_empty())
                    || (known.count == stack.count && known.is_same_item_same_components(stack))
            }
            Self::Hashed(None) => stack.is_empty(),
            Self::Hashed(Some(hashed)) => {
                if stack.is_empty() || hashed.item != stack.item || hashed.count != stack.count {
                    return false;
                }
                let mut added: Vec<u16> = hashed.added.iter().map(|(id, _)| *id).collect();
                let mut removed = hashed.removed.clone();
                added.sort_unstable();
                removed.sort_unstable();
                added
                    .iter()
                    .copied()
                    .eq(stack.components.added().map(|(id, _)| id))
                    && removed.iter().copied().eq(stack.components.removed())
            }
        }
    }
}

/// An open container menu (vanilla `AbstractContainerMenu`).
#[derive(Clone, Debug)]
pub struct Menu {
    container_id: i32,
    kind: MenuKind,
    slots: Vec<SlotDef>,
    grid: Vec<ItemStack>,
    grid_width: usize,
    result: ItemStack,
    container: Vec<ItemStack>,
    /// Container data (vanilla `AbstractContainerMenu.dataSlots`), most recently synced to the
    /// client through `container_set_data`.
    data: Vec<i32>,
    remote_data: Vec<i32>,
    furnace: Option<FurnaceState>,
    carried: ItemStack,
    state_id: i32,
    quickcraft_status: i32,
    quickcraft_type: i32,
    quickcraft_slots: Vec<usize>,
    remote: Vec<Remote>,
    remote_carried: Remote,
}

/// Vanilla `addStandardInventorySlots`: the main inventory rows, then the hotbar.
fn standard_inventory_slots(slots: &mut Vec<SlotDef>) {
    for slot in (HOTBAR..ITEMS).chain(0..HOTBAR) {
        slots.push(SlotDef {
            store: Store::Inventory(slot),
            rule: Rule::Plain,
        });
    }
}

impl Menu {
    fn with_slots(container_id: i32, kind: MenuKind, slots: Vec<SlotDef>) -> Self {
        let grid_width = match kind {
            MenuKind::Inventory => 2,
            MenuKind::Crafting => 3,
            MenuKind::Generic { .. } | MenuKind::Furnace(_) => 0,
        };
        let container = match kind {
            MenuKind::Generic { rows } => vec![ItemStack::empty(); usize::from(rows) * 9],
            MenuKind::Furnace(_) => vec![ItemStack::empty(); 3],
            _ => Vec::new(),
        };
        let data_len = match kind {
            MenuKind::Furnace(_) => FURNACE_DATA_SLOTS,
            _ => 0,
        };
        let remote = vec![Remote::empty(); slots.len()];
        Self {
            container_id,
            kind,
            slots,
            grid: vec![ItemStack::empty(); grid_width * grid_width],
            grid_width,
            result: ItemStack::empty(),
            container,
            data: vec![0; data_len],
            remote_data: vec![i32::MIN; data_len],
            furnace: matches!(kind, MenuKind::Furnace(_)).then(FurnaceState::default),
            carried: ItemStack::empty(),
            state_id: 0,
            quickcraft_status: 0,
            quickcraft_type: 0,
            quickcraft_slots: Vec::new(),
            remote,
            remote_carried: Remote::empty(),
        }
    }

    /// The player's inventory menu, container 0 (vanilla `InventoryMenu`): 0 the crafting
    /// result, 1..5 the 2x2 grid, 5..9 armor (head to feet), 9..36 the main inventory, 36..45
    /// the hotbar and 45 the offhand.
    pub fn inventory() -> Self {
        let mut slots = vec![SlotDef {
            store: Store::Result,
            rule: Rule::Result,
        }];
        slots.extend((0..4).map(|i| SlotDef {
            store: Store::Grid(i),
            rule: Rule::Plain,
        }));
        for (slot, name) in [
            (HEAD, "head"),
            (CHEST, "chest"),
            (LEGS, "legs"),
            (FEET, "feet"),
        ] {
            slots.push(SlotDef {
                store: Store::Inventory(slot),
                rule: Rule::Armor(name),
            });
        }
        standard_inventory_slots(&mut slots);
        slots.push(SlotDef {
            store: Store::Inventory(OFFHAND),
            rule: Rule::Plain,
        });
        Self::with_slots(0, MenuKind::Inventory, slots)
    }

    /// A crafting table (vanilla `CraftingMenu`): 0 the result, 1..10 the 3x3 grid, 10..37 the
    /// main inventory and 37..46 the hotbar.
    pub fn crafting(container_id: i32) -> Self {
        let mut slots = vec![SlotDef {
            store: Store::Result,
            rule: Rule::Result,
        }];
        slots.extend((0..9).map(|i| SlotDef {
            store: Store::Grid(i),
            rule: Rule::Plain,
        }));
        standard_inventory_slots(&mut slots);
        Self::with_slots(container_id, MenuKind::Crafting, slots)
    }

    /// A chest-like menu (vanilla `ChestMenu`) of `rows` rows (1..=6) followed by the player's
    /// inventory. The menu owns the container's stacks; see [`Menu::container_mut`].
    pub fn generic(container_id: i32, rows: u8) -> Self {
        let rows = rows.clamp(1, 6);
        let mut slots: Vec<SlotDef> = (0..usize::from(rows) * 9)
            .map(|i| SlotDef {
                store: Store::Container(i),
                rule: Rule::Plain,
            })
            .collect();
        standard_inventory_slots(&mut slots);
        Self::with_slots(container_id, MenuKind::Generic { rows }, slots)
    }

    /// A furnace-family menu (vanilla `FurnaceMenu`, `BlastFurnaceMenu`, `SmokerMenu`):
    /// 0 input, 1 fuel, 2 result, 3..39 the player's inventory.
    pub fn furnace(container_id: i32, kind: FurnaceKind) -> Self {
        let mut slots = vec![
            SlotDef {
                store: Store::Container(0),
                rule: Rule::FurnaceInput,
            },
            SlotDef {
                store: Store::Container(1),
                rule: Rule::Fuel,
            },
            SlotDef {
                store: Store::Result,
                rule: Rule::Output,
            },
        ];
        standard_inventory_slots(&mut slots);
        Self::with_slots(container_id, MenuKind::Furnace(kind), slots)
    }

    pub fn container_id(&self) -> i32 {
        self.container_id
    }

    pub fn kind(&self) -> MenuKind {
        self.kind
    }

    /// Number of slots.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn state_id(&self) -> i32 {
        self.state_id
    }

    /// Vanilla `incrementStateId`.
    pub fn increment_state_id(&mut self) -> i32 {
        self.state_id = (self.state_id + 1) & 32767;
        self.state_id
    }

    /// The stack on the cursor.
    pub fn carried(&self) -> &ItemStack {
        &self.carried
    }

    pub fn set_carried(&mut self, stack: ItemStack) {
        self.carried = normalize(stack);
    }

    /// The crafting grid, row-major.
    pub fn grid(&self) -> &[ItemStack] {
        &self.grid
    }

    /// The crafting result slot.
    pub fn result(&self) -> &ItemStack {
        &self.result
    }

    /// The generic container's own stacks.
    pub fn container(&self) -> &[ItemStack] {
        &self.container
    }

    pub fn container_mut(&mut self) -> &mut [ItemStack] {
        &mut self.container
    }

    /// The menu's container data (vanilla `AbstractContainerMenu.dataSlots`).
    pub fn data(&self) -> &[i32] {
        &self.data
    }

    /// One container data value, or 0 when the menu has no such slot.
    pub fn data_value(&self, id: usize) -> i32 {
        self.data.get(id).copied().unwrap_or(0)
    }

    /// The furnace timers, for a furnace-family menu.
    pub fn furnace_state(&self) -> Option<&FurnaceState> {
        self.furnace.as_ref()
    }

    /// Restores the furnace timers when the menu is opened from a block entity.
    pub fn set_furnace_state(&mut self, state: FurnaceState) {
        self.furnace = Some(state);
        self.sync_data();
    }

    /// Copies the timers into the container data slots (vanilla
    /// `AbstractFurnaceMenu.broadcastChanges` calling `setData`).
    fn sync_data(&mut self) {
        let Some(furnace) = &self.furnace else {
            return;
        };
        if self.data.len() < FURNACE_DATA_SLOTS {
            self.data.resize(FURNACE_DATA_SLOTS, 0);
            self.remote_data.resize(FURNACE_DATA_SLOTS, i32::MIN);
        }
        self.data[FURNACE_LIT] = i32::from(furnace.lit());
        self.data[FURNACE_COOK_PROGRESS] = furnace.cook;
        self.data[FURNACE_COOK_TOTAL] = furnace.cook_total;
        self.data[FURNACE_BURN_LEFT] = furnace.burn;
        self.data[FURNACE_BURN_TOTAL] = furnace.burn_total;
    }

    /// Advances the furnace timers by one tick (vanilla `AbstractFurnaceBlockEntity.serverTick`),
    /// then refreshes the container data. Other menus do nothing.
    pub fn tick(&mut self, crafter: &dyn Crafter) {
        if let MenuKind::Furnace(kind) = self.kind {
            self.tick_furnace(kind, crafter);
        }
        self.sync_data();
    }

    fn tick_furnace(&mut self, kind: FurnaceKind, crafter: &dyn Crafter) {
        let input = self.container[0].clone();
        let fuel = self.container[1].clone();
        let recipe = if input.is_empty() {
            None
        } else {
            crafter.cook(kind, &input)
        };
        let can_cook = recipe.as_ref().is_some_and(|recipe| {
            self.result.is_empty()
                || (self.result.is_same_item_same_components(&recipe.result)
                    && self.result.count + recipe.result.count <= self.result.max_stack_size())
        });
        let mut state = self.furnace.clone().unwrap_or_default();
        if state.burn > 0 {
            state.burn -= 1;
        }
        if can_cook {
            if state.burn == 0 {
                let burn = crafter.burn_time(&fuel);
                if burn > 0 {
                    state.burn = burn;
                    state.burn_total = burn;
                    let mut left = fuel;
                    left.count -= 1;
                    self.container[1] = normalize(left);
                }
            }
            if state.burn > 0 {
                let recipe = recipe.expect("can_cook implies a recipe");
                if state.cook == 0 {
                    state.cook_total = recipe.cook_time.max(1);
                }
                state.cook += 1;
                if state.cook >= state.cook_total {
                    if self.result.is_empty() {
                        self.result = recipe.result.clone();
                    } else {
                        self.result.count += recipe.result.count;
                    }
                    let mut left = self.container[0].clone();
                    left.count -= 1;
                    self.container[0] = normalize(left);
                    state.pending_experience += recipe.experience;
                    state.cook = 0;
                    state.cook_total = 0;
                }
            } else {
                state.cook = 0;
                state.cook_total = 0;
            }
        } else {
            state.cook = 0;
            state.cook_total = 0;
        }
        self.furnace = Some(state);
    }

    /// The stack in a menu slot.
    pub fn slot<'s>(&'s self, inventory: &'s PlayerInventory, slot: usize) -> &'s ItemStack {
        match self.slots[slot].store {
            Store::Inventory(i) => inventory.get(i),
            Store::Grid(i) => &self.grid[i],
            Store::Result => &self.result,
            Store::Container(i) => &self.container[i],
        }
    }

    /// Every slot's stack, in menu order.
    pub fn items(&self, inventory: &PlayerInventory) -> Vec<ItemStack> {
        (0..self.slots.len())
            .map(|slot| self.slot(inventory, slot).clone())
            .collect()
    }

    /// The menu slot showing an inventory slot, if this menu shows it.
    pub fn menu_slot_of(&self, inventory_slot: usize) -> Option<usize> {
        self.slots
            .iter()
            .position(|def| def.store == Store::Inventory(inventory_slot))
    }

    /// Writes a menu slot directly (vanilla `Slot.setByPlayer`, as the creative inventory does),
    /// recomputing the crafting result when the grid changes. Out-of-range slots are ignored.
    pub fn set_slot(
        &mut self,
        inventory: &mut PlayerInventory,
        crafter: &dyn Crafter,
        slot: usize,
        stack: ItemStack,
    ) {
        if slot < self.slots.len() {
            let mut p = Clicker::new(inventory, true, crafter);
            self.put(&mut p, slot, stack);
        }
    }

    /// Vanilla `isValidSlotIndex`.
    pub fn is_valid_slot_index(&self, slot: i16) -> bool {
        slot == -1 || slot == SLOT_OUTSIDE || (slot >= 0 && (slot as usize) < self.slots.len())
    }

    fn stack(&self, p: &Clicker, slot: usize) -> ItemStack {
        self.slot(p.inventory, slot).clone()
    }

    /// Vanilla `Slot.setByPlayer`; a changed crafting grid recomputes the result
    /// (`CraftingMenu.slotChangedCraftingGrid`).
    fn put(&mut self, p: &mut Clicker, slot: usize, stack: ItemStack) {
        let stack = normalize(stack);
        match self.slots[slot].store {
            Store::Inventory(i) => p.inventory.set(i, stack),
            Store::Grid(i) => {
                self.grid[i] = stack;
                self.update_result(p);
            }
            Store::Result => self.result = stack,
            Store::Container(i) => self.container[i] = stack,
        }
    }

    fn update_result(&mut self, p: &Clicker) {
        self.result = normalize(p.crafter.craft(&self.grid, self.grid_width));
    }

    /// Recomputes the crafting result from the grid.
    pub fn refresh_result(&mut self, crafter: &dyn Crafter) {
        if self.grid_width > 0 {
            self.result = normalize(crafter.craft(&self.grid, self.grid_width));
        }
    }

    /// `Slot.mayPlace`: nothing goes into a result slot, armor slots take matching equipment.
    fn may_place(&self, slot: usize, stack: &ItemStack) -> bool {
        match self.slots[slot].rule {
            Rule::Plain => true,
            Rule::Armor(name) => equipment_slot(stack) == name,
            Rule::Result | Rule::Output => false,
            // Vanilla furnace input and fuel slots accept any stack; the recipe and fuel checks
            // happen when the block ticks, as in `AbstractFurnaceBlockEntity`.
            Rule::FurnaceInput | Rule::Fuel => true,
        }
    }

    /// `Slot.mayPickup`: `ArmorSlot` keeps cursed armor on outside creative.
    fn may_pickup(&self, p: &Clicker, slot: usize) -> bool {
        match self.slots[slot].rule {
            Rule::Armor(_) => {
                let item = self.slot(p.inventory, slot);
                item.is_empty() || p.infinite_materials || !prevents_armor_change(item)
            }
            _ => true,
        }
    }

    /// `Slot.allowModification`.
    fn allow_modification(&self, p: &Clicker, slot: usize) -> bool {
        self.may_pickup(p, slot) && self.may_place(slot, self.slot(p.inventory, slot))
    }

    /// `Slot.getMaxStackSize(stack)`: armor slots hold one item.
    fn slot_max(&self, slot: usize, stack: &ItemStack) -> i32 {
        match self.slots[slot].rule {
            Rule::Armor(_) => 1.min(max_in_slot(stack)),
            _ => max_in_slot(stack),
        }
    }

    /// `CraftingMenu.canDragTo` / `canTakeItemForPickAll`: never the result slot.
    fn is_result(&self, slot: usize) -> bool {
        self.slots[slot].rule == Rule::Result
    }

    /// `Slot.remove`; a `ResultContainer` always gives up its whole stack.
    fn remove(&mut self, p: &mut Clicker, slot: usize, amount: i32) -> ItemStack {
        if self.slots[slot].rule == Rule::Result {
            return std::mem::take(&mut self.result);
        }
        let mut stack = self.stack(p, slot);
        let taken = stack.split(amount).unwrap_or_default();
        if !taken.is_empty() {
            self.put(p, slot, stack);
        }
        taken
    }

    /// `Slot.tryRemove`.
    fn try_remove(
        &mut self,
        p: &mut Clicker,
        slot: usize,
        amount: i32,
        decrement: i32,
    ) -> Option<ItemStack> {
        if !self.may_pickup(p, slot) {
            return None;
        }
        if !self.allow_modification(p, slot) && decrement < self.slot(p.inventory, slot).count {
            return None;
        }
        let taken = self.remove(p, slot, amount.min(decrement));
        (!taken.is_empty()).then_some(taken)
    }

    /// `Slot.safeTake`.
    fn safe_take(
        &mut self,
        p: &mut Clicker,
        slot: usize,
        amount: i32,
        decrement: i32,
    ) -> ItemStack {
        match self.try_remove(p, slot, amount, decrement) {
            Some(taken) => {
                self.on_take(p, slot);
                taken
            }
            None => ItemStack::empty(),
        }
    }

    /// `Slot.safeInsert`: moves up to `count` items of `stack` into the slot and returns the
    /// rest.
    fn safe_insert(
        &mut self,
        p: &mut Clicker,
        slot: usize,
        mut stack: ItemStack,
        count: i32,
    ) -> ItemStack {
        if stack.is_empty() || !self.may_place(slot, &stack) {
            return stack;
        }
        let existing = self.stack(p, slot);
        let moved = count
            .min(stack.count)
            .min(self.slot_max(slot, &stack) - existing.count);
        if moved <= 0 {
            return stack;
        }
        if existing.is_empty() {
            let part = stack.split(moved).unwrap_or_default();
            self.put(p, slot, part);
        } else if existing.is_same_item_same_components(&stack) {
            stack.count -= moved;
            let grown = existing.copy_with_count(existing.count + moved);
            self.put(p, slot, grown);
        }
        normalize(stack)
    }

    /// `Slot.onTake`; for a `ResultSlot` this consumes one of each ingredient and leaves the
    /// crafting remainders (vanilla `ResultSlot.onTake`).
    fn on_take(&mut self, p: &mut Clicker, slot: usize) {
        if self.slots[slot].rule == Rule::Output {
            if let Some(furnace) = &mut self.furnace {
                p.xp += furnace.pending_experience;
                furnace.pending_experience = 0.0;
            }
            return;
        }
        if self.slots[slot].rule != Rule::Result {
            return;
        }
        for cell in 0..self.grid.len() {
            let ingredient = self.grid[cell].clone();
            if ingredient.is_empty() {
                continue;
            }
            let mut left = ingredient.copy_with_count(ingredient.count - 1);
            if let Some(mut remainder) = craft_remainder(ingredient.item) {
                if left.is_empty() {
                    left = remainder;
                } else if left.is_same_item_same_components(&remainder) {
                    left.count += remainder.count;
                } else {
                    // Vanilla drops a remainder the inventory cannot take.
                    p.inventory.add(&mut remainder, p.infinite_materials);
                    p.drop(remainder);
                }
            }
            self.grid[cell] = normalize(left);
        }
        self.update_result(p);
    }

    /// Vanilla `canItemQuickReplace(slot, stack, true)`.
    fn can_quick_replace(&self, p: &Clicker, slot: usize, stack: &ItemStack) -> bool {
        let existing = self.slot(p.inventory, slot);
        if existing.is_empty() {
            return true;
        }
        existing.is_same_item_same_components(stack) && existing.count <= stack.max_stack_size()
    }

    fn reset_quick_craft(&mut self) {
        self.quickcraft_status = 0;
        self.quickcraft_slots.clear();
    }

    /// Vanilla `AbstractContainerMenu.doClick`: applies one `container_click`.
    ///
    /// `slot` is a menu slot, `-1` or [`SLOT_OUTSIDE`]; other values are ignored.
    pub fn clicked(&mut self, p: &mut Clicker, slot: i16, button: i8, input: ClickInput) {
        let button = i32::from(button);
        let index = (slot >= 0 && (slot as usize) < self.slots.len()).then_some(slot as usize);
        if input == ClickInput::QuickCraft {
            self.quick_craft(p, index, button);
        } else if self.quickcraft_status != 0 {
            self.reset_quick_craft();
        } else if matches!(input, ClickInput::Pickup | ClickInput::QuickMove)
            && (button == 0 || button == 1)
        {
            let primary = button == 0;
            if slot == SLOT_OUTSIDE {
                if !self.carried.is_empty() {
                    let dropped = if primary {
                        std::mem::take(&mut self.carried)
                    } else {
                        self.carried.split(1).unwrap_or_default()
                    };
                    self.carried = normalize(std::mem::take(&mut self.carried));
                    p.drop(dropped);
                }
            } else if input == ClickInput::QuickMove {
                let Some(i) = index else { return };
                if !self.may_pickup(p, i) {
                    return;
                }
                let mut moved = self.quick_move_stack(p, i);
                while !moved.is_empty() && self.slot(p.inventory, i).is_same_item(&moved) {
                    moved = self.quick_move_stack(p, i);
                }
            } else {
                let Some(i) = index else { return };
                self.pickup(p, i, primary);
            }
        } else if input == ClickInput::Swap && ((0..9).contains(&button) || button == 40) {
            let Some(i) = index else { return };
            self.swap(p, i, button as usize);
        } else if input == ClickInput::Clone
            && p.infinite_materials
            && self.carried.is_empty()
            && let Some(i) = index
        {
            let item = self.stack(p, i);
            if !item.is_empty() {
                let max = item.max_stack_size();
                self.carried = item.copy_with_count(max);
            }
        } else if input == ClickInput::Throw
            && self.carried.is_empty()
            && let Some(i) = index
        {
            let amount = if button == 0 {
                1
            } else {
                self.slot(p.inventory, i).count
            };
            let mut taken = self.safe_take(p, i, amount, i32::MAX);
            p.drop(taken.clone());
            if button == 1 {
                while !taken.is_empty() && self.slot(p.inventory, i).is_same_item(&taken) {
                    taken = self.safe_take(p, i, amount, i32::MAX);
                    p.drop(taken.clone());
                }
            }
        } else if input == ClickInput::PickupAll
            && let Some(i) = index
        {
            self.pickup_all(p, i, button);
        }
    }

    /// The `QUICK_CRAFT` (drag) branch of `doClick`.
    fn quick_craft(&mut self, p: &mut Clicker, index: Option<usize>, button: i32) {
        let previous = self.quickcraft_status;
        self.quickcraft_status = button & 3;
        let out_of_order =
            (previous != 1 || self.quickcraft_status != 2) && previous != self.quickcraft_status;
        if out_of_order || self.carried.is_empty() {
            self.reset_quick_craft();
        } else if self.quickcraft_status == 0 {
            self.quickcraft_type = (button >> 2) & 3;
            if self.quickcraft_type == 0
                || self.quickcraft_type == 1
                || (self.quickcraft_type == 2 && p.infinite_materials)
            {
                self.quickcraft_status = 1;
                self.quickcraft_slots.clear();
            } else {
                self.reset_quick_craft();
            }
        } else if self.quickcraft_status == 1 {
            let Some(i) = index else { return };
            let carried = &self.carried;
            if self.can_quick_replace(p, i, carried)
                && self.may_place(i, carried)
                && (self.quickcraft_type == 2
                    || carried.count as usize > self.quickcraft_slots.len())
                && !self.is_result(i)
                && !self.quickcraft_slots.contains(&i)
            {
                self.quickcraft_slots.push(i);
            }
        } else if self.quickcraft_status == 2 {
            if !self.quickcraft_slots.is_empty() {
                if self.quickcraft_slots.len() == 1 {
                    let only = self.quickcraft_slots[0];
                    let kind = self.quickcraft_type;
                    self.reset_quick_craft();
                    self.clicked(p, only as i16, kind as i8, ClickInput::Pickup);
                    return;
                }
                let template = self.carried.clone();
                let mut remaining = template.count;
                let spread = self.quickcraft_slots.len();
                for target in self.quickcraft_slots.clone() {
                    if self.can_quick_replace(p, target, &template)
                        && self.may_place(target, &template)
                        && (self.quickcraft_type == 2 || template.count as usize >= spread)
                        && !self.is_result(target)
                    {
                        let existing = self.slot(p.inventory, target);
                        let had = if existing.is_empty() {
                            0
                        } else {
                            existing.count
                        };
                        let max = template
                            .max_stack_size()
                            .min(self.slot_max(target, &template));
                        let each = match self.quickcraft_type {
                            0 => template.count / spread as i32,
                            1 => 1,
                            _ => template.max_stack_size(),
                        };
                        let placed = (each + had).min(max);
                        remaining -= placed - had;
                        self.put(p, target, template.copy_with_count(placed));
                    }
                }
                self.carried = normalize(template.copy_with_count(remaining));
            }
            self.reset_quick_craft();
        } else {
            self.reset_quick_craft();
        }
    }

    /// The `PICKUP` branch of `doClick` for a slot inside the menu.
    fn pickup(&mut self, p: &mut Clicker, i: usize, primary: bool) {
        let item = self.stack(p, i);
        let carried = self.carried.clone();
        if item.is_empty() {
            if !carried.is_empty() {
                let count = if primary { carried.count } else { 1 };
                self.carried = self.safe_insert(p, i, carried, count);
            }
        } else if self.may_pickup(p, i) {
            if carried.is_empty() {
                let count = if primary {
                    item.count
                } else {
                    (item.count + 1) / 2
                };
                if let Some(taken) = self.try_remove(p, i, count, i32::MAX) {
                    self.carried = taken;
                    self.on_take(p, i);
                }
            } else if self.may_place(i, &carried) {
                if item.is_same_item_same_components(&carried) {
                    let count = if primary { carried.count } else { 1 };
                    self.carried = self.safe_insert(p, i, carried, count);
                } else if carried.count <= self.slot_max(i, &carried) {
                    self.carried = item;
                    self.put(p, i, carried);
                }
            } else if item.is_same_item_same_components(&carried)
                && let Some(taken) =
                    self.try_remove(p, i, item.count, carried.max_stack_size() - carried.count)
            {
                self.carried.count += taken.count;
                self.on_take(p, i);
            }
        }
    }

    /// The `SWAP` branch of `doClick`: `button` is a hotbar slot or 40 (the offhand).
    fn swap(&mut self, p: &mut Clicker, i: usize, button: usize) {
        let held = p.inventory.get(button).clone();
        let item = self.stack(p, i);
        if held.is_empty() && item.is_empty() {
            return;
        }
        if held.is_empty() {
            if self.may_pickup(p, i) {
                p.inventory.set(button, item);
                self.put(p, i, ItemStack::empty());
                self.on_take(p, i);
            }
        } else if item.is_empty() {
            if self.may_place(i, &held) {
                let max = self.slot_max(i, &held);
                let mut held = held;
                if held.count > max {
                    let part = held.split(max).unwrap_or_default();
                    p.inventory.set(button, held);
                    self.put(p, i, part);
                } else {
                    p.inventory.set(button, ItemStack::empty());
                    self.put(p, i, held);
                }
            }
        } else if self.may_pickup(p, i) && self.may_place(i, &held) {
            let max = self.slot_max(i, &held);
            let mut held = held;
            if held.count > max {
                let part = held.split(max).unwrap_or_default();
                p.inventory.set(button, held);
                self.put(p, i, part);
                self.on_take(p, i);
                let mut item = item;
                p.inventory.add(&mut item, p.infinite_materials);
                p.drop(item);
            } else {
                p.inventory.set(button, item);
                self.put(p, i, held);
                self.on_take(p, i);
            }
        }
    }

    /// The `PICKUP_ALL` (double click) branch of `doClick`.
    fn pickup_all(&mut self, p: &mut Clicker, i: usize, button: i32) {
        let mut carried = self.carried.clone();
        if carried.is_empty() || (!self.slot(p.inventory, i).is_empty() && self.may_pickup(p, i)) {
            return;
        }
        let len = self.slots.len() as isize;
        let (start, step) = if button == 0 { (0, 1) } else { (len - 1, -1) };
        for pass in 0..2 {
            let mut q = start;
            while q >= 0 && q < len && carried.count < carried.max_stack_size() {
                let s = q as usize;
                let item = self.stack(p, s);
                if !item.is_empty()
                    && self.can_quick_replace(p, s, &carried)
                    && self.may_pickup(p, s)
                    && !self.is_result(s)
                    && (pass != 0 || item.count != item.max_stack_size())
                {
                    let room = carried.max_stack_size() - carried.count;
                    let taken = self.safe_take(p, s, item.count, room);
                    carried.count += taken.count;
                }
                q += step;
            }
        }
        self.carried = carried;
    }

    /// Vanilla `moveItemStackTo`: merges `stack` into matching stacks of `start..end`, then
    /// into the first empty slot that accepts it. Returns whether anything moved.
    fn move_item_stack_to(
        &mut self,
        p: &mut Clicker,
        stack: &mut ItemStack,
        start: usize,
        end: usize,
        reverse: bool,
    ) -> bool {
        let order: Vec<usize> = if reverse {
            (start..end).rev().collect()
        } else {
            (start..end).collect()
        };
        let mut moved = false;
        if is_stackable(stack) {
            for &k in &order {
                if stack.is_empty() {
                    break;
                }
                let target = self.stack(p, k);
                if target.is_empty() || !stack.is_same_item_same_components(&target) {
                    continue;
                }
                let total = target.count + stack.count;
                let max = self.slot_max(k, &target);
                if total <= max {
                    stack.count = 0;
                    self.put(p, k, target.copy_with_count(total));
                    moved = true;
                } else if target.count < max {
                    stack.count -= max - target.count;
                    self.put(p, k, target.copy_with_count(max));
                    moved = true;
                }
            }
        }
        if !stack.is_empty() {
            for &k in &order {
                if self.slot(p.inventory, k).is_empty() && self.may_place(k, stack) {
                    let max = self.slot_max(k, stack);
                    let part = stack.split(stack.count.min(max)).unwrap_or_default();
                    self.put(p, k, part);
                    moved = true;
                    break;
                }
            }
        }
        moved
    }

    /// Vanilla `quickMoveStack` (shift-click) of `InventoryMenu`, `CraftingMenu` and
    /// `ChestMenu`. Returns the stack as it was before the move, or empty when nothing moved.
    fn quick_move_stack(&mut self, p: &mut Clicker, i: usize) -> ItemStack {
        let original = self.stack(p, i);
        if original.is_empty() {
            return ItemStack::empty();
        }
        let mut stack = original.clone();
        let moved = match self.kind {
            MenuKind::Inventory => {
                let worn = equipment_slot(&original);
                let armor = match worn {
                    "head" => Some(5),
                    "chest" => Some(6),
                    "legs" => Some(7),
                    "feet" => Some(8),
                    _ => None,
                };
                if i == 0 {
                    self.move_item_stack_to(p, &mut stack, 9, 45, true)
                } else if (1..9).contains(&i) {
                    self.move_item_stack_to(p, &mut stack, 9, 45, false)
                } else if let Some(j) = armor
                    && self.slot(p.inventory, j).is_empty()
                {
                    self.move_item_stack_to(p, &mut stack, j, j + 1, false)
                } else if worn == "offhand" && self.slot(p.inventory, 45).is_empty() {
                    self.move_item_stack_to(p, &mut stack, 45, 46, false)
                } else if (9..36).contains(&i) {
                    self.move_item_stack_to(p, &mut stack, 36, 45, false)
                } else if (36..45).contains(&i) {
                    self.move_item_stack_to(p, &mut stack, 9, 36, false)
                } else {
                    self.move_item_stack_to(p, &mut stack, 9, 45, false)
                }
            }
            MenuKind::Crafting => {
                if i == 0 {
                    self.move_item_stack_to(p, &mut stack, 10, 46, true)
                } else if (10..46).contains(&i) {
                    self.move_item_stack_to(p, &mut stack, 1, 10, false)
                        || if i < 37 {
                            self.move_item_stack_to(p, &mut stack, 37, 46, false)
                        } else {
                            self.move_item_stack_to(p, &mut stack, 10, 37, false)
                        }
                } else {
                    self.move_item_stack_to(p, &mut stack, 10, 46, false)
                }
            }
            MenuKind::Generic { rows } => {
                let size = usize::from(rows) * 9;
                let len = self.slots.len();
                if i < size {
                    self.move_item_stack_to(p, &mut stack, size, len, true)
                } else {
                    self.move_item_stack_to(p, &mut stack, 0, size, false)
                }
            }
            MenuKind::Furnace(kind) => {
                if i == 2 {
                    self.move_item_stack_to(p, &mut stack, 3, 39, true)
                } else if i >= 3 {
                    // Vanilla `AbstractFurnaceMenu.quickMoveStack`: input first when the item can
                    // be smelted, otherwise the fuel slot when it burns.
                    let cookable = p.crafter.cook(kind, &original).is_some();
                    let fuel = p.crafter.burn_time(&original) > 0;
                    (cookable && self.move_item_stack_to(p, &mut stack, 0, 1, false))
                        || (fuel && self.move_item_stack_to(p, &mut stack, 1, 2, false))
                } else {
                    self.move_item_stack_to(p, &mut stack, 3, 39, false)
                }
            }
        };
        if !moved {
            return ItemStack::empty();
        }
        let left = stack.count.max(0);
        self.put(p, i, normalize(stack.clone()));
        if left == original.count {
            return ItemStack::empty();
        }
        self.on_take(p, i);
        if self.is_result(i) {
            // Vanilla drops the part of a crafted stack that did not fit.
            p.drop(normalize(stack));
        }
        original
    }

    /// Vanilla `AbstractContainerMenu.removed` plus the menu's own `removed`: the cursor stack
    /// and the crafting grid go back into the inventory (what does not fit is dropped) and the
    /// result is cleared.
    pub fn close(&mut self, p: &mut Clicker) {
        let carried = std::mem::take(&mut self.carried);
        if let Some(rest) = p.inventory.place_back(carried) {
            p.drop(rest);
        }
        self.reset_quick_craft();
        if self.grid_width > 0 {
            self.result = ItemStack::empty();
            for cell in 0..self.grid.len() {
                let stack = std::mem::take(&mut self.grid[cell]);
                if let Some(rest) = p.inventory.place_back(stack) {
                    p.drop(rest);
                }
            }
        }
    }

    /// Vanilla `ServerGamePacketListenerImpl.handleContainerClick` after the container id and
    /// spectator checks: runs the click, records the client's predicted slots and carried
    /// stack, then sends what differs, or the whole menu when the client's state id was stale.
    #[allow(clippy::too_many_arguments)]
    pub fn handle_click(
        &mut self,
        p: &mut Clicker,
        state_id: i32,
        slot: i16,
        button: i8,
        input: ClickInput,
        changed: &[(i16, Option<HashedSlot>)],
        carried: Option<HashedSlot>,
    ) -> Vec<Sync> {
        if !self.is_valid_slot_index(slot) {
            return Vec::new();
        }
        let stale = state_id != self.state_id;
        self.clicked(p, slot, button, input);
        for (slot, predicted) in changed {
            if let Some(remote) = usize::try_from(*slot)
                .ok()
                .and_then(|slot| self.remote.get_mut(slot))
            {
                *remote = Remote::Hashed(predicted.clone());
            }
        }
        self.remote_carried = Remote::Hashed(carried);
        if stale {
            vec![self.broadcast_full_state(p.inventory)]
        } else {
            self.broadcast_changes(p.inventory)
        }
    }

    /// Vanilla `broadcastChanges`: a `container_set_slot` (with a fresh state id) for every slot
    /// the client shows differently, then `set_cursor_item` if the cursor differs.
    pub fn broadcast_changes(&mut self, inventory: &PlayerInventory) -> Vec<Sync> {
        let mut out = Vec::new();
        for slot in 0..self.slots.len() {
            let item = self.slot(inventory, slot).clone();
            if !self.remote[slot].matches(&item) {
                self.remote[slot] = Remote::Known(item.clone());
                out.push(Sync::Slot {
                    container_id: self.container_id,
                    state_id: self.increment_state_id(),
                    slot: slot as i16,
                    item,
                });
            }
        }
        if !self.remote_carried.matches(&self.carried) {
            self.remote_carried = Remote::Known(self.carried.clone());
            out.push(Sync::Carried(self.carried.clone()));
        }
        for (id, &value) in self.data.iter().enumerate() {
            if self.remote_data.get(id).copied() == Some(value) {
                continue;
            }
            if id >= self.remote_data.len() {
                self.remote_data.resize(id + 1, i32::MIN);
            }
            self.remote_data[id] = value;
            out.push(Sync::Data {
                container_id: self.container_id,
                id: id as i16,
                value: value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
            });
        }
        out
    }

    /// Vanilla `sendAllDataToRemote`: the whole menu in one `container_set_content`.
    pub fn broadcast_full_state(&mut self, inventory: &PlayerInventory) -> Sync {
        let items = self.items(inventory);
        self.remote = items.iter().cloned().map(Remote::Known).collect();
        self.remote_carried = Remote::Known(self.carried.clone());
        Sync::Content {
            container_id: self.container_id,
            state_id: self.increment_state_id(),
            items,
            carried: self.carried.clone(),
        }
    }

    /// Vanilla `setRemoteSlot`: records that the client already shows `stack` in a slot (it
    /// predicted the change itself), so no update is sent for it.
    pub fn set_remote_slot(&mut self, slot: usize, stack: &ItemStack) {
        if let Some(remote) = self.remote.get_mut(slot) {
            *remote = Remote::Known(stack.clone());
        }
    }

    /// Vanilla `AnvilMenu.setItemName`: records a renamed item. Only an anvil menu uses it.
    pub fn rename(&mut self, _name: String) {}

    /// Vanilla `AbstractContainerMenu.clickMenuButton`: an enchant choice, stonecutter recipe or
    /// loom pattern. Only the menus that have buttons react.
    pub fn button_click(&mut self, _button: i32) {}

    /// Vanilla `transferState`: adopts what the client was last sent for the inventory slots
    /// `other` shares with this menu, so returning to this menu only sends real changes.
    pub fn transfer_state(&mut self, other: &Menu) {
        for (slot, def) in self.slots.iter().enumerate() {
            if let Store::Inventory(i) = def.store
                && let Some(from) = other.menu_slot_of(i)
            {
                self.remote[slot] = other.remote[from].clone();
            }
        }
    }
}

fn normalize(stack: ItemStack) -> ItemStack {
    if stack.is_empty() {
        ItemStack::empty()
    } else {
        stack
    }
}
