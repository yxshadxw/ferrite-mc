//! Adding picked-up stacks to a player inventory (vanilla `Inventory.add`).
//!
//! Slots are numbered like the player's inventory menu (window 0), which is how the server
//! stores and syncs them: 9..=35 main inventory, 36..=44 hotbar, 45 offhand.

use ferrite_protocol::packets::play::ItemStack;

/// Menu slots in vanilla `Inventory.items` order: the hotbar, then the main inventory.
const ITEM_SLOTS: [usize; 36] = {
    let mut slots = [0; 36];
    let mut i = 0;
    while i < 36 {
        slots[i] = if i < 9 { 36 + i } else { i };
        i += 1;
    }
    slots
};
const OFFHAND: usize = 45;

/// Maximum stack size of an item (64 for unknown ids).
pub fn max_stack_size(item_id: i32) -> i32 {
    u16::try_from(item_id)
        .ok()
        .and_then(ferrite_item::item_defaults)
        .map_or(64, |d| i32::from(d.max_stack_size))
}

fn has_space_for(slot: ItemStack, new: ItemStack) -> bool {
    let max = max_stack_size(slot.item_id);
    slot.count > 0 && slot.item_id == new.item_id && max > 1 && slot.count < max
}

/// Vanilla `getSlotWithRemainingSpace`: the selected hotbar slot, the offhand, then the items
/// in order.
fn slot_with_space(slots: &[ItemStack; 46], selected: usize, new: ItemStack) -> Option<usize> {
    let held = 36 + selected.min(8);
    [held, OFFHAND]
        .into_iter()
        .chain(ITEM_SLOTS)
        .find(|&slot| has_space_for(slots[slot], new))
}

fn free_slot(slots: &[ItemStack; 46]) -> Option<usize> {
    ITEM_SLOTS.into_iter().find(|&slot| slots[slot].count <= 0)
}

/// Adds as much of `stack` as fits, shrinking it, and returns whether anything was taken plus
/// the changed menu slots. Players with infinite materials (creative) take the whole stack even
/// when it does not fit.
pub fn add(
    slots: &mut [ItemStack; 46],
    selected: usize,
    stack: &mut ItemStack,
    infinite: bool,
) -> (bool, Vec<usize>) {
    let mut changed = Vec::new();
    if stack.count <= 0 {
        return (false, changed);
    }
    let before = stack.count;
    loop {
        let last = stack.count;
        let Some(slot) = slot_with_space(slots, selected, *stack).or_else(|| free_slot(slots))
        else {
            break;
        };
        if slots[slot].count <= 0 {
            slots[slot] = ItemStack {
                count: 0,
                item_id: stack.item_id,
            };
        }
        let room = max_stack_size(slots[slot].item_id) - slots[slot].count;
        let moved = stack.count.min(room);
        if moved > 0 {
            slots[slot].count += moved;
            stack.count -= moved;
            if !changed.contains(&slot) {
                changed.push(slot);
            }
        }
        if stack.count <= 0 || stack.count >= last {
            break;
        }
    }
    if stack.count <= 0 {
        *stack = ItemStack::default();
    }
    if stack.count == before && infinite {
        *stack = ItemStack::default();
        return (true, changed);
    }
    (stack.count < before, changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(count: i32, item_id: i32) -> ItemStack {
        ItemStack { count, item_id }
    }

    #[test]
    fn fills_matching_stacks_then_free_slots_in_vanilla_order() {
        let dirt = ferrite_data::registries::id("minecraft:item", "minecraft:dirt").unwrap();
        let mut slots = [ItemStack::default(); 46];
        // Empty inventory: the first hotbar slot.
        let mut s = stack(3, dirt);
        assert_eq!(add(&mut slots, 4, &mut s, false), (true, vec![36]));
        assert_eq!(slots[36], stack(3, dirt));
        assert_eq!(s, ItemStack::default());
        // A matching stack in the selected slot is filled first.
        slots[40] = stack(60, dirt);
        let mut s = stack(10, dirt);
        assert_eq!(add(&mut slots, 4, &mut s, false), (true, vec![40, 36]));
        assert_eq!((slots[40].count, slots[36].count), (64, 9));
        // A full inventory takes nothing, unless the player has infinite materials.
        let stone = ferrite_data::registries::id("minecraft:item", "minecraft:stone").unwrap();
        let mut full = [stack(64, stone); 46];
        let mut s = stack(1, dirt);
        assert_eq!(add(&mut full, 0, &mut s, false), (false, vec![]));
        assert_eq!(s.count, 1);
        assert_eq!(add(&mut full, 0, &mut s, true), (true, vec![]));
        assert_eq!(s, ItemStack::default());
    }

    #[test]
    fn unstackable_items_take_one_slot_each() {
        let sword = ferrite_data::registries::id("minecraft:item", "minecraft:iron_sword").unwrap();
        assert_eq!(max_stack_size(sword), 1);
        let mut slots = [ItemStack::default(); 46];
        slots[36] = stack(1, sword);
        let mut s = stack(1, sword);
        assert_eq!(add(&mut slots, 0, &mut s, false), (true, vec![37]));
    }
}
