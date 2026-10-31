package org.bukkit.craftbukkit;

/** Compatibility offline-player view; Ferrite resolves these through its own player registry. */
public class CraftOfflinePlayer {
    private final org.bukkit.OfflinePlayer player;

    public CraftOfflinePlayer(org.bukkit.OfflinePlayer player) { this.player = java.util.Objects.requireNonNull(player, "player"); }

    public org.bukkit.OfflinePlayer getOfflinePlayer() { return player; }
    public java.util.UUID getUniqueId() { return player.getUniqueId(); }
    public String getName() { return player.getName(); }
    public org.bukkit.entity.Player getPlayer() { return player.getPlayer(); }
    public boolean isOnline() { return player.isOnline(); }
    public boolean isOp() { return player.isOp(); }
    public void setOp(boolean value) { player.setOp(value); }
    @Override public String toString() { return player.toString(); }
    @Override public boolean equals(Object other) { return other instanceof CraftOfflinePlayer value && getUniqueId().equals(value.getUniqueId()); }
    @Override public int hashCode() { return getUniqueId().hashCode(); }
}
