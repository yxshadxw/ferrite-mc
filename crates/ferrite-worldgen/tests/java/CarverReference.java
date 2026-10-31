import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.registries.Registries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.data.worldgen.Carvers;
import net.minecraft.world.level.biome.Biomes;
import net.minecraft.world.level.biome.FixedBiomeSource;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.*;
import java.util.BitSet;

public class CarverReference {
  static class Mask implements CarverOutput {
    BitSet bits = new BitSet();
    public int minY() { return -63; }
    public int maxY() { return 312; }
    public void carve(int x,int y,int z) { if (y>=-64 && y<320) bits.set((y+64)*256+z*16+x); }
  }
  public static void main(String[] args) {
    SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
    var regs = VanillaRegistries.createWorldLookup();
    var settings = regs.lookupOrThrow(Registries.NOISE_SETTINGS).getOrThrow(NoiseGeneratorSettings.OVERWORLD);
    var biome = regs.lookupOrThrow(Registries.BIOME).getOrThrow(Biomes.PLAINS);
    var gen = new NoiseBasedChunkGenerator(new FixedBiomeSource(biome), settings);
    var context = new WorldGenerationContext(gen, LevelHeightAccessor.create(-64, 384));
    var carvers = regs.lookupOrThrow(Registries.CARVER);
    var list = new net.minecraft.world.level.levelgen.carver.WorldCarver[] {carvers.getOrThrow(Carvers.CAVE).value(), carvers.getOrThrow(Carvers.CAVE_EXTRA_UNDERGROUND).value(), carvers.getOrThrow(Carvers.CANYON).value()};
    int cx=Integer.parseInt(args[0]), cz=Integer.parseInt(args[1]);
    var mask = new Mask(); var target = new ChunkPos(cx,cz);
    var random = new WorldgenRandom(new LegacyRandomSource(0));
    for (int dx=-8; dx<=8; dx++) for (int dz=-8; dz<=8; dz++) {
      var source = new ChunkPos(cx+dx,cz+dz);
      for (int i=0;i<list.length;i++) {
        random.setLargeFeatureSeed(-540760265L+i, source.x(), source.z());
        if (list[i].isStartChunk(random)) list[i].carve(context,random,target,source,mask);
      }
    }
    long hash=0;
    for(int i=mask.bits.nextSetBit(0); i>=0; i=mask.bits.nextSetBit(i+1)) hash=hash*31+i;
    System.out.println("MASK " + mask.bits.cardinality() + " " + Long.toUnsignedString(hash));
  }
}
