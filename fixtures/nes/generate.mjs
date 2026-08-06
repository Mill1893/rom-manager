import { createHash } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const directory = dirname(fileURLToPath(import.meta.url));
const rom = Buffer.alloc(16 + 16_384 + 8_192, 0);
rom.set(Buffer.from([0x4e, 0x45, 0x53, 0x1a, 0x01, 0x01]), 0);

// Project-owned NROM fixture: SEI, CLD, and an infinite loop at $8000.
rom.set(Buffer.from([0x78, 0xd8, 0x4c, 0x02, 0x80]), 16);
rom[16 + 16_384 - 4] = 0x00;
rom[16 + 16_384 - 3] = 0x80;
rom[16 + 16_384 - 2] = 0x00;
rom[16 + 16_384 - 1] = 0x80;

await mkdir(directory, { recursive: true });
await writeFile(join(directory, "tracers.nes"), rom);
const digest = createHash("sha256").update(rom).digest("hex");
await writeFile(join(directory, "tracers.sha256"), `${digest}  tracers.nes\n`);
