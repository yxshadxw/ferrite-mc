package dev.ferrite.server;

/** Coordinate-only stand-ins for Bukkit event objects fired from the host. */
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
    static org.bukkit.Location location(double x, double y, double z, float yaw, float pitch) { return new org.bukkit.Location(null, x, y, z, yaw, pitch); }
    /** A block that knows where it is; its type and world are unknown to the host. */
    static org.bukkit.block.Block block(int x, int y, int z) {
        return (org.bukkit.block.Block) java.lang.reflect.Proxy.newProxyInstance(FerriteEventObjects.class.getClassLoader(), new Class<?>[]{org.bukkit.block.Block.class}, (self, method, args) -> switch (method.getName()) {
            case "getX" -> x;
            case "getY" -> y;
            case "getZ" -> z;
            case "getLocation" -> location(x, y, z, 0.0f, 0.0f);
            case "getType", "getBlockData", "getBlockKey" -> throw new UnsupportedOperationException("Ferrite: event blocks have no block snapshot");
            case "getWorld" -> throw new UnsupportedOperationException("Ferrite: event blocks have no world snapshot");
            case "hashCode" -> java.util.Objects.hash(x, y, z);
            case "equals" -> self == args[0];
            case "toString" -> "Block[" + x + "," + y + "," + z + "]";
            default -> throw new UnsupportedOperationException("Ferrite: not implemented: Block#" + method.getName());
        });
    }
    /** The replaced block state of a {@code BlockPlaceEvent}; only identity is known. */
    static org.bukkit.block.BlockState state(org.bukkit.block.Block block) {
        return (org.bukkit.block.BlockState) java.lang.reflect.Proxy.newProxyInstance(FerriteEventObjects.class.getClassLoader(), new Class<?>[]{org.bukkit.block.BlockState.class}, (self, method, args) -> switch (method.getName()) {
            case "getBlock" -> block;
            case "getX" -> block.getX();
            case "getY" -> block.getY();
            case "getZ" -> block.getZ();
            case "getLocation" -> block.getLocation();
            case "getType" -> throw new UnsupportedOperationException("Ferrite: event blocks have no block snapshot");
            case "hashCode" -> block.hashCode();
            case "equals" -> self == args[0];
            case "toString" -> "BlockState[" + block + "]";
            default -> throw new UnsupportedOperationException("Ferrite: not implemented: BlockState#" + method.getName());
        });
    }
}
