package dev.ferrite.server;
import java.util.*;
import java.util.logging.*;
import org.bukkit.Bukkit;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;
class BootstrapTest {
    static final class ProbeEvent extends org.bukkit.event.Event {
        private static final org.bukkit.event.HandlerList HANDLERS = new org.bukkit.event.HandlerList();
        public org.bukkit.event.HandlerList getHandlers() { return HANDLERS; }
        public static org.bukkit.event.HandlerList getHandlerList() { return HANDLERS; }
    }
    static final class CancelEvent extends org.bukkit.event.Event implements org.bukkit.event.Cancellable {
        private static final org.bukkit.event.HandlerList HANDLERS = new org.bukkit.event.HandlerList();
        boolean cancelled;
        public boolean isCancelled() { return cancelled; }
        public void setCancelled(boolean value) { cancelled = value; }
        public org.bukkit.event.HandlerList getHandlers() { return HANDLERS; }
        public static org.bukkit.event.HandlerList getHandlerList() { return HANDLERS; }
    }
    static class ParentListener implements org.bukkit.event.Listener {
        final java.util.concurrent.atomic.AtomicInteger called = new java.util.concurrent.atomic.AtomicInteger();
        @org.bukkit.event.EventHandler public void inherited(CancelEvent event) { called.incrementAndGet(); }
    }
    static final class ChildListener extends ParentListener {}
    @Test void paperOnlyLoadsAndFailureRollsBack() throws Exception {
        var jar = java.nio.file.Files.createTempFile("ferrite-paper", ".jar");
        try (var input = new java.util.jar.JarFile(System.getProperty("test.dependency.jar")); var output = new java.util.jar.JarOutputStream(java.nio.file.Files.newOutputStream(jar))) {
            var entries = input.entries();
            while (entries.hasMoreElements()) {
                var entry = entries.nextElement();
                if (entry.getName().equals("plugin.yml")) continue;
                output.putNextEntry(new java.util.jar.JarEntry(entry.getName()));
                try (var in = input.getInputStream(entry)) { in.transferTo(output); }
                output.closeEntry();
            }
            output.putNextEntry(new java.util.jar.JarEntry("paper-plugin.yml"));
            output.write("name: PaperOnly\nversion: '1.0'\nmain: dev.ferrite.dependency.DependencyPlugin\napi-version: '26.3'\n".getBytes(java.nio.charset.StandardCharsets.UTF_8));
            output.closeEntry();
        }
        var manager = (FerritePluginManager) new FerriteServer().getPluginManager();
        manager.loadOrdered(new String[]{jar.toString()}, null);
        assertTrue(manager.isPluginEnabled("PaperOnly"));
        manager.disablePlugins();
        var failing = (FerritePluginManager) new FerriteServer().getPluginManager();
        assertThrows(IllegalStateException.class, () -> failing.loadOrdered(new String[]{jar.toString(), jar + ".missing"}, null));
        assertEquals(0, failing.getPlugins().length);
    }
    @Test void loadsRunsAndDisablesPlugin() {
        List<String> log = new ArrayList<>();
        Logger.getLogger("").addHandler(new Handler() {
            public void publish(LogRecord record) { log.add(record.getMessage()); }
            public void flush() {}
            public void close() {}
        });
        System.setProperty("ferrite.native", "false");
        Bootstrap.start(new String[]{System.getProperty("test.plugin.jar"), System.getProperty("test.dependency.jar"), System.getProperty("test.legacy.jar")});
        assertEquals("Ferrite", Bukkit.getServer().getName());
        assertNotNull(Bukkit.getServicesManager());
        assertTrue(Bukkit.isPrimaryThread());
        assertTrue(Bukkit.getPluginManager().isPluginEnabled("FerriteTest"));
        assertTrue(((FerritePluginManager) Bukkit.getPluginManager()).isLegacy(Bukkit.getPluginManager().getPlugin("FerriteLegacyTest")));
        assertThrows(ClassNotFoundException.class, () -> Bukkit.getPluginManager().getPlugin("FerriteLegacyTest").getClass().getClassLoader().loadClass("dev.ferrite.dependency.Helper"));
        assertSame(Bukkit.getPluginManager().getPlugin("FerriteDependency"), Bukkit.getPluginManager().getPlugin("FerriteAlias"));
        var order = new ArrayList<String>();
        var owner = Bukkit.getPluginManager().getPlugin("FerriteDependency");
        org.bukkit.event.Listener registered = new org.bukkit.event.Listener() {};
        Bukkit.getPluginManager().registerEvent(CancelEvent.class, registered, org.bukkit.event.EventPriority.HIGH, (listener, event) -> order.add("ignored"), owner, true);
        Bukkit.getPluginManager().registerEvent(CancelEvent.class, registered, org.bukkit.event.EventPriority.LOW, (listener, event) -> { order.add("first"); ((CancelEvent) event).setCancelled(true); }, owner, false);
        Bukkit.getPluginManager().registerEvent(CancelEvent.class, registered, org.bukkit.event.EventPriority.MONITOR, (listener, event) -> order.add("last"), owner, false);
        var child = new ChildListener();
        Bukkit.getPluginManager().registerEvents(child, owner);
        Bukkit.getPluginManager().registerEvent(CancelEvent.class, registered, org.bukkit.event.EventPriority.LOWEST, (listener, event) -> { throw new IllegalStateException("expected"); }, owner, false);
        Bukkit.getPluginManager().callEvent(new CancelEvent());
        assertEquals(1, child.called.get());
        assertEquals(List.of("first", "last"), order);
        var firstEntered = new java.util.concurrent.CountDownLatch(1);
        var release = new java.util.concurrent.CountDownLatch(1);
        var secondEntered = new java.util.concurrent.CountDownLatch(1);
        var count = new java.util.concurrent.atomic.AtomicInteger();
        org.bukkit.event.Listener listener = new org.bukkit.event.Listener() {
            @org.bukkit.event.EventHandler public void handle(ProbeEvent event) {
                if (count.incrementAndGet() == 1) { firstEntered.countDown(); try { release.await(); } catch (InterruptedException e) { Thread.currentThread().interrupt(); } }
                else secondEntered.countDown();
            }
        };
        Bukkit.getPluginManager().registerEvents(listener, Bukkit.getPluginManager().getPlugin("FerriteDependency"));
        Thread first = new Thread(() -> Bukkit.getPluginManager().callEvent(new ProbeEvent()));
        Thread second = new Thread(() -> Bukkit.getPluginManager().callEvent(new ProbeEvent()));
        first.start();
        try {
            assertTrue(firstEntered.await(5, java.util.concurrent.TimeUnit.SECONDS));
            second.start();
            assertFalse(secondEntered.await(200, java.util.concurrent.TimeUnit.MILLISECONDS));
        } catch (InterruptedException e) { throw new AssertionError(e); }
        finally { release.countDown(); try { first.join(); second.join(); } catch (InterruptedException e) { Thread.currentThread().interrupt(); } }
        assertEquals(2, count.get());
        assertTrue(log.contains("dependency visible"));
        var bukkitAsync = new java.util.concurrent.CountDownLatch(1);
        Bukkit.getScheduler().runTaskAsynchronously(Bukkit.getPluginManager().getPlugin("FerriteTest"), bukkitAsync::countDown);
        try { assertTrue(bukkitAsync.await(5, java.util.concurrent.TimeUnit.SECONDS)); } catch (InterruptedException e) { throw new AssertionError(e); }
        var asyncRan = new java.util.concurrent.CountDownLatch(1);
        Bukkit.getAsyncScheduler().runNow(Bukkit.getPluginManager().getPlugin("FerriteTest"), task -> asyncRan.countDown());
        try { assertTrue(asyncRan.await(5, java.util.concurrent.TimeUnit.SECONDS)); } catch (InterruptedException e) { throw new AssertionError(e); }
        assertTrue(log.contains("FerriteTest event"));
        UUID playerId = UUID.randomUUID();
        org.bukkit.entity.Player player = (org.bukkit.entity.Player) java.lang.reflect.Proxy.newProxyInstance(getClass().getClassLoader(), new Class<?>[]{org.bukkit.entity.Player.class}, (proxy, method, args) -> {
            if (method.getName().equals("getUniqueId")) return playerId;
            if (method.getName().equals("hasPermission") || method.getName().equals("isOp")) return false;
            if (method.getName().equals("sendMessage")) return null;
            if (method.getName().equals("getName")) return "player";
            throw new UnsupportedOperationException(method.getName());
        });
        ((FerriteServer) Bukkit.getServer()).registerPlayer(player);
        assertFalse(Bootstrap.dispatchCommand("unknown", "ferrite-test hello"));
        assertFalse(Native.dispatchCommand((org.bukkit.command.CommandSender) java.lang.reflect.Proxy.newProxyInstance(getClass().getClassLoader(), new Class<?>[]{org.bukkit.command.ConsoleCommandSender.class}, (proxy, method, args) -> null), "bridge-command"));
        Bootstrap.dispatchCommand(playerId.toString(), "ferrite-test hello");
        assertFalse(log.contains("hello hello"));
        assertNotNull(Bukkit.getPluginCommand("ft"));
        assertEquals(List.of("hello"), Bukkit.getPluginCommand("ft").tabComplete(Bukkit.getConsoleSender(), "ft", new String[]{"h"}));
        assertTrue(Bootstrap.dispatchCommand("console", "ft hello"));
        assertTrue(log.contains("hello hello"));
        var survivor = new java.util.concurrent.atomic.AtomicInteger();
        Bukkit.getScheduler().runTask(owner, () -> { throw new IllegalStateException("expected task failure"); });
        Bukkit.getScheduler().runTask(owner, survivor::incrementAndGet);
        var delayed = new java.util.concurrent.atomic.AtomicInteger();
        Bukkit.getScheduler().runTaskLater(Bukkit.getPluginManager().getPlugin("FerriteTest"), delayed::incrementAndGet, 2);
        Bootstrap.tick();
        assertEquals(0, delayed.get());
        assertEquals(1, survivor.get());
        Bootstrap.tick(); Bootstrap.tick();
        assertEquals(1, delayed.get());
        assertTrue(log.contains("FerriteTest region"));
        assertTrue(log.contains("FerriteTest owned"));
        assertTrue(log.contains("FerriteTest entity"));
        assertEquals(6, log.stream().filter("FerriteTest tick"::equals).count());
        assertEquals(6, log.stream().filter("FerriteTest global"::equals).count());
        Bootstrap.shutdown();
        assertFalse(Bukkit.getPluginManager().isPluginEnabled("FerriteTest"));
        assertTrue(log.contains("FerriteTest disabled"));
        Bootstrap.tick();
        assertTrue(log.contains("FerriteTest region"));
        assertTrue(log.contains("FerriteTest owned"));
        assertTrue(log.contains("FerriteTest entity"));
        assertEquals(6, log.stream().filter("FerriteTest tick"::equals).count());
        assertEquals(6, log.stream().filter("FerriteTest global"::equals).count());
    }
}
