package dev.ferrite.server;

/** Minimal help map: plugin commands become topics, other topics are added by plugins. */
final class FerriteHelpMap implements org.bukkit.help.HelpMap {
    private final java.util.Map<String, org.bukkit.help.HelpTopic> topics = new java.util.concurrent.ConcurrentHashMap<>();
    private final java.util.List<String> ignored = new java.util.ArrayList<>();
    private final FerriteServer server;

    FerriteHelpMap(FerriteServer server) { this.server = server; }

    @Override public org.bukkit.help.HelpTopic getHelpTopic(String name) {
        java.util.Objects.requireNonNull(name, "name");
        var topic = topics.get(name.toLowerCase(java.util.Locale.ROOT));
        if (topic != null) return topic;
        var command = server.getCommandMap().getCommand(name);
        return command == null ? null : new org.bukkit.help.GenericCommandHelpTopic(command);
    }
    @Override public java.util.Collection<org.bukkit.help.HelpTopic> getHelpTopics() { return java.util.List.copyOf(topics.values()); }
    @Override public void addTopic(org.bukkit.help.HelpTopic topic) { topics.put(topic.getName().toLowerCase(java.util.Locale.ROOT), topic); }
    @Override public void clear() { topics.clear(); }
    @Override public void registerHelpTopicFactory(Class<?> commandClass, org.bukkit.help.HelpTopicFactory<?> factory) {}
    @Override public java.util.List<String> getIgnoredPlugins() { return java.util.List.copyOf(ignored); }
}
