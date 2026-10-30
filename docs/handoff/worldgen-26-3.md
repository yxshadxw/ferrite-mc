# World generation on Minecraft 26.3

The 26.3 server jar and generated data use updated names and structures:

- `NoiseRouter.chunkSurfaceLevel()` replaces the older brief's `initial_density_without_jaggedness` differential target. It references `minecraft:overworld/chunk_surface_level`, which interpolates the preliminary surface-level function.
- `NoiseBasedChunkGenerator.buildTerrain` is the 26.3 entry point named in place of the older `doFill` API. The overworld final-density data explicitly contains a 4-by-8 `interpolated` node; interpolation belongs to the density graph.
- Noise settings reference `material_rule`, and 26.3 uses the separate `worldgen/material_rule` and `worldgen/material_condition` registries instead of an inline `surface_rule`.
- `DensitySampler.sampleValue` returns a Java `float`; a faithful evaluator needs to preserve vanilla's intermediate float arithmetic.

These observations were verified against `vanilla/26.3/lib/server-26.3.jar` with `javap` and the generated overworld JSON. They do not establish that terrain generation has been implemented or verified.
