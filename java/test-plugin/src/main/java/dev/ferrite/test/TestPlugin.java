package dev.ferrite.test;
import org.bukkit.event.*;
import org.bukkit.plugin.java.JavaPlugin;
public final class TestPlugin extends JavaPlugin implements Listener {
    @Override public void onEnable() {
        getLogger().info("FerriteTest enabled");
        if (!getName().equals("FerriteLegacyTest")) getLogger().info(dev.ferrite.dependency.Helper.greeting());
        getCommand("ferrite-test").setTabCompleter((sender, command, alias, args) -> java.util.List.of("hello"));
        getCommand("ferrite-test").setExecutor((sender, command, label, args) -> { sender.sendMessage("hello " + String.join(" ", args)); dev.ferrite.server.Native.dispatchCommand((org.bukkit.command.CommandSender) java.lang.reflect.Proxy.newProxyInstance(getClass().getClassLoader(), new Class<?>[]{org.bukkit.command.ConsoleCommandSender.class}, (p, m, a) -> null), "spoof"); dev.ferrite.server.Native.dispatchCommand(sender, "bridge-command"); return true; });
        org.bukkit.World world = (org.bukkit.World) java.lang.reflect.Proxy.newProxyInstance(getClass().getClassLoader(), new Class<?>[]{org.bukkit.World.class}, (proxy, method, args) -> {
            if (method.getName().equals("getUID")) return java.util.UUID.fromString("00000000-0000-0000-0000-000000000001");
            throw new UnsupportedOperationException(method.getName());
        });
        getServer().getRegionScheduler().run(this, world, -4, 8, task -> { if (getServer().isOwnedByCurrentRegion(world, -4, 8) && getServer().isOwnedByCurrentRegion(world, io.papermc.paper.math.Position.block(-64, 0, 128)) && getServer().isOwnedByCurrentRegion(new org.bukkit.Location(world, -64, 0, 128)) && !getServer().isOwnedByCurrentRegion(world, -4, 8, 1)) getLogger().info("FerriteTest owned"); getLogger().info("FerriteTest region"); });
        ((dev.ferrite.server.FerriteServer) getServer()).entityScheduler(java.util.UUID.fromString("00000000-0000-0000-0000-000000000002"), () -> true).run(this, task -> getLogger().info("FerriteTest entity"), null);
        getServer().getPluginManager().registerEvents(this, this);
        getServer().getPluginManager().callEvent(new TestEvent());
        getServer().getGlobalRegionScheduler().runAtFixedRate(this, task -> getLogger().info("FerriteTest global"), 1, 1);
        getServer().getScheduler().runTaskTimer(this, () -> getLogger().info("FerriteTest tick"), 1, 1);
    }
    @EventHandler public void onTest(TestEvent event) { getLogger().info("FerriteTest event"); }
    @Override public void onDisable() { getLogger().info("FerriteTest disabled"); }
}
