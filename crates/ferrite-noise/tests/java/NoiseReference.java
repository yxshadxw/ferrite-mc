import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.function.BooleanSupplier;
import java.util.function.DoubleSupplier;
import java.util.function.IntSupplier;
import java.util.function.LongSupplier;

import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.levelgen.PositionalRandomFactory;
import net.minecraft.world.level.levelgen.RandomSupport;
import net.minecraft.world.level.levelgen.XoroshiroRandomSource;
import net.minecraft.world.level.levelgen.densityfunction.DensitySampler;
import net.minecraft.world.level.levelgen.synth.BlendedNoise;
import net.minecraft.world.level.levelgen.synth.Noise;
import net.minecraft.world.level.levelgen.synth.NormalNoise;
import net.minecraft.world.level.levelgen.synth.PerlinNoise;
import net.minecraft.world.level.levelgen.synth.SimplexNoise;
import net.minecraft.world.level.levelgen.synth.SmearedPerlinNoise;

/**
 * Dumps reference values for ferrite-noise from the local 26.3 server jar.
 *
 * <p>Usage: {@code java NoiseReference <output.json>}. The output is read by
 * {@code crates/ferrite-noise/tests/reference.rs}; nothing here is committed.
 *
 * <p>Run this with {@code -XX:+UnlockDiagnosticVMOptions -XX:-UseLibmIntrinsic}:
 * the JVM is allowed to replace {@code Math.log} with a platform libm intrinsic
 * that differs from {@code StrictMath.log} in the last bit, which would make
 * {@code nextGaussian()} depend on the host libm. Disabling that intrinsic pins
 * the reference to fdlibm, the deterministic behaviour of the Java spec.
 */
public final class NoiseReference {
    private static final int SAMPLES = 10_000;
    private static final long LEGACY_SEED = 0x123456789ABCDEFL;
    private static final long XOROSHIRO_SEED = 0x0FEDCBA987654321L;

    private interface FloatSupplier {
        float getAsFloat();
    }

    private interface SectionWriter {
        void write(StringBuilder out);
    }

    private NoiseReference() {}

    public static void main(String[] args) throws IOException {
        if (args.length != 1) {
            System.err.println("usage: NoiseReference <output.json>");
            System.exit(2);
        }

        // BlendedNoise implements DensityFunction, whose static initialiser needs
        // the vanilla registries to be bootstrapped, and bootstrapping needs a
        // detected game version.
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();

        StringBuilder out = new StringBuilder(8 << 20);
        out.append('{');
        boolean[] firstSection = {true};
        section(out, firstSection, "meta", "\"samples\":" + SAMPLES + ",\"minecraft\":\"26.3\"");

        // ---- random sources -------------------------------------------------
        LegacyRandomSource legacyInt = new LegacyRandomSource(LEGACY_SEED);
        LegacyRandomSource legacyBound = new LegacyRandomSource(LEGACY_SEED);
        LegacyRandomSource legacyLong = new LegacyRandomSource(LEGACY_SEED);
        LegacyRandomSource legacyDouble = new LegacyRandomSource(LEGACY_SEED);
        LegacyRandomSource legacyFloat = new LegacyRandomSource(LEGACY_SEED);
        LegacyRandomSource legacyGaussian = new LegacyRandomSource(LEGACY_SEED);
        LegacyRandomSource legacyBoolean = new LegacyRandomSource(LEGACY_SEED);
        section(out, firstSection, "legacy_random_sequences",
                intsField("next_int", legacyInt::nextInt, SAMPLES),
                intsField("next_int_bound_1000", () -> legacyBound.nextInt(1000), SAMPLES),
                longsField("next_long", legacyLong::nextLong, SAMPLES),
                doublesField("next_double", legacyDouble::nextDouble, SAMPLES),
                floatsField("next_float", legacyFloat::nextFloat, SAMPLES),
                doublesField("next_gaussian", legacyGaussian::nextGaussian, SAMPLES),
                booleansField("next_boolean", legacyBoolean::nextBoolean, SAMPLES));

        XoroshiroRandomSource xoroInt = new XoroshiroRandomSource(XOROSHIRO_SEED);
        XoroshiroRandomSource xoroBound = new XoroshiroRandomSource(XOROSHIRO_SEED);
        XoroshiroRandomSource xoroLong = new XoroshiroRandomSource(XOROSHIRO_SEED);
        XoroshiroRandomSource xoroDouble = new XoroshiroRandomSource(XOROSHIRO_SEED);
        XoroshiroRandomSource xoroFloat = new XoroshiroRandomSource(XOROSHIRO_SEED);
        XoroshiroRandomSource xoroGaussian = new XoroshiroRandomSource(XOROSHIRO_SEED);
        XoroshiroRandomSource xoroBoolean = new XoroshiroRandomSource(XOROSHIRO_SEED);
        section(out, firstSection, "xoroshiro_random_sequences",
                intsField("next_int", xoroInt::nextInt, SAMPLES),
                intsField("next_int_bound_1000", () -> xoroBound.nextInt(1000), SAMPLES),
                longsField("next_long", xoroLong::nextLong, SAMPLES),
                doublesField("next_double", xoroDouble::nextDouble, SAMPLES),
                floatsField("next_float", xoroFloat::nextFloat, SAMPLES),
                doublesField("next_gaussian", xoroGaussian::nextGaussian, SAMPLES),
                booleansField("next_boolean", xoroBoolean::nextBoolean, SAMPLES));

        // ---- positional factories ------------------------------------------
        PositionalRandomFactory legacyFactory =
                new LegacyRandomSource.LegacyPositionalRandomFactory(0x0BADF00DCAFEBABEL);
        PositionalRandomFactory xoroFactory =
                new XoroshiroRandomSource.XoroshiroPositionalRandomFactory(
                        0x0123456789ABCDEFL, 0x0FEDCBA987654321L);
        section(out, firstSection, "legacy_positional",
                doublesField("at", () -> legacyFactory.at(posX(0), posY(0), posZ(0)).nextDouble(), 1));
        section(out, firstSection, "positional_legacy",
                positionsField("at", legacyFactory),
                hashesField("from_hash_of", legacyFactory),
                seedsField("from_seed", legacyFactory));
        section(out, firstSection, "positional_xoroshiro",
                positionsField("at", xoroFactory),
                hashesField("from_hash_of", xoroFactory),
                seedsField("from_seed", xoroFactory));

        // ---- noise primitives ----------------------------------------------
        PerlinNoise perlin = new PerlinNoise(new LegacyRandomSource(1234L));
        section(out, firstSection, "perlin",
                noise3dField("samples", perlin));

        SmearedPerlinNoise smeared = new SmearedPerlinNoise(new LegacyRandomSource(5678L), 1.5);
        section(out, firstSection, "smeared_perlin",
                noise3dField("samples", smeared));

        SimplexNoise simplex = new SimplexNoise(new LegacyRandomSource(9012L));
        section(out, firstSection, "simplex",
                noise2dField("samples_2d", simplex),
                noise3dField("samples_3d", simplex));

        NormalNoise normal = NormalNoise.builder()
                .setBaseAmplitude(1.0)
                .setBaseOctave(-8)
                .setOctaveCount(5)
                .setNormalize(true)
                .setAmplitudeModifier(1, 0.5)
                .setAmplitudeModifier(3, 0.0)
                .build();
        Noise normalNoise = normal.create(new XoroshiroRandomSource(3456L));
        section(out, firstSection, "normal_noise",
                noise3dField("samples", normalNoise));

        NormalNoise legacyNormal = NormalNoise.builder()
                .setBaseAmplitude(1.0)
                .setBaseOctave(-6)
                .setOctaveCount(4)
                .setLegacyNormalization()
                .build();
        Noise legacyNormalNoise = legacyNormal.createForLegacyNetherBiome(new LegacyRandomSource(7890L));
        section(out, firstSection, "legacy_normal_noise",
                noise3dField("samples", legacyNormalNoise));

        BlendedNoise blended = new BlendedNoise(1.0, 1.0, 80.0, 160.0, 8.0);
        DensitySampler blendedSampler = blended.compileSampler(new XoroshiroRandomSource(2468L));
        section(out, firstSection, "blended_noise",
                blendedField(blendedSampler));

        section(out, firstSection, "random_support",
                longsField("upgrade_unmixed_lo", () -> RandomSupport.upgradeSeedTo128bitUnmixed(LEGACY_SEED).seedLo(), 1),
                longsField("upgrade_unmixed_hi", () -> RandomSupport.upgradeSeedTo128bitUnmixed(LEGACY_SEED).seedHi(), 1),
                longsField("upgrade_lo", () -> RandomSupport.upgradeSeedTo128bit(LEGACY_SEED).seedLo(), 1),
                longsField("upgrade_hi", () -> RandomSupport.upgradeSeedTo128bit(LEGACY_SEED).seedHi(), 1),
                longsField("hash_lo", () -> RandomSupport.seedFromHashOf("minecraft:overworld").seedLo(), 1),
                longsField("hash_hi", () -> RandomSupport.seedFromHashOf("minecraft:overworld").seedHi(), 1),
                longsField("mix_stafford_13", () -> RandomSupport.mixStafford13(LEGACY_SEED), 1));

        out.append('}');
        Path output = Path.of(args[0]);
        if (output.getParent() != null) {
            Files.createDirectories(output.getParent());
        }
        Files.writeString(output, out.toString(), StandardCharsets.UTF_8);
        System.out.println("wrote " + output.toAbsolutePath() + " (" + out.length() + " chars)");
    }

    // ---- section helpers ---------------------------------------------------

    private static void section(StringBuilder out, boolean[] first, String name, String... fields) {
        if (!first[0]) {
            out.append(',');
        }
        first[0] = false;
        out.append('"').append(name).append("\":{");
        for (int i = 0; i < fields.length; i++) {
            if (i > 0) {
                out.append(',');
            }
            out.append(fields[i]);
        }
        out.append('}');
    }

    private static String intsField(String key, IntSupplier supplier, int count) {
        StringBuilder out = new StringBuilder(count * 12 + 32);
        out.append('"').append(key).append("\":[");
        for (int i = 0; i < count; i++) {
            if (i > 0) {
                out.append(',');
            }
            out.append(supplier.getAsInt());
        }
        return out.append(']').toString();
    }

    private static String longsField(String key, LongSupplier supplier, int count) {
        StringBuilder out = new StringBuilder(count * 20 + 32);
        out.append('"').append(key).append("\":[");
        for (int i = 0; i < count; i++) {
            if (i > 0) {
                out.append(',');
            }
            out.append('"').append(hex(supplier.getAsLong())).append('"');
        }
        return out.append(']').toString();
    }

    private static String doublesField(String key, DoubleSupplier supplier, int count) {
        StringBuilder out = new StringBuilder(count * 22 + 32);
        out.append('"').append(key).append("\":[");
        for (int i = 0; i < count; i++) {
            if (i > 0) {
                out.append(',');
            }
            out.append('"').append(hex(Double.doubleToRawLongBits(supplier.getAsDouble()))).append('"');
        }
        return out.append(']').toString();
    }

    private static String floatsField(String key, FloatSupplier supplier, int count) {
        StringBuilder out = new StringBuilder(count * 14 + 32);
        out.append('"').append(key).append("\":[");
        for (int i = 0; i < count; i++) {
            if (i > 0) {
                out.append(',');
            }
            out.append('"').append(hexInt(Float.floatToRawIntBits(supplier.getAsFloat()))).append('"');
        }
        return out.append(']').toString();
    }

    private static String booleansField(String key, BooleanSupplier supplier, int count) {
        StringBuilder out = new StringBuilder(count * 6 + 32);
        out.append('"').append(key).append("\":[");
        for (int i = 0; i < count; i++) {
            if (i > 0) {
                out.append(',');
            }
            out.append(supplier.getAsBoolean());
        }
        return out.append(']').toString();
    }

    private static String noise3dField(String key, Noise noise) {
        StringBuilder out = new StringBuilder(SAMPLES * 22 + 32);
        out.append('"').append(key).append("\":[");
        for (int i = 0; i < SAMPLES; i++) {
            double[] p = coords(i);
            if (i > 0) {
                out.append(',');
            }
            out.append('"').append(hex(Double.doubleToRawLongBits(noise.get(p[0], p[1], p[2])))).append('"');
        }
        return out.append(']').toString();
    }

    private static String noise2dField(String key, Noise noise) {
        StringBuilder out = new StringBuilder(SAMPLES * 22 + 32);
        out.append('"').append(key).append("\":[");
        for (int i = 0; i < SAMPLES; i++) {
            double[] p = coords(i);
            if (i > 0) {
                out.append(',');
            }
            out.append('"').append(hex(Double.doubleToRawLongBits(noise.get(p[0], p[2])))).append('"');
        }
        return out.append(']').toString();
    }

    private static String positionsField(String key, PositionalRandomFactory factory) {
        StringBuilder out = new StringBuilder(SAMPLES * 22 + 32);
        out.append('"').append(key).append("\":[");
        for (int i = 0; i < SAMPLES; i++) {
            if (i > 0) {
                out.append(',');
            }
            double value = factory.at(posX(i), posY(i), posZ(i)).nextDouble();
            out.append('"').append(hex(Double.doubleToRawLongBits(value))).append('"');
        }
        return out.append(']').toString();
    }

    private static String hashesField(String key, PositionalRandomFactory factory) {
        StringBuilder out = new StringBuilder(SAMPLES * 22 + 32);
        out.append('"').append(key).append("\":[");
        for (int i = 0; i < SAMPLES; i++) {
            if (i > 0) {
                out.append(',');
            }
            double value = factory.fromHashOf("minecraft:feature_" + i).nextDouble();
            out.append('"').append(hex(Double.doubleToRawLongBits(value))).append('"');
        }
        return out.append(']').toString();
    }

    private static String seedsField(String key, PositionalRandomFactory factory) {
        StringBuilder out = new StringBuilder(SAMPLES * 22 + 32);
        out.append('"').append(key).append("\":[");
        for (int i = 0; i < SAMPLES; i++) {
            if (i > 0) {
                out.append(',');
            }
            double value = factory.fromSeed(123456789L + i).nextDouble();
            out.append('"').append(hex(Double.doubleToRawLongBits(value))).append('"');
        }
        return out.append(']').toString();
    }

    private static String blendedField(DensitySampler sampler) {
        StringBuilder out = new StringBuilder(SAMPLES * 22 + 32);
        out.append("\"samples\":[");
        for (int i = 0; i < SAMPLES; i++) {
            if (i > 0) {
                out.append(',');
            }
            float value = sampler.sampleValue(null, blendedX(i), blendedY(i), blendedZ(i));
            out.append('"').append(hexInt(Float.floatToRawIntBits(value))).append('"');
        }
        return out.append(']').toString();
    }

    // ---- deterministic coordinates ----------------------------------------

    private static double[] coords(int i) {
        double x = (i % 4096) * 0.3125 - 640.0;
        double y = (i % 211) * 0.25 - 26.0;
        double z = (i / 4096) * 0.5 - 12.0;
        if (i % 1000 == 0) {
            x = 25_165_824.0 + 1.5;
            y = -25_165_824.0 - 2.5;
        }
        return new double[] {x, y, z};
    }

    private static int posX(int i) {
        return (i * 13) % 2000 - 1000;
    }

    private static int posY(int i) {
        return (i * 7) % 128 - 64;
    }

    private static int posZ(int i) {
        return (i * 29) % 3000 - 1500;
    }

    private static int blendedX(int i) {
        return (i * 37) % 4000 - 2000;
    }

    private static int blendedY(int i) {
        return (i * 17) % 128 - 64;
    }

    private static int blendedZ(int i) {
        return (i * 7) % 3000 - 1500;
    }

    // ---- hex helpers -------------------------------------------------------

    private static String hex(long value) {
        String text = Long.toHexString(value);
        return "0000000000000000".substring(text.length()) + text;
    }

    private static String hexInt(int value) {
        String text = Integer.toHexString(value);
        return "00000000".substring(text.length()) + text;
    }
}
