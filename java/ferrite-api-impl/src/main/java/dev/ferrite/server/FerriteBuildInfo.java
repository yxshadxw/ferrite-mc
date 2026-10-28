package dev.ferrite.server;

import io.papermc.paper.ServerBuildInfo;
import java.time.Instant;
import java.util.Optional;
import java.util.OptionalInt;
import net.kyori.adventure.key.Key;

public final class FerriteBuildInfo implements ServerBuildInfo {
    public Key brandId() { return Key.key("ferrite", "ferrite"); }
    public boolean isBrandCompatible(Key key) { return brandId().equals(key); }
    public String brandName() { return "Ferrite"; }
    public String minecraftVersionId() { return "26.3"; }
    public String minecraftVersionName() { return "26.3"; }
    public OptionalInt buildNumber() { return OptionalInt.empty(); }
    public Instant buildTime() { return Instant.EPOCH; }
    public Optional<String> gitBranch() { return Optional.empty(); }
    public Optional<String> gitCommit() { return Optional.empty(); }
    public String asString(StringRepresentation representation) { return "0.1.0"; }
}
