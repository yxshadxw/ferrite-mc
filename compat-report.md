# Plugin compatibility report

Each plugin is tested alone in an isolated process with a basic console command; optional integrations are not exercised.

## Vault

Process: exit code: 0

```text

running 1 test
10�� 28, 2026 11:36:50 ���� org.bukkit.Bukkit setServer
����: This server is running Ferrite version 0.1.0 (Implementing API version 26.3)
10�� 28, 2026 11:36:51 ���� net.milkbowl.vault.Vault loadPermission
����: [Permission] SuperPermissions loaded as backup permission system.
10�� 28, 2026 11:36:51 ���� net.milkbowl.vault.Vault onEnable
����: Enabled Version 1.7.3-b131
FERRITE_RESULT enabled=Ok(true)
10�� 28, 2026 11:36:51 ���� dev.ferrite.server.FerriteServer lambda$createConsoleSender$0
����: [Vault] Vault v1.7.3-b131 Information
10�� 28, 2026 11:36:51 ���� dev.ferrite.server.FerriteServer lambda$createConsoleSender$0
����: [Vault] Economy: None [null]
10�� 28, 2026 11:36:51 ���� dev.ferrite.server.FerriteServer lambda$createConsoleSender$0
����: [Vault] Permission: SuperPerms [SuperPerms]
10�� 28, 2026 11:36:51 ���� dev.ferrite.server.FerriteServer lambda$createConsoleSender$0
����: [Vault] Chat: None [null]
FERRITE_COMMAND vault-info: Ok(true)
FERRITE_LOG This server is running Ferrite version 0.1.0 (Implementing API version 26.3)
FERRITE_LOG [Permission] SuperPermissions loaded as backup permission system.
FERRITE_LOG Enabled Version 1.7.3-b131
FERRITE_LOG [Vault] Vault v1.7.3-b131 Information
FERRITE_LOG [Vault] Economy: None [null]
FERRITE_LOG [Vault] Permission: SuperPerms [SuperPerms]
FERRITE_LOG [Vault] Chat: None [null]
test compat_probe ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.40s


```

## LuckPerms

Process: exit code: 0

```text

running 1 test
10�� 28, 2026 11:36:51 ���� org.bukkit.Bukkit setServer
����: This server is running Ferrite version 0.1.0 (Implementing API version 26.3)
10�� 28, 2026 11:36:55 ���� me.lucko.luckperms.common.plugin.logging.JavaPluginLogger info
����: Loading configuration...
10�� 28, 2026 11:36:55 ���� dev.ferrite.server.FerriteServer lambda$createConsoleSender$0
����: ��b       ��3 __    
10�� 28, 2026 11:36:55 ���� dev.ferrite.server.FerriteServer lambda$createConsoleSender$0
����: ��b  |    ��3|__)   ��2LuckPerms��r ��bv5.5.88
10�� 28, 2026 11:36:55 ���� dev.ferrite.server.FerriteServer lambda$createConsoleSender$0
����: ��b  |___ ��3|      ��8Running on Bukkit - Ferrite
10�� 28, 2026 11:36:55 ���� dev.ferrite.server.FerriteServer lambda$createConsoleSender$0
����: 
10�� 28, 2026 11:36:56 ���� me.lucko.luckperms.common.plugin.logging.JavaPluginLogger info
����: Loading storage provider... [H2]
10�� 28, 2026 11:36:56 ���� me.lucko.luckperms.common.plugin.logging.JavaPluginLogger info
����: Loading internal permission managers...
10�� 28, 2026 11:36:56 ���� me.lucko.luckperms.common.plugin.logging.JavaPluginLogger info
����: Performing initial data load...
10�� 28, 2026 11:36:56 ���� me.lucko.luckperms.common.plugin.logging.JavaPluginLogger info
����: Successfully enabled. (took 1412ms)
FERRITE_RESULT enabled=Ok(true)
FERRITE_COMMAND luckperms: Ok(true)
FERRITE_LOG This server is running Ferrite version 0.1.0 (Implementing API version 26.3)
FERRITE_LOG Loading configuration...
FERRITE_LOG §b       §3 __    
FERRITE_LOG §b  |    §3|__)   §2LuckPerms§r §bv5.5.88
FERRITE_LOG §b  |___ §3|      §8Running on Bukkit - Ferrite
FERRITE_LOG 
FERRITE_LOG Loading storage provider... [H2]
FERRITE_LOG Loading internal permission managers...
FERRITE_LOG Performing initial data load...
FERRITE_LOG Successfully enabled. (took 1412ms)
10�� 28, 2026 11:36:56 ���� me.lucko.luckperms.common.plugin.logging.JavaPluginLogger info
����: Starting shutdown process...
10�� 28, 2026 11:36:56 ���� me.lucko.luckperms.common.plugin.logging.JavaPluginLogger info
����: Closing storage...
10�� 28, 2026 11:37:03 ���� me.lucko.luckperms.common.plugin.logging.JavaPluginLogger info
����: Goodbye!
test compat_probe ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 12.03s


```

## EssentialsX

Process: exit code: 0

```text

running 1 test
10�� 28, 2026 11:37:03 ���� org.bukkit.Bukkit setServer
����: This server is running Ferrite version 0.1.0 (Implementing API version 26.3)
10�� 28, 2026 11:37:04 ���� net.ess3.provider.providers.BaseLoggerProvider doTheLog
���: Beginning Adventure locale file conversion.
10�� 28, 2026 11:37:04 ���� net.ess3.provider.providers.BaseLoggerProvider doTheLog
�ɰ�: ��4You are running a server that does not properly support Bukkit plugins that rely on internal Mojang code. Consider using an Essentials replacement for your server software.
10�� 28, 2026 11:37:04 ���� net.ess3.provider.providers.BaseLoggerProvider doTheLog
����: ��c������ Ŭ���� ����: !net.minecraft.server.MinecraftServer
10�� 28, 2026 11:37:04 ���� net.ess3.provider.providers.BaseLoggerProvider doTheLog
����: ���ø��� �����Ͽ� ����ϴ� : C:\Users\taegy\AppData\Local\Temp\.tmpKkOKiw\Essentials\config.yml
10�� 28, 2026 11:37:04 ���� net.ess3.provider.providers.BaseLoggerProvider doTheLog
�ɰ�: java.lang.ExceptionInInitializerError
java.lang.ExceptionInInitializerError
	at com.earth2me.essentials.Settings._isWaterSafe(Settings.java:2076)
	at com.earth2me.essentials.Settings.reloadConfig(Settings.java:941)
	at com.earth2me.essentials.Settings.<init>(Settings.java:163)
	at com.earth2me.essentials.Essentials.onEnable(Essentials.java:272)
	at org.bukkit.plugin.java.JavaPlugin.setEnabled(JavaPlugin.java:280)
	at dev.ferrite.server.FerritePluginManager.loadOrdered(FerritePluginManager.java:119)
	at dev.ferrite.server.Bootstrap.start(Bootstrap.java:18)
Caused by: java.lang.UnsupportedOperationException: Ferrite: not implemented: Server#getUnsafe
	at dev.ferrite.server.ServerStubs.getUnsafe(ServerStubs.java:137)
	at dev.ferrite.server.FerriteServer.getUnsafe(FerriteServer.java:3)
	at org.bukkit.Bukkit.getUnsafe(Bukkit.java:2726)
	at org.bukkit.Material.lambda$new$1(Material.java:2947)
	at com.google.common.base.Suppliers$NonSerializableMemoizingSupplier.get(Suppliers.java:194)
	at org.bukkit.Material.asBlockType(Material.java:3825)
	at org.bukkit.Material.isBlock(Material.java:3152)
	at org.bukkit.Material.isTransparent(Material.java:3303)
	at com.earth2me.essentials.utils.LocationUtil.<clinit>(LocationUtil.java:50)
	... 7 more

FERRITE_RESULT failed: Java exception:
java.lang.IllegalStateException: Failed to load plugin
	at dev.ferrite.server.FerritePluginManager.loadOrdered(FerritePluginManager.java:120)
	at dev.ferrite.server.Bootstrap.start(Bootstrap.java:18)
Caused by: java.lang.NullPointerException: Cannot invoke "net.ess3.provider.ServerStateProvider.isStopping()" because the return value of "com.earth2me.essentials.Essentials.provider(java.lang.Class)" is null
	at com.earth2me.essentials.Essentials.onDisable(Essentials.java:540)
	at org.bukkit.plugin.java.JavaPlugin.setEnabled(JavaPlugin.java:286)
	at com.earth2me.essentials.Essentials.handleCrash(Essentials.java:1124)
	at com.earth2me.essentials.Essentials.onEnable(Essentials.java:463)
	at org.bukkit.plugin.java.JavaPlugin.setEnabled(JavaPlugin.java:280)
	at dev.ferrite.server.FerritePluginManager.loadOrdered(FerritePluginManager.java:119)
	... 1 more
test compat_probe ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.80s


```

## PlaceholderAPI

Process: exit code: 0

```text

running 1 test
10�� 28, 2026 11:37:04 ���� org.bukkit.Bukkit setServer
����: This server is running Ferrite version 0.1.0 (Implementing API version 26.3)
10�� 28, 2026 11:37:05 ���� me.clip.placeholderapi.expansion.manager.CloudExpansionManager fetch
����: Fetching available expansion information...
FERRITE_RESULT enabled=Ok(true)
10�� 28, 2026 11:37:05 ���� dev.ferrite.server.FerriteServer lambda$createConsoleSender$0
����: ��b��lPlaceholderAPI ��7(��f2.12.3��7)
��7Author: ��f[HelpChat]
��7PAPI Commands: ��b/papi ��fhelp
��7eCloud Commands��8: ��b/papi ��fecloud
FERRITE_COMMAND placeholderapi version: Ok(true)
FERRITE_LOG This server is running Ferrite version 0.1.0 (Implementing API version 26.3)
FERRITE_LOG Fetching available expansion information...
FERRITE_LOG §b§lPlaceholderAPI §7(§f2.12.3§7)
§7Author: §f[HelpChat]
§7PAPI Commands: §b/papi §fhelp
§7eCloud Commands§8: §b/papi §fecloud
test compat_probe ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 1.02s


```

