import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.densityfunction.*;
import net.minecraft.world.level.block.Blocks;
import java.nio.file.*;
import java.util.Random;

public class AquiferReference {
 public static void main(String[] args) throws Exception {
  SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
  var regs=VanillaRegistries.createWorldLookup();
  var settings=regs.lookupOrThrow(Registries.NOISE_SETTINGS).getOrThrow(NoiseGeneratorSettings.OVERWORLD).value();
  var state=RandomState.create(regs.lookupOrThrow(Registries.NOISE), -540760265L, settings);
  var volume=new DensityVolume(16,384,16,0,-64,0);
  var lava=new Aquifer.FluidStatus(-54,Blocks.LAVA.defaultBlockState());
  var water=new Aquifer.FluidStatus(63,Blocks.WATER.defaultBlockState());
  var aq=settings.aquifers().orElseThrow().create(state.samplersWithContext(SamplerContext.EMPTY_UNCACHED),state.getOrCreateRandomFactory(Identifier.withDefaultNamespace("aquifer")),volume,(x,y,z)->y < -54 ? lava : water);
  var random=new Random(12345);
  try(var out=Files.newBufferedWriter(Path.of(args[0]))) {
    for(int i=0;i<2000;i++) {
      int x=random.nextInt(16), y=random.nextInt(384)-64,z=random.nextInt(16);
      double density=i%2==0?0.0:-0.1;
      var result=aq.computeSubstance(x,y,z,density);
      int code=result==null?0:result.isAir()?1:result.is(Blocks.WATER)?2:result.is(Blocks.LAVA)?3:4;
      out.write(x+"\t"+y+"\t"+z+"\t"+density+"\t"+code+"\n");
    }
  }
 }
}
