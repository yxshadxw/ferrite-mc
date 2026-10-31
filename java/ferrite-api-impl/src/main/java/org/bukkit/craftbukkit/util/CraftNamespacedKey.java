package org.bukkit.craftbukkit.util;

/** Compatibility key helper; Ferrite keys are plain Bukkit namespaced keys. */
public final class CraftNamespacedKey {
    private CraftNamespacedKey() {}

    public static org.bukkit.NamespacedKey fromString(String value) { return org.bukkit.NamespacedKey.fromString(value); }

    public static org.bukkit.NamespacedKey fromStringOrThrow(String value) {
        return java.util.Objects.requireNonNull(org.bukkit.NamespacedKey.fromString(value), "invalid key: " + value);
    }

    public static String toString(org.bukkit.NamespacedKey key) { return key.toString(); }
}
