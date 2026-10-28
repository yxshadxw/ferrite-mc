package dev.ferrite.server;

/** Lightweight item stack until the server supplies full item data. */
final class FerriteItemStack extends org.bukkit.inventory.ItemStack {
    private org.bukkit.Material type;
    private int amount;
    private org.bukkit.inventory.meta.ItemMeta meta;
    FerriteItemStack(org.bukkit.Material type, int amount) { this.type = type; this.amount = amount; }
    @Override public org.bukkit.Material getType() { return type; }
    @Override public void setType(org.bukkit.Material type) { this.type = java.util.Objects.requireNonNull(type); }
    @Override public int getAmount() { return amount; }
    @Override public void setAmount(int amount) { this.amount = amount; }
    @Override public org.bukkit.inventory.meta.ItemMeta getItemMeta() { return meta == null ? FerriteItemMeta.create() : meta.clone(); }
    @Override public boolean setItemMeta(org.bukkit.inventory.meta.ItemMeta meta) { this.meta = meta == null ? null : meta.clone(); return true; }
    @Override public boolean hasItemMeta() { return meta != null; }
    @Override public FerriteItemStack clone() { var result = new FerriteItemStack(type, amount); result.meta = meta == null ? null : meta.clone(); return result; }
}
