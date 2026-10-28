package dev.ferrite.server;
public final class Bootstrap {
    static final Object LEGACY_LOCK = new Object();
    private static FerriteServer server;
    private Bootstrap() {}
    public static void start(String[] paths) { start(paths, null); }
    public static void start(String[] paths, boolean[] regionAware) {
        if (!"false".equals(System.getProperty("ferrite.native"))) java.util.logging.Logger.getLogger("").addHandler(new java.util.logging.Handler() {
            public void publish(java.util.logging.LogRecord record) { Native.log(record.getMessage()); }
            public void flush() {} public void close() {}
        });
        server = new FerriteServer();
        org.bukkit.Bukkit.setServer(server);
        synchronized (LEGACY_LOCK) { ((FerritePluginManager) server.getPluginManager()).loadOrdered(paths, regionAware); }
    }
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
