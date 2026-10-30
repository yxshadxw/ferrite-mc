package dev.ferrite.test;
import org.bukkit.event.*;
import org.bukkit.plugin.java.JavaPlugin;
public final class TestPlugin extends JavaPlugin implements Listener {
    private void writeMarker(String name) {
        try {
            var folder = new java.io.File(getDataFolder().getParentFile(), "TestPlugin");
            java.nio.file.Files.createDirectories(folder.toPath());
            java.nio.file.Files.writeString(new java.io.File(folder, name).toPath(), "ok");
        } catch (java.io.IOException e) { throw new java.io.UncheckedIOException(e); }
    }
    private void appendEvent(String line) {
        try {
            var folder = new java.io.File(getDataFolder().getParentFile(), "TestPlugin");
            java.nio.file.Files.createDirectories(folder.toPath());
            java.nio.file.Files.writeString(new java.io.File(folder, "events.txt").toPath(), line + "\n", java.nio.charset.StandardCharsets.UTF_8, java.nio.file.StandardOpenOption.CREATE, java.nio.file.StandardOpenOption.APPEND);
        } catch (java.io.IOException e) { throw new java.io.UncheckedIOException(e); }
    }
    @EventHandler public void onJoin(org.bukkit.event.player.PlayerJoinEvent event) { appendEvent("join " + event.getPlayer().getName()); }
    @EventHandler public void onQuit(org.bukkit.event.player.PlayerQuitEvent event) { appendEvent("quit " + event.getPlayer().getName()); }
    @EventHandler public void onMove(org.bukkit.event.player.PlayerMoveEvent event) {
        appendEvent("move " + event.getPlayer().getName() + " " + event.getTo().getBlockX() + " " + event.getTo().getBlockY() + " " + event.getTo().getBlockZ());
        if (event.getTo().getY() > 90.0) { appendEvent("move-cancel " + event.getPlayer().getName()); event.setCancelled(true); }
    }
    @EventHandler public void onInteract(org.bukkit.event.player.PlayerInteractEvent event) {
        var clicked = event.getClickedBlock();
        appendEvent("interact " + (event.getAction() == org.bukkit.event.block.Action.LEFT_CLICK_BLOCK ? "left" : "right") + " " + clicked.getX() + " " + clicked.getY() + " " + clicked.getZ());
    }
    @EventHandler public void onChat(io.papermc.paper.event.player.AsyncChatEvent event) {
        String message = net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText().serialize(event.message());
        appendEvent("chat " + message);
        if (message.startsWith("cancel")) event.setCancelled(true);
    }
    @EventHandler public void onLegacyChat(org.bukkit.event.player.AsyncPlayerChatEvent event) {
        appendEvent("legacy-chat " + event.getMessage());
        if (event.getMessage().startsWith("cancel")) event.setCancelled(true);
    }
    @Override public void onEnable() {
        getLogger().info("FerriteTest enabled");
        getLogger().info("server name " + getServer().getName());
        getLogger().info("initial ip ban " + getServer().getIPBans().contains("192.0.2.7"));
        getLogger().info("FerriteTest settings " + getServer().getMotd() + " " + getServer().getMaxPlayers() + " " + getServer().getPort() + " " + getServer().getViewDistance() + " " + getServer().hasWhitelist());
        if (!getName().equals("FerriteLegacyTest")) getLogger().info(dev.ferrite.dependency.Helper.greeting());
        getCommand("testping").setExecutor((sender, command, label, args) -> { sender.sendMessage("pong"); return true; });
        getCommand("ferrite-test").setTabCompleter((sender, command, alias, args) -> java.util.List.of("hello"));
        getCommand("ferrite-test").setExecutor((sender, command, label, args) -> { if (sender instanceof org.bukkit.entity.Player player) { getLogger().info("offline snapshot " + getServer().getOfflinePlayer("KnownOffline").getUniqueId().equals(java.util.UUID.fromString("00000000-0000-0000-0000-00000000002c"))); getLogger().info("player snapshot " + player.getHealth() + " " + player.getFoodLevel() + " " + player.getGameMode()); var w = getServer().getWorld("probe_world"); if (w != null) { if (player.getInventory().getItem(0).getType() != org.bukkit.Material.STONE) throw new IllegalStateException("inventory snapshot"); player.getInventory().setItem(0, new org.bukkit.inventory.ItemStack(org.bukkit.Material.DIRT, 2)); getServer().setWhitelist(false); getServer().banIP("192.0.2.2"); player.displayName(net.kyori.adventure.text.Component.text("Display")); player.sendTitle("Title", "Subtitle", 1, 2, 3); player.playSound(player.getLocation(), "minecraft:entity.player.levelup", org.bukkit.SoundCategory.PLAYERS, 1.0f, 0.5f); player.kickPlayer("bye"); player.setOp(false); player.setHealth(10.0); player.setFoodLevel(9); player.setGameMode(org.bukkit.GameMode.ADVENTURE); if (player.getWorld() != w || !player.teleport(new org.bukkit.Location(w, 5, 70, 6))) throw new IllegalStateException("player routing"); if (w.getHighestBlockYAt(1, 2) != 64 || w.getChunkAt(0, 0).getBlock(1, 64, 2).getType() != org.bukkit.Material.STONE) throw new IllegalStateException("chunk snapshot"); var block = w.getBlockAt(1, 64, 2); getLogger().info("world snapshot " + w.getName() + " " + block.getType()); block.setType(org.bukkit.Material.DIRT); } } sender.sendMessage("hello " + String.join(" ", args)); dev.ferrite.server.Native.dispatchCommand((org.bukkit.command.CommandSender) java.lang.reflect.Proxy.newProxyInstance(getClass().getClassLoader(), new Class<?>[]{org.bukkit.command.ConsoleCommandSender.class}, (p, m, a) -> null), "spoof"); dev.ferrite.server.Native.dispatchCommand(sender, "bridge-command"); return true; });
        org.bukkit.World world = (org.bukkit.World) java.lang.reflect.Proxy.newProxyInstance(getClass().getClassLoader(), new Class<?>[]{org.bukkit.World.class}, (proxy, method, args) -> {
            if (method.getName().equals("getUID")) return java.util.UUID.fromString("00000000-0000-0000-0000-000000000001");
            throw new UnsupportedOperationException(method.getName());
        });
        getServer().getRegionScheduler().run(this, world, -4, 8, task -> { if (getServer().isOwnedByCurrentRegion(world, -4, 8) && getServer().isOwnedByCurrentRegion(world, io.papermc.paper.math.Position.block(-64, 0, 128)) && getServer().isOwnedByCurrentRegion(new org.bukkit.Location(world, -64, 0, 128)) && !getServer().isOwnedByCurrentRegion(world, -4, 8, 1)) getLogger().info("FerriteTest owned"); if (getServer().isOwnedByCurrentRegion(world, -4, 8, -3, 8)) getLogger().info("FerriteTest range owned"); getLogger().info("FerriteTest region"); });
        ((dev.ferrite.server.FerriteServer) getServer()).entityScheduler(java.util.UUID.fromString("00000000-0000-0000-0000-000000000002"), () -> true).run(this, task -> getLogger().info("FerriteTest entity"), null);
        getServer().getPluginManager().registerEvents(this, this);
        getServer().getPluginManager().callEvent(new TestEvent());
        getServer().getGlobalRegionScheduler().runAtFixedRate(this, task -> getLogger().info("FerriteTest global"), 1, 1);
        getServer().getScheduler().runTaskTimer(this, () -> getLogger().info("FerriteTest tick"), 1, 1);
        writeMarker("enabled.txt");
    }
    @EventHandler public void onTest(TestEvent event) { getLogger().info("FerriteTest event"); }
    @Override public void onDisable() { getLogger().info("FerriteTest disabled"); writeMarker("disabled.txt"); }
}
