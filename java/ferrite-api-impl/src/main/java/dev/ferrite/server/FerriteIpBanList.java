package dev.ferrite.server;

final class FerriteIpBanList implements org.bukkit.ban.IpBanList {
    private final java.util.Map<java.net.InetAddress, Entry> bans = new java.util.concurrent.ConcurrentHashMap<>();
    private static java.net.InetAddress address(String value) { return com.google.common.net.InetAddresses.forString(value); }
    void seed(String ip) { var address = address(ip); bans.put(address, new Entry(address, null, null, null)); }
    private Entry current(java.net.InetAddress address) { var entry = bans.get(address); if (entry != null && entry.expires != null && entry.expires.before(new java.util.Date())) { bans.remove(address, entry); return null; } return entry; }
    public org.bukkit.BanEntry<java.net.InetAddress> getBanEntry(java.net.InetAddress address) { return current(address); }
    @SuppressWarnings("unchecked") public <E extends org.bukkit.BanEntry<? super java.net.InetAddress>> E getBanEntry(String address) { return (E) current(address(address)); }
    @SuppressWarnings("unchecked") public <E extends org.bukkit.BanEntry<? super java.net.InetAddress>> E addBan(String address, String reason, java.util.Date expires, String source) { return (E) addBan(address(address), reason, expires, source); }
    public org.bukkit.BanEntry<java.net.InetAddress> addBan(java.net.InetAddress address, String reason, java.util.Date expires, String source) { var entry = new Entry(address, reason, expires, source); entry.save(); return entry; }
    public org.bukkit.BanEntry<java.net.InetAddress> addBan(java.net.InetAddress address, String reason, java.time.Instant expires, String source) { return addBan(address, reason, expires == null ? null : java.util.Date.from(expires), source); }
    public org.bukkit.BanEntry<java.net.InetAddress> addBan(java.net.InetAddress address, String reason, java.time.Duration duration, String source) { return addBan(address, reason, duration == null ? null : java.time.Instant.now().plus(duration), source); }
    @SuppressWarnings({"rawtypes", "unchecked"}) public java.util.Set<org.bukkit.BanEntry> getBanEntries() { return (java.util.Set) getEntries(); }
    @SuppressWarnings("unchecked") public <E extends org.bukkit.BanEntry<? super java.net.InetAddress>> java.util.Set<E> getEntries() { var result = new java.util.HashSet<E>(); for (var ip : bans.keySet()) { var entry = current(ip); if (entry != null) result.add((E) entry); } return java.util.Set.copyOf(result); }
    public boolean isBanned(java.net.InetAddress address) { return current(address) != null; }
    public boolean isBanned(String address) { return isBanned(address(address)); }
    public void pardon(java.net.InetAddress address) { if (!Native.serverBan(address.getHostAddress(), null, -1, null, false)) throw new IllegalStateException("No server command handler"); bans.remove(address); }
    public void pardon(String address) { pardon(address(address)); }
    java.util.Set<String> addresses() { var result = new java.util.HashSet<String>(); for (var ip : bans.keySet()) if (current(ip) != null) result.add(ip.getHostAddress()); return java.util.Set.copyOf(result); }
    private final class Entry implements org.bukkit.BanEntry<java.net.InetAddress> {
        private final java.net.InetAddress address;
        private java.util.Date created = new java.util.Date();
        private String reason, source;
        private java.util.Date expires;
        Entry(java.net.InetAddress address, String reason, java.util.Date expires, String source) { this.address = address; this.reason = reason; this.expires = expires == null ? null : new java.util.Date(expires.getTime()); this.source = source; }
        public String getTarget() { return address.getHostAddress(); }
        public java.net.InetAddress getBanTarget() { return address; }
        public java.util.Date getCreated() { return new java.util.Date(created.getTime()); }
        public void setCreated(java.util.Date value) { created = new java.util.Date(value.getTime()); }
        public String getSource() { return source; } public void setSource(String value) { source = value; }
        public String getReason() { return reason; } public void setReason(String value) { reason = value; }
        public java.util.Date getExpiration() { return expires == null ? null : new java.util.Date(expires.getTime()); }
        public void setExpiration(java.util.Date value) { expires = value == null ? null : new java.util.Date(value.getTime()); }
        public void save() { if (!Native.serverBan(address.getHostAddress(), reason, expires == null ? -1 : expires.getTime(), source, true)) throw new IllegalStateException("No server command handler"); bans.put(address, this); }
        public void remove() { pardon(address); }
    }
}
