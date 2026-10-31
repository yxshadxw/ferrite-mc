package org.bukkit.craftbukkit.inventory;

/** Compatibility player-inventory view. */
public class CraftInventoryPlayer extends CraftInventory {
    public CraftInventoryPlayer(org.bukkit.inventory.PlayerInventory inventory) { super(inventory); }

    private org.bukkit.inventory.PlayerInventory player() { return (org.bukkit.inventory.PlayerInventory) getInventory(); }

    public org.bukkit.inventory.ItemStack[] getArmorContents() { return player().getArmorContents(); }
    public void setArmorContents(org.bukkit.inventory.ItemStack[] items) { player().setArmorContents(items); }
    public org.bukkit.inventory.ItemStack getHelmet() { return player().getHelmet(); }
    public void setHelmet(org.bukkit.inventory.ItemStack item) { player().setHelmet(item); }
    public org.bukkit.inventory.ItemStack getChestplate() { return player().getChestplate(); }
    public void setChestplate(org.bukkit.inventory.ItemStack item) { player().setChestplate(item); }
    public org.bukkit.inventory.ItemStack getLeggings() { return player().getLeggings(); }
    public void setLeggings(org.bukkit.inventory.ItemStack item) { player().setLeggings(item); }
    public org.bukkit.inventory.ItemStack getBoots() { return player().getBoots(); }
    public void setBoots(org.bukkit.inventory.ItemStack item) { player().setBoots(item); }
    public org.bukkit.inventory.ItemStack getItemInMainHand() { return player().getItemInMainHand(); }
    public void setItemInMainHand(org.bukkit.inventory.ItemStack item) { player().setItemInMainHand(item); }
    public org.bukkit.inventory.ItemStack getItemInOffHand() { return player().getItemInOffHand(); }
    public void setItemInOffHand(org.bukkit.inventory.ItemStack item) { player().setItemInOffHand(item); }
    public int getHeldItemSlot() { return player().getHeldItemSlot(); }
}
