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
        Native.link();
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
    /** Resolves the live player object, registering a snapshot when the server has none yet. */
    private static org.bukkit.entity.Player ensurePlayer(String id, String name) {
        var uuid = java.util.UUID.fromString(id);
        var player = server.getPlayer(uuid);
        if (player == null) { player = FerritePlayer.create(server, uuid, name, false, 20.0, 20, 1, null); server.registerPlayer(player); }
        return player;
    }
    /** Fires Bukkit's {@code PlayerJoinEvent}. */
    public static void firePlayerJoin(String id, String name) {
        var player = ensurePlayer(id, name);
        synchronized (LEGACY_LOCK) { server.plugins.callEvent(new org.bukkit.event.player.PlayerJoinEvent(player, net.kyori.adventure.text.Component.empty())); }
    }
    /** Fires Bukkit's {@code PlayerQuitEvent} and forgets the disconnected player. */
    public static void firePlayerQuit(String id) {
        var player = server.getPlayer(java.util.UUID.fromString(id));
        if (player == null) return;
        synchronized (LEGACY_LOCK) { server.plugins.callEvent(new org.bukkit.event.player.PlayerQuitEvent(player, net.kyori.adventure.text.Component.empty(), org.bukkit.event.player.PlayerQuitEvent.QuitReason.DISCONNECTED)); }
        server.unregisterPlayer(player.getUniqueId());
    }
    /** Fires Paper's {@code AsyncChatEvent} then Bukkit's {@code AsyncPlayerChatEvent}; null means cancelled. */
    public static String fireChat(String id, String name, String message) {
        var player = ensurePlayer(id, name);
        String result = message;
        synchronized (LEGACY_LOCK) {
            if (server.plugins.hasListeners(io.papermc.paper.event.player.AsyncChatEvent.class)) {
                java.util.Set<net.kyori.adventure.audience.Audience> viewers = new java.util.LinkedHashSet<>(server.getOnlinePlayers());
                var text = net.kyori.adventure.text.Component.text(message);
                var event = new io.papermc.paper.event.player.AsyncChatEvent(true, player, viewers, io.papermc.paper.chat.ChatRenderer.defaultRenderer(), text, text, null);
                server.plugins.callEvent(event);
                if (event.isCancelled()) return null;
                result = net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText().serialize(event.message());
            }
            if (server.plugins.hasListeners(org.bukkit.event.player.AsyncPlayerChatEvent.class)) {
                var event = new org.bukkit.event.player.AsyncPlayerChatEvent(true, player, result, new java.util.LinkedHashSet<>(server.getOnlinePlayers()));
                server.plugins.callEvent(event);
                if (event.isCancelled()) return null;
                result = event.getMessage();
            }
        }
        return result;
    }
    /** Fires Bukkit's {@code PlayerMoveEvent}; false means a listener cancelled the move. */
    public static boolean firePlayerMove(String id, String name, double fromX, double fromY, double fromZ, float fromYaw, float fromPitch, double toX, double toY, double toZ, float toYaw, float toPitch) {
        var event = new org.bukkit.event.player.PlayerMoveEvent(ensurePlayer(id, name), new org.bukkit.Location(null, fromX, fromY, fromZ, fromYaw, fromPitch), new org.bukkit.Location(null, toX, toY, toZ, toYaw, toPitch));
        synchronized (LEGACY_LOCK) { server.plugins.callEvent(event); }
        return !event.isCancelled();
    }
    /** Fires Bukkit's {@code PlayerInteractEvent} for a block click; false means cancelled. */
    public static boolean firePlayerInteract(String id, String name, int action, int x, int y, int z, int face) {
        var event = new org.bukkit.event.player.PlayerInteractEvent(ensurePlayer(id, name), action == 0 ? org.bukkit.event.block.Action.LEFT_CLICK_BLOCK : org.bukkit.event.block.Action.RIGHT_CLICK_BLOCK, null, FerriteEventObjects.block(x, y, z), FerriteEventObjects.face(face), org.bukkit.inventory.EquipmentSlot.HAND);
        synchronized (LEGACY_LOCK) { server.plugins.callEvent(event); }
        return !event.isCancelled() && event.useInteractedBlock() != org.bukkit.event.Event.Result.DENY;
    }
    /** Fires Bukkit's {@code BlockBreakEvent}; false means a listener cancelled the break. */
    public static boolean fireBlockBreak(String id, String name, int x, int y, int z) {
        var event = new org.bukkit.event.block.BlockBreakEvent(FerriteEventObjects.block(x, y, z), ensurePlayer(id, name));
        synchronized (LEGACY_LOCK) { server.plugins.callEvent(event); }
        return !event.isCancelled();
    }
    /** Fires Bukkit's {@code BlockPlaceEvent}; false means a listener cancelled the placement. */
    public static boolean fireBlockPlace(String id, String name, int x, int y, int z, int face) {
        var block = FerriteEventObjects.block(x, y, z);
        var blockFace = FerriteEventObjects.face(face);
        var against = FerriteEventObjects.block(x - blockFace.getModX(), y - blockFace.getModY(), z - blockFace.getModZ());
        var event = new org.bukkit.event.block.BlockPlaceEvent(block, FerriteEventObjects.state(block), against, null, ensurePlayer(id, name), true, org.bukkit.inventory.EquipmentSlot.HAND);
        synchronized (LEGACY_LOCK) { server.plugins.callEvent(event); }
        return !event.isCancelled() && event.canBuild();
    }
    /** Fires Bukkit's {@code PlayerCommandPreprocessEvent}; false means a listener cancelled it. */
    public static boolean firePlayerCommand(String id, String name, String message) {
        var event = new org.bukkit.event.player.PlayerCommandPreprocessEvent(ensurePlayer(id, name), message);
        synchronized (LEGACY_LOCK) { server.plugins.callEvent(event); }
        return !event.isCancelled();
    }
    /** Lines of {@code name\taliases\tpermission\tusage} for every enabled plugin command. */
    public static String pluginCommands() {
        StringBuilder out = new StringBuilder();
        for (org.bukkit.plugin.Plugin plugin : server.plugins.getPlugins()) {
            if (!plugin.isEnabled()) continue;
            var commands = plugin.getDescription().getCommands();
            if (commands == null) continue;
            for (var entry : commands.entrySet()) {
                var properties = entry.getValue() == null ? java.util.Collections.<String, Object>emptyMap() : entry.getValue();
                Object aliases = properties.get("aliases");
                String aliasList = "";
                if (aliases instanceof java.util.Collection<?> list) aliasList = list.stream().map(String::valueOf).collect(java.util.stream.Collectors.joining(","));
                else if (aliases instanceof String alias) aliasList = alias;
                out.append(entry.getKey()).append('\t').append(aliasList).append('\t').append(properties.getOrDefault("permission", "")).append('\t').append(properties.getOrDefault("usage", "")).append('\n');
            }
        }
        return out.toString();
    }
    /** Runs a declared plugin command; 0 = unknown, 1 = handled, 2 = no permission. */
    public static int runPluginCommand(String sender, String label, String[] args) {
        org.bukkit.command.CommandSender resolved;
        if ("console".equals(sender)) resolved = server.getConsoleSender();
        else {
            try { resolved = server.getPlayer(java.util.UUID.fromString(sender)); }
            catch (IllegalArgumentException e) { return 0; }
        }
        if (resolved == null) return 0;
        var command = server.getPluginCommand(label);
        if (command == null) return 0;
        synchronized (LEGACY_LOCK) {
            if (!command.testPermission(resolved)) return 2;
            return command.execute(resolved, label, args) ? 1 : 0;
        }
    }
}
