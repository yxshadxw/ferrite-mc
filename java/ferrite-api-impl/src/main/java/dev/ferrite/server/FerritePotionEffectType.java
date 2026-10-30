package dev.ferrite.server;
final class FerritePotionEffectType extends org.bukkit.potion.PotionEffectType {
    private final org.bukkit.NamespacedKey key;
    FerritePotionEffectType(org.bukkit.NamespacedKey key) { this.key = key; }
    public org.bukkit.NamespacedKey getKey() { return key; }
    public String getName() { return key.getKey().toUpperCase(java.util.Locale.ROOT); }
    public String translationKey() { return "effect." + key.getNamespace() + "." + key.getKey(); }
    public String getTranslationKey() { return translationKey(); }
    private UnsupportedOperationException unsupported() { return new UnsupportedOperationException("Ferrite: effect properties not implemented: " + key); }
    public org.bukkit.potion.PotionEffect createEffect(int duration, int amplifier) { return new org.bukkit.potion.PotionEffect(this, duration, amplifier); }
    public boolean isInstant() { throw unsupported(); }
    public org.bukkit.potion.PotionEffectTypeCategory getCategory() { throw unsupported(); }
    public org.bukkit.Color getColor() { throw unsupported(); }
    public double getDurationModifier() { throw unsupported(); }
    public int getId() { throw unsupported(); }
    public java.util.Map<org.bukkit.attribute.Attribute, org.bukkit.attribute.AttributeModifier> getEffectAttributes() { throw unsupported(); }
    public double getAttributeModifierAmount(org.bukkit.attribute.Attribute attribute, int amplifier) { throw unsupported(); }
    public org.bukkit.potion.PotionEffectType.Category getEffectCategory() { throw unsupported(); }
    public boolean isEnabledByFeature(org.bukkit.World world) { throw unsupported(); }
}
