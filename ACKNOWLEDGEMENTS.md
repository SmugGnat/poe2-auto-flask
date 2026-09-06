# Acknowledgements

`poe2-auto-flask` is a Rust implementation built specifically for this project. During development, public Path of Exile 2 reverse-engineering projects were used to cross-check memory-layout, signature, and runtime-behavior details.

The main references were:

- **GameHelper / GameHelper2** — implementation and layout references informed the game-state lookup, entity component lookup, world-area classification, vital/reservation handling, buff/status-effect data, and flask inventory work. [MordWraith/Gamehelper](https://github.com/MordWraith/Gamehelper) and [derekShaheen/GameHelper2](https://github.com/derekShaheen/GameHelper2) are GPLv3-licensed references. [Gordin/GameHelper2](https://github.com/Gordin/GameHelper2) was also reviewed as a layout reference; no root project license was present when this project was prepared for release.
- **POE2Radar** — used as an additional signature and memory-layout cross-check. [Sikaka/POE2Radar](https://github.com/Sikaka/POE2Radar) and [NattKh/POE2Radar](https://github.com/NattKh/POE2Radar) are MIT-licensed.
- **ExileCore2** — [exCore2/ExileCore2](https://github.com/exCore2/ExileCore2) was used as an additional memory-layout and inventory reference. No root project license was present when this project was prepared for release, so it is treated only as a research reference. No ExileCore2 files are bundled with this project.

Because portions of the implementation were developed with reference to GPLv3 GameHelper implementation details, this project is released under `GPL-3.0-only`.

The original upstream C# projects and binaries are not bundled with `poe2-auto-flask`. These acknowledgements do not imply endorsement, authorship, or affiliation.

Rust dependencies retain their own licenses. Official releases include `THIRD_PARTY_NOTICES.txt`, generated from the exact registry packages selected by `Cargo.lock` for the release build.
