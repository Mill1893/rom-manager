# ROM identity, format, and dataset constraints

Research date: 2026-08-05

## Scope

This report identifies constraints for recognizing user-supplied game media without
shipping game content. It covers cartridge, optical-disc, arcade, and home-computer
media, plus archives and public hash datasets. It is evidence for a later domain-model
and compatibility decision; it does not choose that model or a first-release matrix.

This is technical research, not legal advice. Dataset and content licensing should be
reviewed before release.

## Conclusions

- A hash identifies a particular byte sequence, not an abstract game. Every lookup
  must therefore retain what bytes were hashed: the whole file, a headerless payload,
  one archive entry, one disc track, or another documented representation.
- A game can require an ordered set of byte sequences and dependencies. Disc
  descriptors, multi-disc playlists, arcade ROM sets, BIOS/device sets, and parent
  CHDs cannot be reduced safely to one file hash.
- Dataset algorithms are interoperability fields. CRC32, MD5, and SHA-1 must be
  computed where a selected catalog requires them, but they should not be the
  application's adversarial integrity mechanism. NIST recommends moving security
  uses away from SHA-1 because practical collision attacks exist
  ([NIST, "NIST Retires SHA-1"](https://www.nist.gov/news-events/news/2022/12/nist-retires-sha-1-cryptographic-algorithm)).
- An archive is packaging. Its outer bytes can change when member order,
  timestamps, compression, comments, or other metadata change while member bytes do
  not. Identification should be able to hash bounded, decompressed member streams
  without trusting filenames or extracting to arbitrary filesystem paths.
- Dataset provenance and license travel with imported records. MAME's `hash/`
  directory has a clear CC0 dedication; libretro-database is CC BY-SA 4.0. Equivalent
  direct reuse permission was not found for No-Intro or Redump, so those datasets
  must not be bundled until their terms are clarified.
- The product boundary should remain metadata-only: inspect user-selected content,
  retain fingerprints and user-local paths, and do not fetch, upload, or redistribute
  game or firmware bytes.

## Identity constraints

### The hashed representation is part of the evidence

Libretro's database documentation describes checksum-based validation and naming,
but also says its key varies by system: CRC for smaller media and an embedded serial
for some large disc images. Its records can retain CRC, MD5, and SHA-1 even when a
different lookup key is used
([libretro-database README, pinned revision](https://github.com/libretro/libretro-database/blob/6fd53f98459c9a29a657c37a2efaac9f7dec25e5/README.md#fields--headers)).
MAME software lists independently represent ROM size, CRC, SHA-1, offset, load
operation, dump status, data-area width/endianness, and disk SHA-1
([MAME software-list DTD, pinned revision](https://github.com/mamedev/mame/blob/aaac1f637a8cbf23724b61ea578d70a32f2cf4fe/hash/softwarelist.dtd)).
These are not interchangeable notions of identity.

Consequences for later design:

- Store the hash algorithm, digest, byte length, and a representation/transform
  identifier together. A naked digest is insufficient evidence.
- Compare only against catalog records defined for the same byte target. Do not
  silently strip, add, byte-swap, pad, repair, or convert data and then present the
  result as a match to the original input.
- If a documented normalization is supported, preserve both observations: the
  source-file fingerprint and the derived-byte fingerprint, plus the transformation
  that connected them.
- Keep catalog identity/version and record provenance with a match. Catalog updates
  can correct dumps, add revisions, or change set structure.
- Treat distinct revisions, regions, prototypes, bad dumps, and overdumps as distinct
  catalog assertions even when the product may later group them for display.

### Hash policy

CRC32, MD5, and SHA-1 are common catalog lookup fields. For example, the
libretro-database sample record contains all three, while MAME's software-list DTD
uses CRC and SHA-1 for ROMs and SHA-1 for disks
([libretro-database README](https://github.com/libretro/libretro-database/blob/6fd53f98459c9a29a657c37a2efaac9f7dec25e5/README.md#fields-specified-in-game-information-databases),
[MAME DTD](https://github.com/mamedev/mame/blob/aaac1f637a8cbf23724b61ea578d70a32f2cf4fe/hash/softwarelist.dtd)).
They remain necessary to join those datasets. Separately, new application-controlled
fingerprints should use a modern digest such as SHA-256; this prevents an imported
catalog's legacy algorithm from becoming a security guarantee.

Hashing should be streaming and bounded. Record byte count with every digest, reject
unexpected truncation, and distinguish a verified catalog match from an internally
consistent but unknown file.

## Media findings

### Cartridge images

Cartridge files are often single-file media, but "the file" is not always the ROM-chip
payload. Libretro's source table explicitly lists three NES representations from
No-Intro: iNES 1.0 headered, NES 2.0 headered, and headerless
([libretro-database source table](https://github.com/libretro/libretro-database/blob/6fd53f98459c9a29a657c37a2efaac9f7dec25e5/README.md#sources)).
MAME software lists also separate the logical data area from loading details such as
offset, interleaving, word swapping, reloads, and ignored bytes
([MAME software-list DTD](https://github.com/mamedev/mame/blob/aaac1f637a8cbf23724b61ea578d70a32f2cf4fe/hash/softwarelist.dtd)).

Constraints:

- Headered and headerless hashes are different observations. Header removal must be
  selected by a system/catalog-specific rule, not inferred from file size alone.
- Headers can contain behavior-bearing mapper, memory, region, and trainer metadata;
  recognition cannot imply that two containers are execution-equivalent merely
  because a derived payload hash matches.
- Copier headers, optional trainers, padding/overdumps, interleaved dumps, and
  byte-order variants require explicit format adapters. Unknown transformations
  should produce "unsupported/unknown representation," not a fuzzy match.
- Revisions must remain distinguishable. A shared title or parent relation does not
  make revision bytes identical.

### Optical discs

An optical-disc image may be a graph rather than a file. Redump publishes cuesheets
and DAT files separately by system
([Redump downloads](http://redump.org/downloads/)). A CUE parser consumes referenced
files, track numbers and modes, indices, sessions, and sector sizes including 2048,
2336, and 2352 bytes
([Flycast CUE parser](https://github.com/flyinghead/flycast/blob/master/core/imgread/cue.cpp)).
A GDI parser reads a track count followed by track number, start address, control,
sector size, filename, and file offset
([Flycast GDI parser](https://github.com/flyinghead/flycast/blob/master/core/imgread/gdi.cpp)).
Mednafen likewise supports CUE/BIN, CCD/IMG/SUB, and TOC and notes that subchannel
data and raw-sector error-correction data can affect behavior
([Mednafen compact-disc image documentation](https://mednafen.github.io/documentation/#Section_cd_images)).

Disc identity therefore needs all of the following evidence where applicable:

- complete, ordered track membership;
- digest and byte length for each catalog-defined track stream;
- track mode and sector size;
- file offset, index/pregap, start address, and session structure;
- subchannel or other sidecar membership when the selected representation includes it;
- disc order for multi-disc releases, separately from each disc's identity.

A descriptor hash alone proves only the descriptor text. A concatenation hash alone
loses track boundaries unless the framing is canonical and recorded. Missing tracks,
unresolved references, conflicting paths, malformed sector alignment, and duplicate
track numbers must prevent a complete-set match.

Dumping itself has representation choices. Redumper documents drive read offsets,
pregap/lead-out accessibility, raw subchannel capture, track splitting, and sector
error states
([redumper README](https://github.com/superg/redumper/blob/main/README.md)). The manager
should consume catalog-compatible outputs; it should not claim that conversion or
repair recreates a verified dump unless all required source and transformation rules
are specified.

CHD is another representation, not merely a filename suffix. `chdman` reports data
and metadata SHA-1 values, supports media-specific creation/extraction, and allows
delta CHDs that require a parent
([MAME chdman documentation](https://docs.mamedev.org/tools/chdman.html)). A CHD match
must state whether the digest addresses CHD logical data, metadata, or outer file
bytes, and a delta CHD is incomplete without its parent.

### Arcade sets

MAME defines an arcade ROM image as one chip's data and a ROM set as the multiple
files needed for a machine. It documents parent/clone relationships, merged, split,
and non-merged packaging, plus separate BIOS and device sets
([MAME, "About ROMs and Sets"](https://docs.mamedev.org/usingmame/aboutromsets.html)).
Some machines additionally require one or more CHDs, and delta CHDs require their
parent CHD.

Constraints:

- Identify member ROM bytes and the set definition, not the ZIP filename or ZIP hash.
- A set's completeness is evaluated against a particular MAME/software-list version.
  MAME explains that corrected dumps and documentation changes alter sets over time
  ([MAME, "Troubleshooting your ROM sets"](https://docs.mamedev.org/usingmame/aboutromsets.html#troubleshooting-your-rom-sets-and-the-history-of-roms)).
- Parent/clone is a dependency/grouping relation, not byte identity. Split and merged
  archives can package the same runnable definition differently.
- BIOS, device, parent-ROM, key, and CHD dependencies need independent identity and
  availability states. "Recognized set" and "complete/runnable set" are separate
  results.
- Preserve MAME load semantics where they matter. Equal member digests without the
  correct regions, offsets, interleaving, or reload rules do not establish an equal
  machine image.

### Home-computer media

Home-computer software can be represented at several capture levels. VICE describes
TAP as raw cassette pulse timing, T64 as a file container with limitations, G64 as a
low-level GCR track stream, P64 as flux-transition data, and D64 as a sector-by-sector
disk image
([VICE file-format documentation](https://vice-emu.sourceforge.io/vice_17.html)).
The same program extracted as a PRG does not preserve the same evidence as its D64,
G64, P64, or TAP source; protections, loaders, directory structure, errors, timing,
and other files can be lost.

Constraints:

- Represent the capture format/level explicitly; do not silently equate extracted
  files, sector images, encoded tracks, flux captures, and tape pulse captures.
- Multi-disk games and disks with multiple files require ordered membership rather
  than title-name grouping.
- Writable media can change during emulation. Identification should fingerprint the
  user-supplied baseline separately from later modified working copies.
- Format variants matter. VICE documents multiple TAP versions and D64 sizes with
  differing track counts and optional error bytes; extension-only dispatch is not
  enough.

## Archive and descriptor handling

ZIP is explicitly a container for one or more files. Its records contain compression
method, names, comments, timestamps, extra fields, attributes, and member ordering;
members may be stored or independently compressed
([PKWARE APPNOTE 6.3.10, sections 4.1-4.4](https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT)).
The 7z format can additionally use solid compression, compressed headers, Unicode
names, encryption, and multiple methods
([7-Zip format documentation](https://www.7-zip.org/7z.html)). RAR supports solid and
multi-volume archives, encryption, links/redirections, and optional per-file hashes
([RAR 5.0 technical note](https://www.rarlab.com/technote.htm)).

Required handling constraints:

- Detect supported formats by signature/parser, not extension alone. PKWARE
  specifically recommends internal ZIP record signatures.
- Keep an outer-file fingerprint for cache invalidation/audit, but identify media from
  bounded member streams and descriptor structure where the catalog expects members.
- Preserve duplicate member names as separate entries until a format-specific rule
  rejects them; never let a map keyed only by filename silently overwrite one.
- Apply maximum archive bytes, member count, per-member output, total output,
  compression ratio, nesting depth, descriptor recursion, and processing time.
- Reject encrypted or unsupported-method members as uninspectable rather than unknown
  media. Multi-volume archives are incomplete until every required volume is present.
- Avoid filesystem extraction where possible. If extraction is unavoidable, reject
  absolute paths, `..`, and symlink escapes. Libarchive exposes distinct safeguards
  for exactly these cases
  ([libarchive `archive_write_disk` manual](https://github.com/libarchive/libarchive/blob/master/libarchive/archive_write_disk.3)).
- Resolve CUE/GDI/M3U and similar references inside one constrained virtual root.
  Mednafen warns that CUE/TOC files can include arbitrary local files and enables an
  untrusted-path check by default
  ([Mednafen security documentation](https://mednafen.github.io/documentation/#Section_security_includes)).
- Filenames are hints, never proof. Decode names deterministically, retain raw names
  when possible, and match content hashes independently of rename conventions.

## Dataset and license assessment

| Source | Identity coverage and evidence | License/reuse evidence | Release constraint |
| --- | --- | --- | --- |
| MAME `hash/` software lists | Cartridge, computer, and other software parts; ROM CRC/SHA-1 and disk SHA-1 with structural/load metadata ([DTD](https://github.com/mamedev/mame/blob/aaac1f637a8cbf23724b61ea578d70a32f2cf4fe/hash/softwarelist.dtd)) | MAME's `COPYING` expressly dedicates the contents of `hash/` under CC0 ([pinned `COPYING`](https://github.com/mamedev/mame/blob/aaac1f637a8cbf23724b61ea578d70a32f2cf4fe/COPYING), [CC0 text](https://github.com/mamedev/mame/blob/aaac1f637a8cbf23724b61ea578d70a32f2cf4fe/docs/legal/CC0)) | Clear candidate for bundling, subject to preserving source/version and not implying MAME endorsement. The CC0 statement is limited to `hash/`, not all MAME code. |
| libretro-database | Aggregates native records and bulk imports from No-Intro, Redump, MAME, TOSEC, and others; documents checksum/serial lookup and source precedence ([README](https://github.com/libretro/libretro-database/blob/6fd53f98459c9a29a657c37a2efaac9f7dec25e5/README.md)) | Repository license is CC BY-SA 4.0 ([pinned license](https://github.com/libretro/libretro-database/blob/6fd53f98459c9a29a657c37a2efaac9f7dec25e5/LICENSE)); it requires attribution and ShareAlike for covered adaptations and addresses database rights | Reuse needs attribution/ShareAlike design and a provenance audit. A repository license cannot safely be assumed to cure rights in every imported third-party record; retain source per record/import. |
| No-Intro DAT-o-MATIC | Primarily non-disc media; libretro identifies No-Intro as its bulk source for many systems and multiple NES representations ([libretro source table](https://github.com/libretro/libretro-database/blob/6fd53f98459c9a29a657c37a2efaac9f7dec25e5/README.md#sources)) | No explicit database redistribution license equivalent to MAME's CC0 was found in the public owner materials reviewed | Do not bundle or mirror direct No-Intro data until the owner confirms applicable terms. Runtime import of a user-obtained DAT is a separate product/legal decision. |
| Redump | Disc-oriented DATs and cuesheets by system ([official downloads](http://redump.org/downloads/)); libretro describes its Redump directories as bulk upstream imports ([README](https://github.com/libretro/libretro-database/blob/6fd53f98459c9a29a657c37a2efaac9f7dec25e5/README.md#folder-guide)) | No explicit database redistribution license was found on the official download/site materials reviewed | Do not bundle or mirror Redump DATs/cuesheets until reuse terms are confirmed. Do not mistake public download access for redistribution permission. |
| TOSEC | Overlapping home-computer/console metadata, used as a lower-precedence source by libretro ([libretro source table](https://github.com/libretro/libretro-database/blob/6fd53f98459c9a29a657c37a2efaac9f7dec25e5/README.md#sources)) | No sufficiently clear, broad database license was established in this review | Treat as unavailable for bundling pending a source-specific license review. |
| PKWARE ZIP specification | Container parsing and metadata semantics ([APPNOTE](https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT)) | APPNOTE permits using its information to create ZIP readers/writers but restricts reproducing the document; some marked proprietary features require separate terms | Implement interoperable reading through a suitable library; cite rather than copy specification text. Decide supported methods explicitly. |
| 7-Zip/LZMA SDK | 7z format and implementation ([format page](https://www.7-zip.org/7z.html), [license page](https://www.7-zip.org/license.txt)) | 7-Zip is mainly LGPL with stated exceptions; LZMA SDK is public domain | Library selection must follow component terms. Format support does not grant rights to archived game content. |
| RAR/UnRAR | RAR 5 structure and extraction behavior ([technical note](https://www.rarlab.com/technote.htm)) | UnRAR source has a license restriction against using it to recreate the RAR compression algorithm ([RARLAB license](https://www.rarlab.com/license.htm)) | Decompression support must use a legally compatible implementation and respect its license; writing RAR is unnecessary for identification. |

The catalog contains metadata, not permission to distribute the bytes it describes.
The U.S. Copyright Office treats computer programs as copyrightable literary works
and distinguishes ownership of a copy from ownership of copyright
([Copyright Office Circular 61](https://www.copyright.gov/circs/circ61.pdf)). Product
policy should not claim that possession, identification, or a catalog match proves a
user's right to copy or share content.

## Inputs to a later compatibility decision

The following capability boundaries should be decided per system and catalog, not as
one global "ROM support" switch:

- accepted source representations and exact byte targets;
- allowed, documented derivations and whether they establish exact or derived matches;
- required catalog and catalog version;
- single-file versus ordered-set completeness rules;
- descriptor and archive formats and safe parser limits;
- firmware, parent, device, key, sidecar, and CHD dependency reporting;
- writable-copy behavior;
- match confidence/status vocabulary; and
- dataset distribution mode: bundled, downloaded under accepted terms, or imported
  by the user.

## Explicit unknowns and blockers

- No-Intro's current database redistribution and commercial-use terms were not
  established from an explicit owner license. Obtain written/posted terms before
  bundling any direct export.
- Redump's current database and cuesheet redistribution terms were not found. Confirm
  them with the project before bundling or mirroring.
- libretro-database's CC BY-SA license is clear at repository level, but the rights and
  attribution requirements of each upstream bulk import still need a provenance/legal
  review.
- The exact hash target used by every candidate DAT for every system has not been
  exhaustively mapped. A catalog adapter must document and test this before that
  system is declared supported.
- CUE has implementation-specific subsets and extensions. The supported command,
  encoding, path, audio codec, session, pregap, and subchannel subset needs a separate
  compatibility decision.
- CHD version compatibility and whether matching uses CHD logical SHA-1, metadata
  SHA-1, extracted track hashes, or a combination needs an explicit decision.
- Arcade set support must select one or more MAME versions and define how upgrades
  migrate prior matches and completeness results.
- Home-computer coverage needs format-by-format decisions; extension lists alone do
  not define equivalent capture quality.
- Jurisdiction, distribution model, and whether user-imported metadata is persisted or
  shared can change the legal analysis. Obtain project-specific legal review before
  release.
