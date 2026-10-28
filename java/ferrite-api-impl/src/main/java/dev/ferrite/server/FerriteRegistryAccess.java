package dev.ferrite.server;

/** Registry identities extracted from locally generated vanilla data. */
public final class FerriteRegistryAccess implements io.papermc.paper.registry.RegistryAccess {
    private static final java.util.Map<String, java.util.Set<String>> KEYS = loadKeys();
    private static final java.util.Map<String, org.bukkit.Registry<?>> REGISTRIES = new java.util.concurrent.ConcurrentHashMap<>();
    private static org.bukkit.Registry<?> cached(String name, java.util.function.Supplier<org.bukkit.Registry<?>> factory) { return REGISTRIES.computeIfAbsent(name, ignored -> factory.get()); }
    private static java.util.Map<String, java.util.Set<String>> loadKeys() {
        var result = new java.util.HashMap<String, java.util.Set<String>>();
        try (var stream = java.util.Objects.requireNonNull(FerriteRegistryAccess.class.getResourceAsStream("/ferrite/registry-keys.tsv"), "Missing generated registry keys"); var reader = new java.io.BufferedReader(new java.io.InputStreamReader(stream, java.nio.charset.StandardCharsets.UTF_8))) {
            for (String line; (line = reader.readLine()) != null;) { String[] parts = line.split("\\t", 2); result.computeIfAbsent(parts[0], ignored -> new java.util.HashSet<>()).add(parts[1]); }
        } catch (java.io.IOException e) { throw new IllegalStateException("Cannot read registry keys", e); }
        return result;
    }
    private static boolean contains(String registry, org.bukkit.NamespacedKey key) { return KEYS.getOrDefault(registry, java.util.Set.of()).contains(key.toString()); }
    @SuppressWarnings("unchecked") @Override public <T extends org.bukkit.Keyed> org.bukkit.Registry<T> getRegistry(Class<T> type) {
        if (type == org.bukkit.enchantments.Enchantment.class) return (org.bukkit.Registry<T>) cached("enchantment", FerriteRegistryAccess::enchantmentRegistry);
        if (type == org.bukkit.potion.PotionEffectType.class) return (org.bukkit.Registry<T>) cached("mob_effect", FerriteRegistryAccess::potionRegistry);
        if (type == org.bukkit.block.Biome.class) return (org.bukkit.Registry<T>) cached("worldgen/biome", () -> identityRegistry("worldgen/biome", org.bukkit.block.Biome.class));
        return unavailable(type.getName());
    }
    @Override public <T extends org.bukkit.Keyed> org.bukkit.Registry<T> getRegistry(io.papermc.paper.registry.RegistryKey<T> type) {
        if (type.equals(io.papermc.paper.registry.RegistryKey.MENU)) return cast(cached("menu", FerriteRegistryAccess::menuRegistry));
        if (type.equals(io.papermc.paper.registry.RegistryKey.BLOCK)) return cast(cached("block", FerriteRegistryAccess::blockRegistry));
        if (type.equals(io.papermc.paper.registry.RegistryKey.ITEM)) return cast(cached("item", FerriteRegistryAccess::itemRegistry));
        if (type.equals(io.papermc.paper.registry.RegistryKey.ENTITY_TYPE)) return cast(cached("entity_type", FerriteRegistryAccess::entityRegistry));
        if (type.equals(io.papermc.paper.registry.RegistryKey.ENCHANTMENT)) return cast(cached("enchantment", FerriteRegistryAccess::enchantmentRegistry));
        if (type.equals(io.papermc.paper.registry.RegistryKey.MOB_EFFECT)) return cast(cached("mob_effect", FerriteRegistryAccess::potionRegistry));
        if (type.equals(io.papermc.paper.registry.RegistryKey.ATTRIBUTE)) return cast(cached("attribute", () -> identityRegistry("attribute", org.bukkit.attribute.Attribute.class)));
        if (type.equals(io.papermc.paper.registry.RegistryKey.SOUND_EVENT)) return cast(cached("sound_event", () -> identityRegistry("sound_event", org.bukkit.Sound.class)));
        if (type.equals(io.papermc.paper.registry.RegistryKey.BIOME)) return cast(cached("worldgen/biome", () -> identityRegistry("worldgen/biome", org.bukkit.block.Biome.class)));
        return unavailable(type.key().toString());
    }
    @SuppressWarnings("unchecked") private static <T extends org.bukkit.Keyed> org.bukkit.Registry<T> menuRegistry() {
        return (org.bukkit.Registry<T>) new BaseRegistry<org.bukkit.inventory.MenuType>("menu") {
            private final java.util.Map<org.bukkit.NamespacedKey, org.bukkit.inventory.MenuType> entries = new java.util.concurrent.ConcurrentHashMap<>();
            @Override public org.bukkit.inventory.MenuType get(org.bukkit.NamespacedKey key) {
                if (!contains("menu", key)) return null;
                return entries.computeIfAbsent(key, k -> (org.bukkit.inventory.MenuType) java.lang.reflect.Proxy.newProxyInstance(getClass().getClassLoader(), new Class<?>[]{org.bukkit.inventory.MenuType.Typed.class}, (proxy, method, args) -> switch (method.getName()) {
                    case "getKey" -> k;
                    case "key" -> k;
                    case "typed" -> proxy;
                    case "hashCode" -> k.hashCode();
                    case "equals" -> proxy == args[0];
                    case "toString" -> "MenuType[" + k + "]";
                    default -> throw new UnsupportedOperationException("Ferrite: not implemented: MenuType#" + method.getName());
                }));
            }
        };
    }
    @SuppressWarnings("unchecked") private static <T extends org.bukkit.Keyed> org.bukkit.Registry<T> itemRegistry() {
        return (org.bukkit.Registry<T>) new BaseRegistry<org.bukkit.inventory.ItemType>("item") {
            private final java.util.Map<org.bukkit.NamespacedKey, org.bukkit.inventory.ItemType> entries = new java.util.concurrent.ConcurrentHashMap<>();
            @Override public org.bukkit.inventory.ItemType get(org.bukkit.NamespacedKey key) {
                if (!contains("item", key)) return null;
                var material = org.bukkit.Material.getMaterial(key.getKey().toUpperCase(java.util.Locale.ROOT));
                if (material == null || material.isLegacy()) return null;
                return entries.computeIfAbsent(key, k -> (org.bukkit.inventory.ItemType) java.lang.reflect.Proxy.newProxyInstance(getClass().getClassLoader(), new Class<?>[]{org.bukkit.inventory.ItemType.Typed.class}, (proxy, method, args) -> switch (method.getName()) {
                    case "getKey", "key" -> k;
                    case "typed" -> proxy;
                    case "createItemStack" -> new FerriteItemStack(material, args == null || args.length == 0 ? 1 : (int) args[0]);
                    case "hashCode" -> k.hashCode();
                    case "equals" -> proxy == args[0];
                    case "toString" -> "ItemType[" + k + "]";
                    default -> throw new UnsupportedOperationException("Ferrite: not implemented: ItemType#" + method.getName());
                }));
            }
        };
    }
    private static org.bukkit.Registry<org.bukkit.block.BlockType> blockRegistry() {
        return new BaseRegistry<>("block") {
            private final java.util.Map<org.bukkit.NamespacedKey, org.bukkit.block.BlockType> entries = new java.util.concurrent.ConcurrentHashMap<>();
            @Override public org.bukkit.block.BlockType get(org.bukkit.NamespacedKey key) {
                if (!contains("block", key)) return null;
                var previous = entries.get(key); if (previous != null) return previous;
                var material = org.bukkit.Material.getMaterial(key.getKey().toUpperCase(java.util.Locale.ROOT));
                var value = (org.bukkit.block.BlockType) java.lang.reflect.Proxy.newProxyInstance(getClass().getClassLoader(), new Class<?>[]{org.bukkit.block.BlockType.Typed.class}, (proxy, method, args) -> switch (method.getName()) {
                    case "getKey", "key" -> key;
                    case "typed" -> proxy;
                    case "asMaterial" -> material;
                    case "isAir" -> key.getKey().equals("air") || key.getKey().equals("cave_air") || key.getKey().equals("void_air");
                    case "hasItemType" -> contains("item", key);
                    case "getItemType" -> org.bukkit.Registry.ITEM.get(key);
                    case "hashCode" -> key.hashCode();
                    case "equals" -> proxy == args[0];
                    case "toString" -> "BlockType[" + key + "]";
                    default -> throw new UnsupportedOperationException("Ferrite: not implemented: BlockType#" + method.getName());
                });
                previous = entries.putIfAbsent(key, value); return previous == null ? value : previous;
            }
        };
    }
    private static org.bukkit.Registry<org.bukkit.entity.EntityType> entityRegistry() {
        return new BaseRegistry<>("entity_type") {
            @Override public org.bukkit.entity.EntityType get(org.bukkit.NamespacedKey key) {
                if (!contains("entity_type", key)) return null;
                try { return org.bukkit.entity.EntityType.valueOf(key.getKey().toUpperCase(java.util.Locale.ROOT)); }
                catch (IllegalArgumentException e) { return null; }
            }
        };
    }
    private static org.bukkit.Registry<org.bukkit.enchantments.Enchantment> enchantmentRegistry() {
        return new BaseRegistry<>("enchantment") {
            private final java.util.Map<org.bukkit.NamespacedKey, org.bukkit.enchantments.Enchantment> entries = new java.util.concurrent.ConcurrentHashMap<>();
            @Override public org.bukkit.enchantments.Enchantment get(org.bukkit.NamespacedKey key) {
                if (!contains("enchantment", key)) return null;
                var value = entries.get(key); if (value != null) return value;
                value = new FerriteEnchantment(key); var prior = entries.putIfAbsent(key, value); return prior == null ? value : prior;
            }
        };
    }
    private static org.bukkit.Registry<org.bukkit.potion.PotionEffectType> potionRegistry() {
        return new BaseRegistry<>("mob_effect") {
            private final java.util.Map<org.bukkit.NamespacedKey, org.bukkit.potion.PotionEffectType> entries = new java.util.concurrent.ConcurrentHashMap<>();
            @Override public org.bukkit.potion.PotionEffectType get(org.bukkit.NamespacedKey key) {
                if (!contains("mob_effect", key)) return null;
                var value = entries.get(key); if (value != null) return value;
                value = new FerritePotionEffectType(key); var prior = entries.putIfAbsent(key, value); return prior == null ? value : prior;
            }
        };
    }
    @SuppressWarnings("unchecked") private static <T extends org.bukkit.Keyed> org.bukkit.Registry<T> cast(org.bukkit.Registry<?> registry) { return (org.bukkit.Registry<T>) registry; }
    static org.bukkit.block.Biome customBiome() {
        var key = org.bukkit.NamespacedKey.minecraft("custom");
        return (org.bukkit.block.Biome) java.lang.reflect.Proxy.newProxyInstance(FerriteRegistryAccess.class.getClassLoader(), new Class<?>[]{org.bukkit.block.Biome.class}, (proxy, method, args) -> switch (method.getName()) {
            case "getKey", "key" -> key;
            case "name", "getName" -> "CUSTOM";
            case "hashCode" -> System.identityHashCode(proxy);
            case "equals" -> proxy == args[0];
            case "toString" -> "CUSTOM";
            default -> throw new UnsupportedOperationException("Ferrite: not implemented: Biome#" + method.getName());
        });
    }
    private static <T extends org.bukkit.Keyed> org.bukkit.Registry<T> identityRegistry(String kind, Class<T> type) {
        return new BaseRegistry<T>(kind) {
            private final java.util.Map<org.bukkit.NamespacedKey,T> entries = new java.util.concurrent.ConcurrentHashMap<>();
            @Override public T get(org.bukkit.NamespacedKey key) {
                if (!contains(kind, key)) return null;
                T existing = entries.get(key);
                if (existing != null) return existing;
                var k = key;
                T value = type.cast(java.lang.reflect.Proxy.newProxyInstance(getClass().getClassLoader(), new Class<?>[]{type}, (proxy, method, args) -> switch (method.getName()) {
                    case "getKey", "key" -> k;
                    case "name", "getName" -> k.getKey().toUpperCase(java.util.Locale.ROOT).replace('.', '_');
                    case "translationKey", "getTranslationKey" -> kind + "." + k.getNamespace() + "." + k.getKey();
                    case "hashCode" -> k.hashCode();
                    case "equals" -> proxy == args[0];
                    case "toString" -> k.toString();
                    default -> throw new UnsupportedOperationException("Ferrite: not implemented: " + type.getSimpleName() + "#" + method.getName());
                }));
                T previous = entries.putIfAbsent(key, value);
                return previous == null ? value : previous;
            }
            @Override public int size() { return KEYS.get(kind).size(); }
            @Override public java.util.stream.Stream<org.bukkit.NamespacedKey> keyStream() { return KEYS.get(kind).stream().map(org.bukkit.NamespacedKey::fromString); }
            @Override public java.util.stream.Stream<T> stream() { return keyStream().map(this::get); }
            @Override public java.util.Iterator<T> iterator() { return stream().iterator(); }
        };
    }
    private static <T extends org.bukkit.Keyed> org.bukkit.Registry<T> unavailable(String name) { return new BaseRegistry<T>(name) {}; }
    private abstract static class BaseRegistry<T extends org.bukkit.Keyed> implements org.bukkit.Registry<T> {
        private final String name;
        BaseRegistry(String name) { this.name = name; }
        private UnsupportedOperationException unavailable() { return new UnsupportedOperationException("Ferrite: registry data not implemented: " + name); }
        public T get(org.bukkit.NamespacedKey key) { throw unavailable(); }
        public org.bukkit.NamespacedKey getKey(T value) { return value.getKey(); }
        public boolean hasTag(io.papermc.paper.registry.tag.TagKey<T> key) { throw unavailable(); }
        public io.papermc.paper.registry.tag.Tag<T> getTag(io.papermc.paper.registry.tag.TagKey<T> key) { throw unavailable(); }
        public java.util.Collection<io.papermc.paper.registry.tag.Tag<T>> getTags() { throw unavailable(); }
        public java.util.stream.Stream<T> stream() { throw unavailable(); }
        public java.util.stream.Stream<org.bukkit.NamespacedKey> keyStream() { throw unavailable(); }
        public int size() { throw unavailable(); }
        public java.util.Iterator<T> iterator() { throw unavailable(); }
    }
}
