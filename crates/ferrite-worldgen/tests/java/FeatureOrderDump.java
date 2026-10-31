import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.Registries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.biome.MultiNoiseBiomeSource;
import net.minecraft.world.level.biome.MultiNoiseBiomeSourceParameterLists;
import net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator;
import net.minecraft.world.level.levelgen.NoiseGeneratorSettings;
import net.minecraft.world.level.levelgen.WorldgenRandom;
import net.minecraft.world.level.levelgen.XoroshiroRandomSource;

/**
 * Dumps the 26.3 feature-sorter order and the decoration/feature seeds used by
 * {@code ChunkGenerator.applyBiomeDecoration}, so the Rust decoration pipeline can use the
 * same global feature indices. This file contains no Minecraft code or assets.
 */
public final class FeatureOrderDump {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        var registries = VanillaRegistries.createWorldLookup();
        var settings = registries.lookupOrThrow(Registries.NOISE_SETTINGS)
                .getOrThrow(NoiseGeneratorSettings.OVERWORLD);
        var preset = registries.lookupOrThrow(Registries.MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST)
                .getOrThrow(MultiNoiseBiomeSourceParameterLists.OVERWORLD);
        var source = MultiNoiseBiomeSource.createFromPreset(preset);
        var generator = new NoiseBasedChunkGenerator(source, settings);
        var placedLookup = registries.lookupOrThrow(Registries.PLACED_FEATURE);
        var names = new java.util.IdentityHashMap<Object, String>();
        placedLookup.listElements().forEach(holder -> names.put(
                holder.value(), holder.key().identifier().toString()));

        var field = net.minecraft.world.level.chunk.ChunkGenerator.class
                .getDeclaredField("featuresPerStep");
        field.setAccessible(true);
        @SuppressWarnings("unchecked")
        var supplier = (java.util.function.Supplier<List<?>>) field.get(generator);
        var steps = supplier.get();

        long seed = -540760265L;
        var random = new WorldgenRandom(new XoroshiroRandomSource(0L));
        long decoration = random.setDecorationSeed(seed, 0, 0);

        try (var out = Files.newBufferedWriter(Path.of(args[0]))) {
            out.write("#seed\t" + seed + "\n");
            out.write("#chunk\t0\t0\tdecoration\t" + decoration + "\n");
            for (var holder : source.possibleBiomes()) {
                out.write("#biome\t" + holder.unwrapKey().orElseThrow().identifier() + "\n");
            }
            for (int step = 0; step < steps.size(); step++) {
                var data = steps.get(step);
                var method = data.getClass().getDeclaredMethod("features");
                method.setAccessible(true);
                @SuppressWarnings("unchecked")
                var features = (List<Object>) method.invoke(data);
                for (int index = 0; index < features.size(); index++) {
                    random.setFeatureSeed(decoration, index, step);
                    out.write("#feature_seed\t" + step + "\t" + index + "\t"
                            + random.nextLong() + "\n");
                    out.write(step + "\t" + index + "\t"
                            + names.getOrDefault(features.get(index), "<unknown>") + "\n");
                }
            }
        }
    }
}
