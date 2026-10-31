package org.bukkit.craftbukkit.entity;

/** Compatibility player type; NMS accessors are intentionally absent. */
public abstract class CraftPlayer extends CraftHumanEntity {
    /** The Bukkit view of this player. */
    public org.bukkit.entity.Player getBukkitEntity() { return (org.bukkit.entity.Player) this; }
}
