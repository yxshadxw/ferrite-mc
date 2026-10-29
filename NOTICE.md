# Notices

Ferrite is an independent, from-scratch implementation. It is **not** affiliated with, endorsed by,
or sponsored by Mojang Studios or Microsoft. "Minecraft" is a trademark of Mojang Synergies AB.

- **No Mojang code or assets are distributed in this repository.** Vanilla data (registries, block
  states, worldgen JSON, loot tables, ...) is generated *locally* on your machine from the official
  server jar you download yourself (`python tools/fetch_vanilla.py`) and is git-ignored.
- Some tests and tools (`tools/wirecheck`, `tools/extractor`, `crates/*/tests/java`) compile small Java
  programs against that locally downloaded jar to check Ferrite's behavior against the real game.
  Only our own source is committed, never the jar, its classes, or dumps produced from it.
- Several modules (for example `ferrite-noise`) reimplement vanilla algorithms bit-for-bit so worlds
  and behavior stay compatible; they are written as independent Rust code and verified against the
  vanilla classes by differential tests.
- Compatibility target for plugins is the public Bukkit/Spigot/Paper API (a separate project,
  licensed by its own authors). The Paper API artifact is downloaded by the Gradle build and not
  vendored here.
