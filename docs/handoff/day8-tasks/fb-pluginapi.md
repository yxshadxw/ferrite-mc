# Freebuff task (Bukkit API breadth, second pass)

Base `63b5695`. oc14 already widened `Server`/`PluginManager`, added inventory/death/chunk/weather
events, and `Native.playerInventory` is wired. The gap list is in the oc14 commit and
`docs/BACKLOG.md` ("Plugins (embedded JVM)"). Read `docs/handoff/day8-tasks/COMMON-RULES.md`.

Area: `java/ferrite-api-impl`, `java/test-plugin-typical`, `crates/ferrite-plugin`, and the
`ServerHooks` route in `crates/ferrite-server`. Do not touch worldgen, pistons, command blocks,
containers, dimensions or `ferrite-command`.

Implement, highest value first, each with a test in `java/test-plugin-typical` asserting the real
value (not just "does not throw"):

1. `Server` methods that are still stubs and cheap to back with real data: `getRecipesFor`/
   `recipeIterator` (1,763 recipes already load in `ferrite-recipe`), `getBossBars`/`getBossBar`,
   `getTags`, `getHelpMap`, `getWorldContainer`, `getUpdateFolder`, `getIp`/`getPort`,
   `getMaxPlayers`/`getViewDistance`/`getSimulationDistance`.
2. World/block access: `World#getBlockAt` for arbitrary coordinates via an on-demand block snapshot
   (remove the `setBlockSnapshot` opt-in), `Block#getType`/`getBlockData`/`getLightFromSky`,
   `World#getHighestBlockYAt`, `World#getBiome`.
3. `Player`/entity: health/food/experience read+write back into the region player, `Player#teleport`
   through the existing mailbox, `Entity#getType`/`getUniqueId`/`getLocation` for tracked mobs.
4. Scheduler: `BukkitRunnable#runTaskTimerAsynchronously` cancellation from another thread, and
   plugin disable cancelling all of that plugin's tasks (assert no task runs after disable).

Keep the known-unfixable list (NMS-typed `CraftItemStack.asNMSCopy`, `CraftWorld#getHandle`,
`CraftMagicNumbers`) documented in `java/ferrite-api-impl/README` or the compat notes; do not fake
them with `NoSuchMethodError`-hiding stubs.
