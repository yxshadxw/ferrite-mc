package dev.ferrite.server;
public final class Bootstrap {
    static final Object LEGACY_LOCK = new Object();
    private static FerriteServer server;
    private static final java.util.List<String> initialIpBans = new java.util.ArrayList<>();
    private static FerriteServer.Settings settings = new FerriteServer.Settings("Ferrite", 20, 25565, 10, false, true);
    public static void configure(String motd, int maxPlayers, int port, int viewDistance, boolean whitelist, boolean onlineMode) {
        initialIpBans.clear();
        settings = new FerriteServer.Settings(motd, maxPlayers, port, viewDistance, whitelist, onlineMode);
    }
    public static void configureIpBan(String ip) { initialIpBans.add(ip); }
    private Bootstrap() {}
    public static void start(String[] paths) { start(paths, null); }
    public static void start(String[] paths, boolean[] regionAware) {
        if (!"false".equals(System.getProperty("ferrite.native"))) java.util.logging.Logger.getLogger("").addHandler(new java.util.logging.Handler() {
            public void publish(java.util.logging.LogRecord record) { Native.log(record.getMessage()); }
            public void flush() {} public void close() {}
        });
        server = new org.bukkit.craftbukkit.CraftServer(settings);
        initialIpBans.forEach(server::seedIpBan);
        org.bukkit.Bukkit.setServer(server);
        synchronized (LEGACY_LOCK) { server.plugins.loadOrdered(paths, regionAware); }
    }
    public static void registerWorld(String id, String name, long time, int x, int y, int z) { server.registerWorld(FerriteWorld.create(server, java.util.UUID.fromString(id), name, time, x, y, z)); }
    public static void setHeightSnapshot(String world, int x, int z, int y) { FerriteWorld.setHeight(server.getWorld(java.util.UUID.fromString(world)), x, z, y); }
    public static void setBlockSnapshot(String world, int x, int y, int z, String material) { FerriteWorld.setSnapshot(server.getWorld(java.util.UUID.fromString(world)), x, y, z, material); }
    public static void registerPlayer(String id, String name, boolean operator) { registerPlayer(id, name, operator, 20.0, 20, 0, null, 0, 0, 0, 0, 0); }
    public static void registerPlayer(String id, String name, boolean operator, double health, int food, int mode, String world, double x, double y, double z, float yaw, float pitch) { var location = world == null ? null : new org.bukkit.Location(java.util.Objects.requireNonNull(server.getWorld(java.util.UUID.fromString(world))), x, y, z, yaw, pitch); server.registerPlayer(FerritePlayer.create(server, java.util.UUID.fromString(id), name, operator, health, food, mode, location)); }
    public static void setInventorySnapshot(String player, int slot, String material, int amount) { ((FerritePlayer) java.util.Objects.requireNonNull(server.getPlayer(java.util.UUID.fromString(player)))).inventorySnapshot(slot, material, amount); }
    public static void registerOfflinePlayer(String id, String name, boolean operator) { server.registerOfflinePlayer(java.util.UUID.fromString(id), name, operator); }
    public static boolean isPluginEnabled(String name) { return server.getPluginManager().isPluginEnabled(name); }
    public static boolean dispatchCommand(String sender, String line) {
        org.bukkit.command.CommandSender resolved;
        if ("console".equals(sender)) resolved = server.getConsoleSender();
        else {
            try { resolved = server.getPlayer(java.util.UUID.fromString(sender)); }
            catch (IllegalArgumentException | NullPointerException e) { return false; }
        }
        synchronized (LEGACY_LOCK) { return resolved != null && server.dispatchCommand(resolved, line); }
    }
    public static void tick() { synchronized (LEGACY_LOCK) { server.scheduler.tick(); Native.tickFallback(); } }
    public static void runGlobalTask(long id) { synchronized (LEGACY_LOCK) { server.scheduler.executeId(id); } }
    public static void runEntityTask(long id) { server.entitySchedulers.execute(id); }
    public static void runRegionTask(long id) { server.regionScheduler.execute(id); }
    public static void shutdown() { synchronized (LEGACY_LOCK) { server.getPluginManager().disablePlugins(); server.asyncScheduler.shutdown(); } }
}
