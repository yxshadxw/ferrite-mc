package dev.ferrite.server;

/** Stand-ins for Bukkit event objects fired from the host; only explicit data is known. */
final class FerriteEventObjects {
    private FerriteEventObjects() {}
    /** Vanilla's direction order: down, up, north, south, west, east. */
    private static final org.bukkit.block.BlockFace[] FACES = {
        org.bukkit.block.BlockFace.DOWN,
        org.bukkit.block.BlockFace.UP,
        org.bukkit.block.BlockFace.NORTH,
        org.bukkit.block.BlockFace.SOUTH,
        org.bukkit.block.BlockFace.WEST,
        org.bukkit.block.BlockFace.EAST,
    };
    static org.bukkit.block.BlockFace face(int index) { return FACES[Math.floorMod(index, FACES.length)]; }
    /** A block that knows its position, world and type; the host sends no other block data. */
    static org.bukkit.block.Block block(int x, int y, int z, org.bukkit.Material type) {
        return (org.bukkit.block.Block) java.lang.reflect.Proxy.newProxyInstance(FerriteEventObjects.class.getClassLoader(), new Class<?>[]{org.bukkit.block.Block.class}, (self, method, args) -> switch (method.getName()) {
            case "getX" -> x;
            case "getY" -> y;
            case "getZ" -> z;
            case "getWorld" -> Bootstrap.defaultWorld();
            case "getLocation" -> new org.bukkit.Location(Bootstrap.defaultWorld(), x, y, z, 0.0f, 0.0f);
            case "getType" -> type;
            case "getBlockData", "getBlockKey" -> throw new UnsupportedOperationException("Ferrite: event blocks have no block data snapshot");
            case "hashCode" -> java.util.Objects.hash(x, y, z);
            case "equals" -> self == args[0];
            case "toString" -> "Block[" + x + "," + y + "," + z + "]";
            default -> throw new UnsupportedOperationException("Ferrite: not implemented: Block#" + method.getName());
        });
    }
    /** The replaced block state of a {@code BlockPlaceEvent}; identity and type only. */
    static org.bukkit.block.BlockState state(org.bukkit.block.Block block, org.bukkit.Material type) {
        return (org.bukkit.block.BlockState) java.lang.reflect.Proxy.newProxyInstance(FerriteEventObjects.class.getClassLoader(), new Class<?>[]{org.bukkit.block.BlockState.class}, (self, method, args) -> switch (method.getName()) {
            case "getBlock" -> block;
            case "getX" -> block.getX();
            case "getY" -> block.getY();
            case "getZ" -> block.getZ();
            case "getWorld" -> block.getWorld();
            case "getLocation" -> block.getLocation();
            case "getType" -> type;
            case "getBlockData" -> throw new UnsupportedOperationException("Ferrite: event blocks have no block data snapshot");
            case "hashCode" -> block.hashCode();
            case "equals" -> self == args[0];
            case "toString" -> "BlockState[" + block + "]";
            default -> throw new UnsupportedOperationException("Ferrite: not implemented: BlockState#" + method.getName());
        });
    }
}
