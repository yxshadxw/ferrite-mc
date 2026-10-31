package dev.ferrite.server;

public class FerriteServer extends ServerStubs {
    public record Settings(String motd, int maxPlayers, int port, int viewDistance, boolean whitelist, boolean onlineMode) {}
    private final Settings settings;
    public FerriteServer() { this(new Settings("Ferrite", 20, 25565, 10, false, true)); }
    protected FerriteServer(Settings settings) { this.settings = settings; this.whitelistEnabled = settings.whitelist(); facade.paperPluginManager = plugins; }
    @Override public String getMotd() { return settings.motd(); }
    @Override public int getMaxPlayers() { return settings.maxPlayers(); }
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

    // ---- runtime surface used by plugins at enable time -------------------------------------
    private final FerriteUnsafe unsafe = new FerriteUnsafe();
    @Override public org.bukkit.UnsafeValues getUnsafe() { return unsafe; }
    @SuppressWarnings("unchecked")
    @Override public <T extends org.bukkit.Keyed> org.bukkit.Registry<T> getRegistry(Class<T> type) {
        java.util.Objects.requireNonNull(type, "type");
        var access = io.papermc.paper.registry.RegistryAccess.registryAccess();
        if (type == org.bukkit.block.BlockType.class) return (org.bukkit.Registry<T>) access.getRegistry(io.papermc.paper.registry.RegistryKey.BLOCK);
        if (type == org.bukkit.inventory.ItemType.class) return (org.bukkit.Registry<T>) access.getRegistry(io.papermc.paper.registry.RegistryKey.ITEM);
        if (type == org.bukkit.enchantments.Enchantment.class) return (org.bukkit.Registry<T>) access.getRegistry(io.papermc.paper.registry.RegistryKey.ENCHANTMENT);
        if (type == org.bukkit.potion.PotionEffectType.class) return (org.bukkit.Registry<T>) access.getRegistry(io.papermc.paper.registry.RegistryKey.MOB_EFFECT);
        if (type == org.bukkit.attribute.Attribute.class) return (org.bukkit.Registry<T>) access.getRegistry(io.papermc.paper.registry.RegistryKey.ATTRIBUTE);
        if (type == org.bukkit.Sound.class) return (org.bukkit.Registry<T>) access.getRegistry(io.papermc.paper.registry.RegistryKey.SOUND_EVENT);
        if (type == org.bukkit.block.Biome.class) return (org.bukkit.Registry<T>) access.getRegistry(io.papermc.paper.registry.RegistryKey.BIOME);
        if (type == org.bukkit.inventory.MenuType.class) return (org.bukkit.Registry<T>) access.getRegistry(io.papermc.paper.registry.RegistryKey.MENU);
        if (type == org.bukkit.entity.EntityType.class) return (org.bukkit.Registry<T>) access.getRegistry(io.papermc.paper.registry.RegistryKey.ENTITY_TYPE);
        return access.getRegistry(type);
    }
    @Override public org.bukkit.block.data.BlockData createBlockData(String data) { return FerriteBlockData.create(data); }
    @Override public org.bukkit.block.data.BlockData createBlockData(org.bukkit.Material material) { return FerriteBlockData.create(material); }
    @Override public org.bukkit.block.data.BlockData createBlockData(org.bukkit.Material material, String data) {
        java.util.Objects.requireNonNull(material, "material");
        if (data != null && !data.isEmpty()) throw new UnsupportedOperationException("Ferrite: block-state properties are owned by the Rust server: " + data);
        return FerriteBlockData.create(material);
    }
    @Override public org.bukkit.block.data.BlockData createBlockData(org.bukkit.Material material, java.util.function.Consumer<? super org.bukkit.block.data.BlockData> consumer) {
        var data = FerriteBlockData.create(material);
        consumer.accept(data);
        return data;
    }
    @Override public int getCurrentTick() { return (int) Native.currentTick(); }
    private static final double[] NOMINAL_TPS = {20.0, 20.0, 20.0};
    /** Nominal until per-region tick metrics are exposed to the bridge (see docs/BACKLOG.md). */
    @Override public double[] getTPS() { return NOMINAL_TPS.clone(); }
    @Override public double getAverageTickTime() { return 50.0; }
    @Override public long[] getTickTimes() { return new long[100]; }
    private volatile boolean stopping;
    void markStopping() { stopping = true; }
    @Override public boolean isStopping() { return stopping; }
    private org.bukkit.GameMode defaultGameMode = org.bukkit.GameMode.SURVIVAL;
    @Override public org.bukkit.GameMode getDefaultGameMode() { return defaultGameMode; }
    @Override public void setDefaultGameMode(org.bukkit.GameMode mode) { defaultGameMode = java.util.Objects.requireNonNull(mode, "mode"); }
    @Override public org.bukkit.entity.Entity getEntity(java.util.UUID id) { return players.get(id); }
    @Override public java.util.List<org.bukkit.entity.Player> matchPlayer(String name) {
        var player = getPlayer(name);
        return player == null ? java.util.List.of() : java.util.List.of(player);
    }
    @Override public String getIp() { return "0.0.0.0"; }
    @Override public java.io.File getWorldContainer() { return new java.io.File("."); }
    @Override public java.io.File getPluginsFolder() { return new java.io.File("plugins"); }
    @Override public String getUpdateFolder() { return "update"; }
    @Override public java.io.File getUpdateFolderFile() { return new java.io.File("update"); }
    @Override public java.util.Set<org.bukkit.OfflinePlayer> getWhitelistedPlayers() { return java.util.Set.of(); }
    @Override public java.util.Set<org.bukkit.OfflinePlayer> getBannedPlayers() { return java.util.Set.of(); }
    private int spawnRadius;
    @Override public int getSpawnRadius() { return spawnRadius; }
    @Override public void setSpawnRadius(int radius) { if (radius < 0) throw new IllegalArgumentException("radius"); spawnRadius = radius; }
    @Override public int getSimulationDistance() { return settings.viewDistance(); }
    @Override public int getMaxWorldSize() { return 29999984; }
    @Override public boolean getAllowFlight() { return false; }
    @Override public boolean getAllowNether() { return true; }
    @Override public boolean getAllowEnd() { return true; }
    @Override public String getWorldType() { return "normal"; }
    @Override public boolean getGenerateStructures() { return true; }
    @Override public boolean isHardcore() { return false; }
    @Override public java.util.UUID getPlayerUniqueId(String name) { var player = getPlayerExact(name); return player == null ? null : player.getUniqueId(); }
    @Override public org.bukkit.OfflinePlayer getOfflinePlayerIfCached(String name) {
        var player = getPlayerExact(name);
        if (player != null) return player;
        var id = offlineIds.get(name.toLowerCase(java.util.Locale.ROOT));
        return id == null ? null : offline(id, name);
    }
}
