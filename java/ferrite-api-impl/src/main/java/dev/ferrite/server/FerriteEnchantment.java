package dev.ferrite.server;
final class FerriteEnchantment extends org.bukkit.enchantments.Enchantment {
    private final org.bukkit.NamespacedKey key;
    FerriteEnchantment(org.bukkit.NamespacedKey key) { this.key = key; }
    public org.bukkit.NamespacedKey getKey() { return key; }
    public String getName() { return key.getKey().toUpperCase(java.util.Locale.ROOT); }
    public String translationKey() { return "enchantment." + key.getNamespace() + "." + key.getKey(); }
    public String getTranslationKey() { return translationKey(); }
    private UnsupportedOperationException unsupported() { return new UnsupportedOperationException("Ferrite: enchantment properties not implemented: " + key); }
    public int getMaxLevel() { throw unsupported(); }
    public int getStartLevel() { throw unsupported(); }
    public org.bukkit.enchantments.EnchantmentTarget getItemTarget() { throw unsupported(); }
    public boolean isTreasure() { throw unsupported(); }
    public boolean isCursed() { throw unsupported(); }
    public boolean conflictsWith(org.bukkit.enchantments.Enchantment other) { throw unsupported(); }
    public boolean canEnchantItem(org.bukkit.inventory.ItemStack item) { throw unsupported(); }
    public net.kyori.adventure.text.Component displayName(int level) { throw unsupported(); }
    public boolean isTradeable() { throw unsupported(); }
    public boolean isDiscoverable() { throw unsupported(); }
    public int getMinModifiedCost(int level) { throw unsupported(); }
    public int getMaxModifiedCost(int level) { throw unsupported(); }
    public int getAnvilCost() { throw unsupported(); }
    public io.papermc.paper.enchantments.EnchantmentRarity getRarity() { throw unsupported(); }
    public float getDamageIncrease(int level, org.bukkit.entity.EntityCategory category) { throw unsupported(); }
    public float getDamageIncrease(int level, org.bukkit.entity.EntityType type) { throw unsupported(); }
    public java.util.Set<org.bukkit.inventory.EquipmentSlotGroup> getActiveSlotGroups() { throw unsupported(); }
    public net.kyori.adventure.text.Component description() { throw unsupported(); }
    public io.papermc.paper.registry.set.RegistryKeySet<org.bukkit.inventory.ItemType> getSupportedItems() { throw unsupported(); }
    public io.papermc.paper.registry.set.RegistryKeySet<org.bukkit.inventory.ItemType> getPrimaryItems() { throw unsupported(); }
    public int getWeight() { throw unsupported(); }
    public io.papermc.paper.registry.set.RegistryKeySet<org.bukkit.enchantments.Enchantment> getExclusiveWith() { throw unsupported(); }
}
