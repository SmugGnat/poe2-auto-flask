# Acknowledgements

`poe2-auto-flask` is a Rust implementation built specifically for this project. Public Path of Exile projects informed its development.

The main references were:

- **GameHelper / GameHelper2** — [MordWraith/Gamehelper](https://github.com/MordWraith/Gamehelper) and [derekShaheen/GameHelper2](https://github.com/derekShaheen/GameHelper2) are GPLv3-licensed implementation references. [Gordin/GameHelper2](https://github.com/Gordin/GameHelper2) was also reviewed; no root project license was present when this project was prepared for release.
- **POE2Radar** — [Sikaka/POE2Radar](https://github.com/Sikaka/POE2Radar) and [NattKh/POE2Radar](https://github.com/NattKh/POE2Radar) are MIT-licensed research references.
- **ExileCore2** — [exCore2/ExileCore2](https://github.com/exCore2/ExileCore2) was an additional research reference. No root project license was present when this project was prepared for release. No ExileCore2 files are bundled with this project.

Because portions of the implementation were developed with reference to GPLv3 GameHelper implementation details, this project is released under `GPL-3.0-only`.

The original upstream C# projects and binaries are not bundled with `poe2-auto-flask`. These acknowledgements do not imply endorsement, authorship, or affiliation.

Rust dependencies retain their own licenses. Official releases include the project and third-party dependency license terms in `LICENSES.txt`, generated from the exact registry packages selected by `Cargo.lock` for the release build.
