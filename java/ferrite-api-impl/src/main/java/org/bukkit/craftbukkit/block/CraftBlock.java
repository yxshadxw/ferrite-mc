package org.bukkit.craftbukkit.block;

/** Compatibility block view; block state mutation goes through the Bukkit Block API. */
public class CraftBlock {
    private final org.bukkit.block.Block block;

    public CraftBlock(org.bukkit.block.Block block) { this.block = java.util.Objects.requireNonNull(block, "block"); }

    public org.bukkit.block.Block getBlock() { return block; }
    public org.bukkit.World getWorld() { return block.getWorld(); }
    public int getX() { return block.getX(); }
    public int getY() { return block.getY(); }
    public int getZ() { return block.getZ(); }
    public org.bukkit.Location getLocation() { return block.getLocation(); }
    public org.bukkit.Chunk getChunk() { return block.getChunk(); }
    public org.bukkit.Material getType() { return block.getType(); }
    public void setType(org.bukkit.Material type) { block.setType(type); }
    public org.bukkit.block.data.BlockData getBlockData() { return block.getBlockData(); }
    public void setBlockData(org.bukkit.block.data.BlockData data) { block.setBlockData(data); }
    public org.bukkit.block.Block getRelative(org.bukkit.block.BlockFace face) { return block.getRelative(face); }
    public boolean isEmpty() { return block.isEmpty(); }
    public boolean isLiquid() { return block.isLiquid(); }
    @Override public String toString() { return block.toString(); }
    @Override public boolean equals(Object other) { return other instanceof CraftBlock value && block.equals(value.block); }
    @Override public int hashCode() { return block.hashCode(); }
}
