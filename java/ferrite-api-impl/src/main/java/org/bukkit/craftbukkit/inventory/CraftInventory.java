package org.bukkit.craftbukkit.inventory;

/** Compatibility inventory view over any Bukkit inventory. */
public class CraftInventory {
    private final org.bukkit.inventory.Inventory inventory;

    public CraftInventory(org.bukkit.inventory.Inventory inventory) { this.inventory = java.util.Objects.requireNonNull(inventory, "inventory"); }

    public org.bukkit.inventory.Inventory getInventory() { return inventory; }
    public int getSize() { return inventory.getSize(); }
    public int getMaxStackSize() { return inventory.getMaxStackSize(); }
    public void setMaxStackSize(int size) { inventory.setMaxStackSize(size); }
    public org.bukkit.inventory.ItemStack getItem(int slot) { return inventory.getItem(slot); }
    public void setItem(int slot, org.bukkit.inventory.ItemStack item) { inventory.setItem(slot, item); }
    public org.bukkit.inventory.ItemStack[] getContents() { return inventory.getContents(); }
    public void setContents(org.bukkit.inventory.ItemStack[] items) { inventory.setContents(items); }
    public int firstEmpty() { return inventory.firstEmpty(); }
    public boolean isEmpty() { return inventory.isEmpty(); }
    public void clear() { inventory.clear(); }
    public void clear(int slot) { inventory.clear(slot); }
    public org.bukkit.inventory.InventoryHolder getHolder() { return inventory.getHolder(); }
    public org.bukkit.event.inventory.InventoryType getType() { return inventory.getType(); }
    public java.util.List<org.bukkit.entity.HumanEntity> getViewers() { return inventory.getViewers(); }
    public java.util.ListIterator<org.bukkit.inventory.ItemStack> iterator() { return inventory.iterator(); }
    @Override public String toString() { return inventory.toString(); }
}
