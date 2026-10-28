package dev.ferrite.server;

public final class FerriteServer extends ServerStubs {
    private final java.util.concurrent.ConcurrentMap<java.util.UUID, org.bukkit.entity.Player> players = new java.util.concurrent.ConcurrentHashMap<>();
    public void registerPlayer(org.bukkit.entity.Player player) { players.put(player.getUniqueId(), player); }
    public void unregisterPlayer(java.util.UUID id) { players.remove(id); }
    @Override public org.bukkit.entity.Player getPlayer(java.util.UUID id) { return players.get(id); }
    private final Thread primaryThread = Thread.currentThread();
    private final org.bukkit.plugin.ServicesManager services = new org.bukkit.plugin.SimpleServicesManager();
    @Override public org.bukkit.plugin.ServicesManager getServicesManager() { return services; }
    @Override public net.kyori.adventure.text.Component permissionMessage() { return net.kyori.adventure.text.Component.text("You do not have permission to use this command."); }
    @Override public boolean isPrimaryThread() { return Thread.currentThread() == primaryThread || Thread.holdsLock(Bootstrap.LEGACY_LOCK); }
    private final java.util.Map<String, org.bukkit.command.PluginCommand> commands = new java.util.HashMap<>();
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
    void unregisterCommands(org.bukkit.plugin.Plugin plugin) { commands.values().removeIf(command -> command.getPlugin() == plugin); }
    @Override public org.bukkit.command.PluginCommand getPluginCommand(String name) { return commands.get(name.toLowerCase(java.util.Locale.ROOT)); }
    @Override public boolean dispatchCommand(org.bukkit.command.CommandSender sender, String line) {
        String[] parts = line.trim().split("\\s+");
        var command = getPluginCommand(parts[0]);
        return command != null && command.execute(sender, parts[0], java.util.Arrays.copyOfRange(parts, 1, parts.length));
    }
    private final org.bukkit.command.ConsoleCommandSender console = createConsoleSender();
    @Override public org.bukkit.command.ConsoleCommandSender getConsoleSender() { return console; }
    private org.bukkit.command.ConsoleCommandSender createConsoleSender() {
        return (org.bukkit.command.ConsoleCommandSender) java.lang.reflect.Proxy.newProxyInstance(getClass().getClassLoader(), new Class<?>[]{org.bukkit.command.ConsoleCommandSender.class}, (proxy, method, args) -> {
            if (method.getName().equals("sendMessage") && args != null) { for (Object arg : args) if (arg instanceof String message) getLogger().info(message); return null; }
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
    private final FerritePluginManager plugins = new FerritePluginManager(this);
    @Override public org.bukkit.plugin.PluginManager getPluginManager() { return plugins; }
    @Override public String getName() { return "Ferrite"; }
    @Override public String getVersion() { return "Ferrite 0.1.0"; }
    @Override public String getBukkitVersion() { return "26.3"; }
    @Override public String getMinecraftVersion() { return "26.3"; }
    @Override public java.util.logging.Logger getLogger() { return java.util.logging.Logger.getLogger("Ferrite"); }
    @Override public java.util.Collection<? extends org.bukkit.entity.Player> getOnlinePlayers() { return java.util.List.copyOf(players.values()); }
}
