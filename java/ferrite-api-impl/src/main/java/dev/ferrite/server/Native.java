package dev.ferrite.server;

/** Entry points for the host bridge. Fallback mode is intended for Java tests. */
public final class Native {
    private Native() {}
    private record FallbackRegion(long id, long due, String world, int x, int z) {}
    private record FallbackEntity(long id, long due) {}
    private static final java.util.List<FallbackEntity> fallbackEntities = new java.util.ArrayList<>();
    private record FallbackGlobal(long id, long due) {}
    private static final java.util.List<FallbackGlobal> fallbackGlobals = new java.util.ArrayList<>();
    private static volatile long fallbackTick;
    private static final java.util.List<FallbackRegion> fallbackRegions = new java.util.ArrayList<>();
    private static final ThreadLocal<FallbackRegion> currentFallbackRegion = new ThreadLocal<>();
    static void tickFallback() {
        if (enabled()) return;
        java.util.List<FallbackRegion> ready = new java.util.ArrayList<>();
        synchronized (fallbackRegions) {
            fallbackTick++;
            fallbackRegions.removeIf(entry -> { if (entry.due() <= fallbackTick) { ready.add(entry); return true; } return false; });
        }
        java.util.List<FallbackGlobal> globals = new java.util.ArrayList<>();
        synchronized (fallbackGlobals) { fallbackGlobals.removeIf(entry -> { if (entry.due() <= fallbackTick) { globals.add(entry); return true; } return false; }); }
        for (FallbackGlobal entry : globals) Bootstrap.runGlobalTask(entry.id());
        java.util.List<FallbackEntity> entities = new java.util.ArrayList<>();
        synchronized (fallbackEntities) { fallbackEntities.removeIf(entry -> { if (entry.due() <= fallbackTick) { entities.add(entry); return true; } return false; }); }
        for (FallbackEntity entry : entities) Bootstrap.runEntityTask(entry.id());
        for (FallbackRegion entry : ready) {
            currentFallbackRegion.set(entry);
            try { Bootstrap.runRegionTask(entry.id()); } finally { currentFallbackRegion.remove(); }
        }
    }
    static boolean enabled() { return !"false".equals(System.getProperty("ferrite.native")); }
    public static void log(String message) { if (enabled()) Bindings.log(message); else System.out.println(message); }
    public static void scheduleGlobal(long taskId, long delay) { if (enabled()) Bindings.scheduleGlobal(taskId, delay); else synchronized (fallbackGlobals) { fallbackGlobals.add(new FallbackGlobal(taskId, fallbackTick + Math.max(1, delay))); } }
    public static void scheduleRegion(long taskId, String world, int chunkX, int chunkZ, long delay, String plugin) { if (enabled()) Bindings.scheduleRegion(taskId, world, chunkX, chunkZ, delay, plugin); else synchronized (fallbackRegions) { fallbackRegions.add(new FallbackRegion(taskId, fallbackTick + Math.max(1, delay), world, chunkX, chunkZ)); } }
    public static void scheduleEntity(long taskId, String entity, long delay, String plugin) { if (enabled()) Bindings.scheduleEntity(taskId, entity, delay, plugin); else synchronized (fallbackEntities) { fallbackEntities.add(new FallbackEntity(taskId, fallbackTick + Math.max(1, delay))); } }
    public static boolean isRegionThread(String world, int chunkX, int chunkZ) { if (enabled()) return Bindings.isRegionThread(world, chunkX, chunkZ); FallbackRegion region = currentFallbackRegion.get(); return region != null && region.world().equals(world) && region.x() == chunkX && region.z() == chunkZ; }
    public static long currentTick() { return enabled() ? Bindings.currentTick() : fallbackTick; }
    public static boolean dispatchCommand(org.bukkit.command.CommandSender sender, String line) {
        org.bukkit.Server server = org.bukkit.Bukkit.getServer();
        String identity;
        if (sender == server.getConsoleSender()) identity = "console";
        else if (sender instanceof org.bukkit.entity.Player player && server.getPlayer(player.getUniqueId()) == player) identity = player.getUniqueId().toString();
        else return false;
        return enabled() ? Bindings.dispatchCommand(identity, line) : server.dispatchCommand(sender, line);
    }
    static final class Bindings {
        static native void log(String message);
        static native void scheduleGlobal(long taskId, long delay);
        static native void scheduleRegion(long taskId, String world, int chunkX, int chunkZ, long delay, String plugin);
        static native void scheduleEntity(long taskId, String entity, long delay, String plugin);
        static native boolean isRegionThread(String world, int chunkX, int chunkZ);
        static native long currentTick();
        static native boolean dispatchCommand(String sender, String line);
    }
}
