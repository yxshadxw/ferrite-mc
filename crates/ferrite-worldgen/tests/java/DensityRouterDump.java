import java.io.BufferedWriter;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Random;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.registries.Registries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.world.level.levelgen.NoiseGeneratorSettings;
import net.minecraft.world.level.levelgen.RandomState;
import net.minecraft.world.level.levelgen.densityfunction.DensityFunction;

/** Dumps sampled 26.3 overworld router values for the Rust differential test. */
public final class DensityRouterDump {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        var registries = VanillaRegistries.createWorldLookup();
        var settings = registries.lookupOrThrow(Registries.NOISE_SETTINGS)
                .getOrThrow(NoiseGeneratorSettings.OVERWORLD).value();
        var state = RandomState.create(registries.lookupOrThrow(Registries.NOISE), 123456789L, settings);
        var router = settings.noiseRouter();
        DensityFunction[] functions = {router.finalDensity(), router.continents(), router.erosion(),
                router.depth(), router.ridges(), router.temperature(), router.vegetation(), router.chunkSurfaceLevel()};
        var random = new Random(12345L);
        try (BufferedWriter out = Files.newBufferedWriter(Path.of(args[0]))) {
            for (int i = 0; i < 2000; i++) {
                int x = random.nextInt(8192) - 4096;
                int y = random.nextInt(384) - 64;
                int z = random.nextInt(8192) - 4096;
                out.write(x + "\t" + y + "\t" + z);
                for (var function : functions) {
                    out.write("\t" + Double.toString((double) state.sampleBlockValueUncached(function, x, y, z)));
                }
                out.newLine();
            }
        }
    }
}
