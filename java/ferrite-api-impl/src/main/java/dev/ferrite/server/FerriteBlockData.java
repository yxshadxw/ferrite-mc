package dev.ferrite.server;

/**
 * Minimal {@link org.bukkit.block.data.BlockData} for plugin-created and snapshot blocks.
 *
 * <p>The version 26.3 block-state table is owned by the Rust server, so the Java bridge only
 * carries the material and the vanilla state string. Property accessors are intentionally
 * unimplemented rather than guessed: a plugin that asks for an axis or facing gets a clear
 * exception instead of a made-up value.
 */
final class FerriteBlockData implements java.lang.reflect.InvocationHandler {
    private final org.bukkit.Material material;
    private final String state;
    private final org.bukkit.block.data.BlockData proxy;

    private FerriteBlockData(org.bukkit.Material material, String state) {
        this.material = material;
        this.state = state;
        this.proxy = (org.bukkit.block.data.BlockData) java.lang.reflect.Proxy.newProxyInstance(
                getClass().getClassLoader(), new Class<?>[]{org.bukkit.block.data.BlockData.class}, this);
    }

    /** Parses a vanilla block-state id such as {@code minecraft:stone} or {@code minecraft:oak_stairs[facing=east]}. */
    static org.bukkit.block.data.BlockData create(String value) {
        java.util.Objects.requireNonNull(value, "data");
        int open = value.indexOf('[');
        String id = open < 0 ? value : value.substring(0, open);
        String properties = open < 0 ? null : value.substring(open);
        if (!id.contains(":")) id = "minecraft:" + id;
        var material = org.bukkit.Material.matchMaterial(id);
        if (material == null) throw new IllegalArgumentException("Unknown block type: " + value);
        return create(material, properties);
    }

    static org.bukkit.block.data.BlockData create(org.bukkit.Material material) { return create(material, null); }

    static org.bukkit.block.data.BlockData create(org.bukkit.Material material, String properties) {
        java.util.Objects.requireNonNull(material, "material");
        if (!material.isBlock()) throw new IllegalArgumentException(material.getKey() + " is not a block");
        return new FerriteBlockData(material, properties).proxy;
    }

    static String id(org.bukkit.block.data.BlockData data) {
        return data.getAsString();
    }

    @Override public Object invoke(Object self, java.lang.reflect.Method method, Object[] args) {
        return switch (method.getName()) {
            case "getMaterial", "getType" -> material;
            case "getAsString", "toString" -> material.getKey() + (state == null ? "" : state);
            case "matches" -> args[0] instanceof org.bukkit.block.data.BlockData other && (material.getKey() + (state == null ? "" : state)).equals(other.getAsString());
            case "isSupported" -> true;
            case "isPlaced" -> false;
            case "clone" -> create(material, state);
            case "merge" -> { if (!(args[0] instanceof org.bukkit.block.data.BlockData other) || other.getMaterial() != material) throw new IllegalArgumentException("Cannot merge " + args[0] + " into " + material); yield create(material, state); }
            case "equals" -> self == args[0];
            case "hashCode" -> java.util.Objects.hash(material, state);
            default -> throw new UnsupportedOperationException(
                    "Ferrite: BlockData#" + method.getName() + " is not implemented for " + material.getKey()
                            + "; block-state properties are owned by the Rust server");
        };
    }
}
