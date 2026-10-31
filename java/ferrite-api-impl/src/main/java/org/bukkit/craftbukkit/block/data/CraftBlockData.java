package org.bukkit.craftbukkit.block.data;

/** Compatibility BlockData view over a Ferrite BlockData snapshot. */
public final class CraftBlockData {
    private final org.bukkit.block.data.BlockData data;

    private CraftBlockData(org.bukkit.block.data.BlockData data) { this.data = data; }

    public static org.bukkit.block.data.BlockData create(org.bukkit.block.data.BlockData data) {
        return new CraftBlockData(java.util.Objects.requireNonNull(data, "data")).data;
    }

    public static CraftBlockData fromData(org.bukkit.block.data.BlockData data) { return new CraftBlockData(data); }

    public org.bukkit.block.data.BlockData getData() { return data; }
    public org.bukkit.Material getMaterial() { return data.getMaterial(); }
    public String getAsString() { return data.getAsString(); }
    @Override public String toString() { return data.getAsString(); }
}
