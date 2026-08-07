# NES tracer fixture

`tracers.nes` is a project-owned, deterministic iNES NROM image. It contains a minimal 6502 startup loop and blank CHR data; it is not derived from a commercial game. The fixture and generator are distributed under the repository's MIT license.

Regenerate it with `node fixtures/nes/generate.mjs`. The checked-in SHA-256 file is the independent fixture identity used by packaged and physical validation.
