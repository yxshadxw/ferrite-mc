import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Random;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.world.level.levelgen.NoiseGeneratorSettings;
import net.minecraft.world.level.levelgen.RandomState;
import net.minecraft.world.level.levelgen.densityfunction.SamplerContext;
import net.minecraft.world.level.biome.BiomeManager;
import net.minecraft.world.level.biome.MultiNoiseBiomeSource;
import net.minecraft.world.level.biome.MultiNoiseBiomeSourceParameterLists;

/** Dumps actual biome-manager selections for the 26.3 overworld. */
public final class SurfaceBiomeDump {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var registries = VanillaRegistries.createWorldLookup();
        var settings = registries.lookupOrThrow(Registries.NOISE_SETTINGS).getOrThrow(NoiseGeneratorSettings.OVERWORLD).value();
        long seed = -540760265L;
        var state = RandomState.create(registries.lookupOrThrow(Registries.NOISE), seed, settings);
        var preset = registries.lookupOrThrow(Registries.MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST).getOrThrow(MultiNoiseBiomeSourceParameterLists.OVERWORLD);
        var source = MultiNoiseBiomeSource.createFromPreset(preset);
        var resolver = source.createResolver(state.createClimateSampler(SamplerContext.EMPTY_UNCACHED));
        int[] q = new int[3];
        var manager = new BiomeManager((x,y,z)->{ q[0]=x; q[1]=y; q[2]=z; return resolver.getNoiseBiome(x,y,z); }, BiomeManager.obfuscateSeed(seed));
        var random = new Random(91723);
        try (var out = Files.newBufferedWriter(Path.of(args[0]))) {
            for (int i=0; i<2000; i++) {
                int x=random.nextInt(8192)-4096, y=random.nextInt(160)-32, z=random.nextInt(8192)-4096;
                var biome = manager.getBiome(x,y,z).unwrapKey().orElseThrow().identifier();
                out.write(x+"\t"+y+"\t"+z+"\t"+biome+"\t"+q[0]+"\t"+q[1]+"\t"+q[2]+"\n");
            }
        }
    }
}
