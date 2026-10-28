import com.mojang.datafixers.util.Pair;
import java.io.BufferedWriter;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Random;
import net.minecraft.SharedConstants;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderOwner;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.Climate;
import net.minecraft.world.level.biome.MultiNoiseBiomeSource;
import net.minecraft.world.level.biome.MultiNoiseBiomeSourceParameterList;

/**
 * Differential harness for ferrite-worldgen's multi-noise biome source.
 *
 * <p>Builds the real overworld {@link MultiNoiseBiomeSource}, then samples 20,000 random
 * {@link Climate.TargetPoint}s. Each row contains the six quantized target values plus the
 * biome chosen by the vanilla R-tree and by {@code findValueBruteForce}. The Rust test uses the
 * brute-force column as the tie-breaking reference and records R-tree divergences caused by
 * overlapping parameter ranges.
 *
 * <p>Run with the Java 25 toolchain and the Minecraft server jar plus its libraries on the
 * classpath. This file contains no Minecraft code or assets.
 */
public final class BiomeSourceDump {
    private static final int SAMPLE_COUNT = 20_000;
    private static final long SEED = 0x5EED_1234L;

    public static void main(String[] args) throws Exception {
        if (args.length != 1) {
            throw new IllegalArgumentException("usage: BiomeSourceDump <output.tsv>");
        }

        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();

        HolderOwner<Biome> owner = new HolderOwner<Biome>() {};
        var known =
                MultiNoiseBiomeSourceParameterList.knownPresets()
                        .get(MultiNoiseBiomeSourceParameterList.Preset.OVERWORLD);
        List<Pair<Climate.ParameterPoint, Holder<Biome>>> entries = new ArrayList<>();
        for (var pair : known.values()) {
            entries.add(
                    Pair.of(
                            pair.getFirst(),
                            Holder.Reference.createStandAlone(owner, pair.getSecond())));
        }
        var parameterList = new Climate.ParameterList<Holder<Biome>>(entries);
        var source = MultiNoiseBiomeSource.createFromList(parameterList);

        Random random = new Random(SEED);
        try (BufferedWriter writer = Files.newBufferedWriter(Path.of(args[0]))) {
            writer.write(
                    "#temperature\thumidity\tcontinentalness\terosion\tdepth\tweirdness"
                            + "\trtree\tbruteforce\n");
            for (int i = 0; i < SAMPLE_COUNT; i++) {
                float temperature = -2.0F + 4.0F * random.nextFloat();
                float humidity = -2.0F + 4.0F * random.nextFloat();
                float continentalness = -2.0F + 4.0F * random.nextFloat();
                float erosion = -2.0F + 4.0F * random.nextFloat();
                float depth = random.nextFloat();
                float weirdness = -2.0F + 4.0F * random.nextFloat();
                Climate.TargetPoint target =
                        Climate.target(
                                temperature,
                                humidity,
                                continentalness,
                                erosion,
                                depth,
                                weirdness);
                Holder<Biome> rtree = source.getNoiseBiome(target);
                Holder<Biome> bruteForce = parameterList.findValueBruteForce(target);
                writer.write(
                        target.temperature()
                                + "\t"
                                + target.humidity()
                                + "\t"
                                + target.continentalness()
                                + "\t"
                                + target.erosion()
                                + "\t"
                                + target.depth()
                                + "\t"
                                + target.weirdness()
                                + "\t"
                                + id(rtree)
                                + "\t"
                                + id(bruteForce)
                                + "\n");
            }
        }
    }

    private static String id(Holder<Biome> holder) {
        return holder.unwrapKey().orElseThrow().identifier().toString();
    }
}
