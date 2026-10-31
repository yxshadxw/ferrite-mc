package dev.ferrite.server;

public class FerriteServer extends ServerStubs {
    public record Settings(String motd, int maxPlayers, int port, int viewDistance, boolean whitelist, boolean onlineMode, boolean allowFlight, boolean allowNether, boolean allowEnd, boolean generateStructures, int maxWorldSize, int simulationDistance, int spawnRadius, int idleTimeout, int pauseWhenEmptyTime, int connectionThrottle) {
        public Settings(String motd, int maxPlayers, int port, int viewDistance, boolean whitelist, boolean onlineMode) { this(motd, maxPlayers, port, viewDistance, whitelist, onlineMode, false, true, true, true, 29999984, 10, 10, 0, -1, -1); }
    }
    private final Settings settings;
    private volatile String motd;
    private volatile int maxPlayers;
    private volatile org.bukkit.GameMode defaultGameMode = org.bukkit.GameMode.SURVIVAL;
    private volatile boolean whitelistEnforced;
    private volatile int spawnRadius;
    private volatile int idleTimeout;
    private volatile int pauseWhenEmptyTime;
    private volatile boolean stopping;
    private volatile String shutdownMessage;
    volatile String pluginsFolder;
    public FerriteServer() { this(new Settings("Ferrite", 20, 25565, 10, false, true)); }
    protected FerriteServer(Settings settings) { this.settings = settings; this.whitelistEnabled = settings.whitelist(); this.whitelistEnforced = settings.whitelist(); this.motd = settings.motd(); this.maxPlayers = settings.maxPlayers(); this.spawnRadius = settings.spawnRadius(); this.idleTimeout = settings.idleTimeout(); this.pauseWhenEmptyTime = settings.pauseWhenEmptyTime(); this.shutdownMessage = System.getProperty("ferrite.shutdownMessage"); facade.paperPluginManager = plugins; }
    /** Marks the server as shutting down so plugins polling {@link #isStopping()} stop work. */
    void beginShutdown() { stopping = true; }
    @Override public String getMotd() { return motd; }
    @Override public void setMotd(String value) { motd = java.util.Objects.requireNonNull(value, "motd"); }
    @Override public net.kyori.adventure.text.Component motd() { return net.kyori.adventure.text.Component.text(motd); }
    @Override public void motd(net.kyori.adventure.text.Component value) { motd = net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText().serialize(java.util.Objects.requireNonNull(value, "motd")); }
    @Override public int getMaxPlayers() { return maxPlayers; }
    @Override public void setMaxPlayers(int value) { maxPlayers = value; }
    @Override public int getPort() { return settings.port(); }
    @Override public int getViewDistance() { return settings.viewDistance(); }
    @Override public boolean getOnlineMode() { return settings.onlineMode(); }
    @Override public boolean hasWhitelist() { return whitelistEnabled; }
    private final java.util.concurrent.ConcurrentMap<java.util.UUID, org.bukkit.World> worlds = new java.util.concurrent.ConcurrentHashMap<>();
    public void registerWorld(org.bukkit.World world) { worlds.put(world.getUID(), world); }
    public void unregisterWorld(java.util.UUID id) { worlds.remove(id); }
    @Override public java.util.List<org.bukkit.World> getWorlds() { return java.util.List.copyOf(worlds.values()); }
    @Override public org.bukkit.World getWorld(java.util.UUID id) { return worlds.get(id); }
    @Override public org.bukkit.World getWorld(String name) { return worlds.values().stream().filter(world -> world.getName().equalsIgnoreCase(name)).findFirst().orElse(null); }
    @Override public org.bukkit.World getWorld(net.kyori.adventure.key.Key key) { return worlds.values().stream().filter(world -> world.key().equals(key)).findFirst().orElse(null); }
    private final java.util.concurrent.ConcurrentMap<java.util.UUID, org.bukkit.entity.Player> players = new java.util.concurrent.ConcurrentHashMap<>();
    public void registerPlayer(org.bukkit.entity.Player player) { players.put(player.getUniqueId(), player); }
    public void unregisterPlayer(java.util.UUID id) { players.remove(id); }
    @Override public org.bukkit.entity.Player getPlayer(java.util.UUID id) { return players.get(id); }
    @Override public org.bukkit.entity.Player getPlayerExact(String name) {
        java.util.Objects.requireNonNull(name, "name");
        return players.values().stream().filter(p -> p.getName().equalsIgnoreCase(name)).findFirst().orElse(null);
    }
    @Override public org.bukkit.entity.Player getPlayer(String name) {
        java.util.Objects.requireNonNull(name, "name");
        var exact = getPlayerExact(name);
        if (exact != null) return exact;
        String prefix = name.toLowerCase(java.util.Locale.ROOT);
        return players.values().stream().filter(p -> p.getName().toLowerCase(java.util.Locale.ROOT).startsWith(prefix)).min(java.util.Comparator.comparingInt(p -> p.getName().length())).orElse(null);
    }
    private final org.bukkit.plugin.messaging.Messenger messenger = new org.bukkit.plugin.messaging.StandardMessenger();
    @Override public org.bukkit.plugin.messaging.Messenger getMessenger() { return messenger; }
    @Override public java.util.Set<String> getListeningPluginChannels() { return messenger.getIncomingChannels(); }
    private final java.util.concurrent.ConcurrentMap<java.util.UUID, org.bukkit.OfflinePlayer> offlinePlayers = new java.util.concurrent.ConcurrentHashMap<>();
    private final java.util.concurrent.ConcurrentMap<java.util.UUID, String> offlineNames = new java.util.concurrent.ConcurrentHashMap<>();
    private final java.util.concurrent.ConcurrentMap<String, java.util.UUID> offlineIds = new java.util.concurrent.ConcurrentHashMap<>();
    private final java.util.Set<java.util.UUID> operators = java.util.concurrent.ConcurrentHashMap.newKeySet();
    /** Remember a player seen while offline so plugins can resolve its name and id. */
    public void registerOfflinePlayer(java.util.UUID id, String name, boolean operator) {
        java.util.Objects.requireNonNull(id, "id");
        if (name != null) { offlineNames.put(id, name); offlineIds.put(name.toLowerCase(java.util.Locale.ROOT), id); }
        if (operator) operators.add(id); else operators.remove(id);
    }
    private org.bukkit.OfflinePlayer offline(java.util.UUID id, String name) {
        return offlinePlayers.computeIfAbsent(id, key -> (org.bukkit.OfflinePlayer) java.lang.reflect.Proxy.newProxyInstance(getClass().getClassLoader(), new Class<?>[]{org.bukkit.OfflinePlayer.class}, (proxy, method, args) -> switch (method.getName()) {
            case "getUniqueId" -> key;
            case "getName" -> offlineNames.getOrDefault(key, name);
            case "getPlayer" -> players.get(key);
            case "isOnline" -> players.containsKey(key);
            case "isOp" -> operators.contains(key);
            case "setOp" -> { boolean value = (boolean) args[0]; if (!Native.playerOperator(key.toString(), value)) throw new IllegalStateException("No player command handler"); if (value) operators.add(key); else operators.remove(key); yield null; }
            case "hashCode" -> key.hashCode();
            case "equals" -> args[0] instanceof org.bukkit.OfflinePlayer other && key.equals(other.getUniqueId());
            case "toString" -> "OfflinePlayer[" + key + "]";
            default -> throw new UnsupportedOperationException("Ferrite: not implemented: OfflinePlayer#" + method.getName());
        }));
    }
    @Override public org.bukkit.OfflinePlayer getOfflinePlayer(java.util.UUID id) { var player = players.get(id); return player != null ? player : offline(id, offlineNames.get(id)); }
    @Override public org.bukkit.OfflinePlayer getOfflinePlayer(String name) {
        java.util.Objects.requireNonNull(name, "name");
        var player = getPlayerExact(name);
        if (player != null) return player;
        var known = offlineIds.get(name.toLowerCase(java.util.Locale.ROOT));
        if (known != null) return offline(known, name);
        return offline(java.util.UUID.nameUUIDFromBytes(("OfflinePlayer:" + name).getBytes(java.nio.charset.StandardCharsets.UTF_8)), name);
    }
    @Override public java.util.Set<org.bukkit.OfflinePlayer> getOperators() {
        var result = new java.util.HashSet<org.bukkit.OfflinePlayer>();
        for (var id : operators) result.add(getOfflinePlayer(id));
        for (var player : players.values()) if (player.isOp()) result.add(player);
        return java.util.Set.copyOf(result);
    }
    @Override public org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder holder, int size) { return FerriteInventory.create(holder, org.bukkit.event.inventory.InventoryType.CHEST, size); }
    @Override public org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder holder, int size, String title) { java.util.Objects.requireNonNull(title); return createInventory(holder, size); }
    @Override public org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder holder, int size, net.kyori.adventure.text.Component title) { java.util.Objects.requireNonNull(title); return createInventory(holder, size); }
    @Override public org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder holder, org.bukkit.event.inventory.InventoryType type) { return FerriteInventory.create(holder, type, type.getDefaultSize()); }
    @Override public org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder holder, org.bukkit.event.inventory.InventoryType type, String title) { java.util.Objects.requireNonNull(title); return createInventory(holder, type); }
    @Override public org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder holder, org.bukkit.event.inventory.InventoryType type, net.kyori.adventure.text.Component title) { java.util.Objects.requireNonNull(title); return createInventory(holder, type); }
    private final org.bukkit.scoreboard.ScoreboardManager scoreboards = new FerriteScoreboards();
    @Override public org.bukkit.scoreboard.ScoreboardManager getScoreboardManager() { return scoreboards; }
    private final FerriteIpBanList ipBans = new FerriteIpBanList();
    void seedIpBan(String ip) { ipBans.seed(ip); }
    @SuppressWarnings("unchecked") @Override public <T extends org.bukkit.BanList<?>> T getBanList(org.bukkit.BanList.Type type) { if (type != org.bukkit.BanList.Type.IP) throw new UnsupportedOperationException("Ferrite: profile bans not implemented"); return (T) ipBans; }
    @SuppressWarnings("unchecked") @Override public <B extends org.bukkit.BanList<E>, E> B getBanList(io.papermc.paper.ban.BanListType<B> type) { if (type != io.papermc.paper.ban.BanListType.IP) throw new UnsupportedOperationException("Ferrite: profile bans not implemented"); return (B) ipBans; }
    @Override public void banIP(String address) { ipBans.addBan(address, null, (java.util.Date) null, null); }
    @Override public void banIP(java.net.InetAddress address) { ipBans.addBan(address, null, (java.util.Date) null, null); }
    @Override public void unbanIP(String address) { ipBans.pardon(address); }
    @Override public void unbanIP(java.net.InetAddress address) { ipBans.pardon(address); }
    @Override public java.util.Set<String> getIPBans() { return ipBans.addresses(); }
    private volatile boolean whitelistEnabled;
    @Override public void setWhitelist(boolean value) { if (!Native.serverWhitelist(value)) throw new IllegalStateException("No server command handler"); whitelistEnabled = value; }
    private final Thread primaryThread = Thread.currentThread();
    private final org.bukkit.plugin.ServicesManager services = new org.bukkit.plugin.SimpleServicesManager();
    @Override public org.bukkit.plugin.ServicesManager getServicesManager() { return services; }
    @Override public net.kyori.adventure.text.Component permissionMessage() { return net.kyori.adventure.text.Component.text("You do not have permission to use this command."); }
    @Override public boolean isPrimaryThread() { return Thread.currentThread() == primaryThread || Thread.holdsLock(Bootstrap.LEGACY_LOCK); }
    private final java.util.Map<String, org.bukkit.command.Command> commands = new java.util.HashMap<>();
    private final org.bukkit.command.SimpleCommandMap commandMap = new org.bukkit.command.SimpleCommandMap(this, commands);
    @Override public org.bukkit.command.CommandMap getCommandMap() { return commandMap; }
    void registerCommands(org.bukkit.plugin.java.JavaPlugin plugin) throws ReflectiveOperationException {
        var constructor = org.bukkit.command.PluginCommand.class.getDeclaredConstructor(String.class, org.bukkit.plugin.Plugin.class);
        constructor.setAccessible(true);
        if (plugin.getDescription().getCommands() == null) return;
        for (String name : plugin.getDescription().getCommands().keySet()) {
            var command = constructor.newInstance(name, plugin);
            var properties = plugin.getDescription().getCommands().get(name);
            Object permission = properties.get("permission");
            if (permission instanceof String value) command.setPermission(value);
            commands.putIfAbsent(name.toLowerCase(java.util.Locale.ROOT), command);
            commands.put(plugin.getName().toLowerCase(java.util.Locale.ROOT) + ":" + name.toLowerCase(java.util.Locale.ROOT), command);
            Object aliases = properties.get("aliases");
            java.util.List<String> names = new java.util.ArrayList<>();
            if (aliases instanceof String alias) names.add(alias);
            else if (aliases instanceof java.util.List<?> list) for (Object alias : list) if (alias instanceof String value) names.add(value);
            command.setAliases(names);
            for (String alias : names) {
                String key = alias.toLowerCase(java.util.Locale.ROOT);
                commands.putIfAbsent(key, command);
                commands.put(plugin.getName().toLowerCase(java.util.Locale.ROOT) + ":" + key, command);
            }
        }
    }
    void unregisterCommands(org.bukkit.plugin.Plugin plugin) { commands.values().removeIf(command -> command instanceof org.bukkit.command.PluginCommand pc && pc.getPlugin() == plugin); }
    @Override public org.bukkit.command.PluginCommand getPluginCommand(String name) { var command = commands.get(name.toLowerCase(java.util.Locale.ROOT)); return command instanceof org.bukkit.command.PluginCommand pc ? pc : null; }
    @Override public boolean dispatchCommand(org.bukkit.command.CommandSender sender, String line) {
        return commandMap.dispatch(sender, line);
    }
    private final org.bukkit.command.ConsoleCommandSender console = createConsoleSender();
    @Override public org.bukkit.command.ConsoleCommandSender getConsoleSender() { return console; }
    private org.bukkit.command.ConsoleCommandSender createConsoleSender() {
        return (org.bukkit.command.ConsoleCommandSender) java.lang.reflect.Proxy.newProxyInstance(getClass().getClassLoader(), new Class<?>[]{org.bukkit.command.ConsoleCommandSender.class}, (proxy, method, args) -> {
            if (method.getName().equals("sendMessage") && args != null) { for (Object arg : args) if (arg instanceof String message) getLogger().info(message); return null; }
            if (method.getName().equals("equals")) return proxy == args[0];
            if (method.getName().equals("hashCode")) return System.identityHashCode(proxy);
            if (method.getName().equals("toString")) return "ConsoleCommandSender";
            if (method.getName().equals("getName")) return "CONSOLE";
            if (method.getName().equals("getServer")) return this;
            if (method.getName().equals("isOp") || method.getName().equals("hasPermission")) return true;
            throw new UnsupportedOperationException("Ferrite: not implemented: ConsoleCommandSender#" + method.getName());
        });
    }
    final FerriteEntitySchedulers entitySchedulers = new FerriteEntitySchedulers();
    public io.papermc.paper.threadedregions.scheduler.EntityScheduler entityScheduler(java.util.UUID id, java.util.function.BooleanSupplier alive) { return entitySchedulers.forEntity(id, alive); }
    final FerriteAsyncScheduler asyncScheduler = new FerriteAsyncScheduler();
    @Override public io.papermc.paper.threadedregions.scheduler.AsyncScheduler getAsyncScheduler() { return asyncScheduler; }
    final FerriteRegionScheduler regionScheduler = new FerriteRegionScheduler();
    @Override public io.papermc.paper.threadedregions.scheduler.RegionScheduler getRegionScheduler() { return regionScheduler; }
    @Override public boolean isOwnedByCurrentRegion(org.bukkit.World world, int x, int z) { return Native.isRegionThread(world.getUID().toString(), x, z); }
    @Override public boolean isOwnedByCurrentRegion(org.bukkit.World world, io.papermc.paper.math.Position pos) { return isOwnedByCurrentRegion(world, pos.blockX() >> 4, pos.blockZ() >> 4); }
    @Override public boolean isOwnedByCurrentRegion(org.bukkit.World world, io.papermc.paper.math.Position pos, int radius) { return isOwnedByCurrentRegion(world, pos.blockX() >> 4, pos.blockZ() >> 4, radius); }
    @Override public boolean isOwnedByCurrentRegion(org.bukkit.Location location) { return isOwnedByCurrentRegion(java.util.Objects.requireNonNull(location.getWorld()), location.getBlockX() >> 4, location.getBlockZ() >> 4); }
    @Override public boolean isOwnedByCurrentRegion(org.bukkit.Location location, int radius) { return isOwnedByCurrentRegion(java.util.Objects.requireNonNull(location.getWorld()), location.getBlockX() >> 4, location.getBlockZ() >> 4, radius); }
    @Override public boolean isOwnedByCurrentRegion(org.bukkit.World world, int x, int z, int radius) { if (radius < 0) throw new IllegalArgumentException("radius"); long minX = (long) x - radius, minZ = (long) z - radius, maxX = (long) x + radius, maxZ = (long) z + radius; if (minX < Integer.MIN_VALUE || minZ < Integer.MIN_VALUE || maxX > Integer.MAX_VALUE || maxZ > Integer.MAX_VALUE) return false; return isOwnedByCurrentRegion(world, (int) minX, (int) minZ, (int) maxX, (int) maxZ); }
    @Override public boolean isOwnedByCurrentRegion(org.bukkit.World world, int minX, int minZ, int maxX, int maxZ) { if (minX > maxX || minZ > maxZ) throw new IllegalArgumentException("invalid range"); for (long x = minX; x <= maxX; x++) for (long z = minZ; z <= maxZ; z++) if (!isOwnedByCurrentRegion(world, (int) x, (int) z)) return false; return true; }
    @Override public boolean isOwnedByCurrentRegion(org.bukkit.entity.Entity entity) { return entitySchedulers.isOwned(entity.getUniqueId()); }
    final FerriteScheduler scheduler = new FerriteScheduler();
    private final FerriteGlobalScheduler globalScheduler = new FerriteGlobalScheduler(scheduler);
    @Override public io.papermc.paper.threadedregions.scheduler.GlobalRegionScheduler getGlobalRegionScheduler() { return globalScheduler; }
    @Override public org.bukkit.scheduler.BukkitScheduler getScheduler() { return scheduler; }
    final FerritePluginManager plugins = new FerritePluginManager(this);
    final org.bukkit.plugin.SimplePluginManager facade = new org.bukkit.plugin.SimplePluginManager(this, commandMap);
    @Override public org.bukkit.plugin.PluginManager getPluginManager() { return facade; }
    @Override public String getName() { return "Ferrite"; }
    @Override public String getVersion() { return "Ferrite 0.1.0"; }
    @Override public String getBukkitVersion() { return "26.3"; }
    @Override public String getMinecraftVersion() { return "26.3"; }
    @Override public java.util.logging.Logger getLogger() { return java.util.logging.Logger.getLogger("Ferrite"); }
    @Override public java.util.Collection<? extends org.bukkit.entity.Player> getOnlinePlayers() { return java.util.List.copyOf(players.values()); }
    @Override public org.bukkit.GameMode getDefaultGameMode() { return defaultGameMode; }
    @Override public void setDefaultGameMode(org.bukkit.GameMode mode) { defaultGameMode = java.util.Objects.requireNonNull(mode, "mode"); }
    @Override public boolean getAllowFlight() { return settings.allowFlight(); }
    @Override public boolean getAllowNether() { return settings.allowNether(); }
    @Override public boolean getAllowEnd() { return settings.allowEnd(); }
    @Override public boolean getGenerateStructures() { return settings.generateStructures(); }
    @Override public int getMaxWorldSize() { return settings.maxWorldSize(); }
    @Override public int getSimulationDistance() { return settings.simulationDistance(); }
    @Override public int getSpawnRadius() { return spawnRadius; }
    @Override public void setSpawnRadius(int value) { if (value < 0) throw new IllegalArgumentException("spawn radius must not be negative"); spawnRadius = value; }
    @Override public int getIdleTimeout() { return idleTimeout; }
    @Override public void setIdleTimeout(int value) { if (value < 0) throw new IllegalArgumentException("idle timeout must not be negative"); idleTimeout = value; }
    @Override public int getPauseWhenEmptyTime() { return pauseWhenEmptyTime; }
    @Override public void setPauseWhenEmptyTime(int value) { if (value < -1) throw new IllegalArgumentException("pause time must be -1 or more"); pauseWhenEmptyTime = value; }
    @Override public long getConnectionThrottle() { return settings.connectionThrottle(); }
    @Override public boolean isWhitelistEnforced() { return whitelistEnforced; }
    @Override public void setWhitelistEnforced(boolean value) { whitelistEnforced = value; }
    @Override public boolean isStopping() { return stopping; }
    @Override public String getShutdownMessage() { return shutdownMessage; }
    @Override public net.kyori.adventure.text.Component shutdownMessage() { return shutdownMessage == null ? null : net.kyori.adventure.text.Component.text(shutdownMessage); }
    /** Ticks the host scheduler has run, from the shared counter the scheduler tasks use. */
    @Override public int getCurrentTick() { return (int) Native.currentTick(); }
    @Override public int broadcast(net.kyori.adventure.text.Component message, String permission) {
        java.util.Objects.requireNonNull(message, "message");
        int sent = 0;
        for (var player : players.values()) {
            if (permission != null && !player.hasPermission(permission)) continue;
            player.sendMessage(message);
            sent++;
        }
        return sent;
    }
    @Override public java.util.List<org.bukkit.entity.Player> matchPlayer(String name) {
        java.util.Objects.requireNonNull(name, "name");
        String prefix = name.toLowerCase(java.util.Locale.ROOT);
        return players.values().stream().filter(player -> player.getName().toLowerCase(java.util.Locale.ROOT).startsWith(prefix)).toList();
    }
    @Override public java.util.UUID getPlayerUniqueId(String name) {
        var player = getPlayerExact(java.util.Objects.requireNonNull(name, "name"));
        return player == null ? null : player.getUniqueId();
    }
    @Override public org.bukkit.OfflinePlayer getOfflinePlayerIfCached(String name) {
        java.util.Objects.requireNonNull(name, "name");
        var player = getPlayerExact(name);
        if (player != null) return player;
        var known = offlineIds.get(name.toLowerCase(java.util.Locale.ROOT));
        return known == null ? null : offline(known, offlineNames.get(known));
    }
    /** Only players are bridged; other entity kinds are not tracked by the host yet. */
    @Override public org.bukkit.entity.Entity getEntity(java.util.UUID id) { return players.get(java.util.Objects.requireNonNull(id, "id")); }
    @Override public java.io.File getPluginsFolder() { return new java.io.File(pluginsFolder == null ? "plugins" : pluginsFolder); }
    /** The host does not expose a bind address, matching the empty server-ip default. */
    @Override public String getIp() { return ""; }
    /** Hardcore is not a host setting yet. */
    @Override public boolean isHardcore() { return false; }
    @Override public boolean isLoggingIPs() { return true; }
    /** Online mode requires Mojang-signed profiles, so it also enforces secure profiles. */
    @Override public boolean isEnforcingSecureProfiles() { return settings.onlineMode(); }
    @Override public boolean isAcceptingTransfers() { return false; }
    @Override public boolean getHideOnlinePlayers() { return false; }
    /** Every join is forced onto the default game mode by the host's login packet. */
    @Override public boolean forcesDefaultGameMode() { return false; }
    @Override public java.io.File getWorldContainer() { return new java.io.File(".").getAbsoluteFile(); }
    @Override public java.nio.file.Path getLevelDirectory() { return getWorldContainer().toPath(); }
    @Override public String getUpdateFolder() { return "update"; }
    @Override public java.io.File getUpdateFolderFile() { return new java.io.File(getWorldContainer(), getUpdateFolder()); }
    @Override public org.bukkit.Warning.WarningState getWarningState() { return org.bukkit.Warning.WarningState.DEFAULT; }
    @Override public boolean isPaused() { return false; }
    @Override public boolean isTickingWorlds() { return !stopping; }
    @Override public boolean isGlobalTickThread() { return isPrimaryThread(); }
    /** The host persists player data on quit and shutdown; there is no separate flush. */
    @Override public void savePlayers() {}
    @Override public int getMaxChainedNeighborUpdates() { return 1_000_000; }
    @Override public java.util.List<String> getInitialEnabledPacks() { return java.util.List.of("vanilla"); }
    @Override public java.util.List<String> getInitialDisabledPacks() { return java.util.List.of(); }
    @Override public String getResourcePack() { return null; }
    @Override public String getResourcePackHash() { return null; }
    @Override public String getResourcePackPrompt() { return null; }
    @Override public boolean isResourcePackRequired() { return false; }
    @Override public org.bukkit.packs.ResourcePack getServerResourcePack() { return null; }
    @Override public java.util.Iterator<org.bukkit.boss.KeyedBossBar> getBossBars() { return java.util.Collections.emptyIterator(); }
    @Override public org.bukkit.boss.KeyedBossBar getBossBar(org.bukkit.NamespacedKey key) { java.util.Objects.requireNonNull(key, "key"); return null; }
    @Override public boolean removeBossBar(org.bukkit.NamespacedKey key) { java.util.Objects.requireNonNull(key, "key"); return false; }
    private final FerriteRegistryAccess registries = new FerriteRegistryAccess();
    @Override public <T extends org.bukkit.Keyed> org.bukkit.Registry<T> getRegistry(Class<T> type) { return registries.getRegistry(type); }
    private final org.bukkit.inventory.ItemFactory itemFactory = new FerriteItemFactory();
    @Override public org.bukkit.inventory.ItemFactory getItemFactory() { return itemFactory; }
    @Override public void reloadPermissions() { facade.permissions.values().forEach(plugins::recalculatePermissionDefaults); }
}
