# Android handheld Device Profile conventions

Research for [issue #12](https://github.com/Mill1893/rom-manager/issues/12), conducted 2026-08-05. This report describes current conventions and decision-relevant variability; it does not define the final Device Profile model.

## Executive summary

- There is no Android-wide ROM layout. A common workflow uses one user-selected `ROMs` root and frontend-specific lowercase system directories. ES-DE is the strongest reusable convention because its Android configuration owns an explicit mapping among directory key, display name, scraper platform, accepted extensions, theme key, and launch commands.[^esde-android-config]
- Storage location and host transport are separate concerns. The same logical `ROMs/nes` target can be Android internal shared storage, a portable microSD filesystem, a host-mounted filesystem/share, or an MTP object hierarchy. Absolute Android paths and host mount paths are therefore observations, not portable profile paths.[^esde-android-storage][^wpd-enumeration]
- ES-DE keeps ROMs separate from frontend data. By default, Android application data is under internal shared storage `ES-DE`, with metadata at `ES-DE/gamelists/<system>/gamelist.xml` and artwork at `ES-DE/downloaded_media/<system>/<media-type>/...`. Media filenames mirror each ROM's path below its system directory and use the ROM basename.[^esde-android-onboarding][^esde-media]
- ES-DE folder keys are frontend identifiers, not universal Platform names. For example, current Android ES-DE uses `gc`, `n3ds`, `psx`, and `snes`; another frontend may use a different key, display label, scraper ID, or playlist/database name for the same console.[^esde-android-config][^pegasus-metadata][^retroarch-playlists]
- Reliable Odin 3-specific primary documentation for ROM placement, microSD formatting/mount behavior, or USB file-transfer behavior was not found. AYN's first-party product page establishes Android 15 and internal UFS variants, while ES-DE's first-party compatibility documentation confirms Odin 3 support and one firmware-setting workaround. Everything else should remain generic-Android evidence until checked on the user's device.[^ayn-odin3][^esde-odin3]

## Research method and evidence quality

The starting points were the user-supplied guide families:

- Joey's Retro Handhelds recommends extracting a prebuilt `ROMs` tree "anywhere" and states that its folder package follows ES-DE naming.[^joey-android][^joey-folders]
- Retro Game Corps recommends letting ES-DE create the folder tree on the selected storage, or preparing the same tree on a microSD card with its directory archive.[^rgc-android][^rgc-directories]

Those are useful descriptions of a current enthusiast workflow, but the findings below rely on owners of each contract where possible: AYN for published Odin 3 facts, Android/Google for storage and USB behavior, ES-DE/Daijisho/Pegasus for frontend behavior, Libretro for RetroArch behavior, and Microsoft WPD documentation for the host-side object model used with portable devices.

The ES-DE citations are pinned to upstream commit `9060f03d1fb6595836924f070d8c36d38f4f6b82` (2026-08-04) because its system list and compatibility notes can change. A profile based on this evidence should record the frontend/configuration version it implements rather than claiming an eternal canonical list.

## Storage and transport are different axes

| Workflow | Namespace seen by Android/frontend | Namespace seen by desktop manager | Consequences for a reusable profile |
| --- | --- | --- | --- |
| Internal shared storage | Common ES-DE example: `/storage/emulated/0/ROMs`; default frontend data: `/storage/emulated/0/ES-DE` | Over USB, a File Transfer/MTP storage object and child objects, not a normal mounted filesystem path | Keep an MTP storage selector/object ancestry separate from logical paths such as `ROMs/nes`. Do not persist the Android absolute path as the host destination. |
| Portable microSD in handheld | Common ES-DE example: `/storage/<volume-id>/ROMs`, such as `/storage/459E-3A7F/ROMs` | May appear as another MTP storage object while inserted | The Android volume ID is card/format dependent. Select the intended storage, then resolve the relative root. Record whether the frontend's ROM root and data root are on this volume. |
| Portable microSD in a card reader | Not mounted in Android during transfer | Ordinary host-mounted removable filesystem | Sync relative to the selected card root. The host mount letter/path is ephemeral. This path offers normal filesystem semantics and should not be conflated with MTP to the same card. |
| Adopted microSD | Presented as part of private/adopted Android storage | Not a portable card-reader target; Android formats and encrypts adopted media for one device | Exclude direct-reader expectations. If exposed over USB, handle through the device transport and validate what Android publishes. |
| OS-mounted local disk or network share | Only relevant to Android if the OS/device has actually mounted it and the frontend can select it | Ordinary filesystem at a host mount path | Once mounted by the host OS, use filesystem behavior and a relative root. Connection/authentication/discovery are outside a profile's folder convention. ES-DE permits a configurable ROM root on a file share but warns that network protocols, especially SMB, can perform poorly.[^esde-network] |
| USB MTP/File Transfer | Android remains owner of its storage | Device -> storage -> folder/file object hierarchy | Require the user to unlock the device and select **File Transfer**. Enumerate objects and their properties; do not assume POSIX paths, drive letters, atomic rename, or ordinary filesystem watchers.[^android-usb][^wpd-enumeration] |

Android's storage documentation distinguishes app-specific storage from shared storage and documents scoped access to shared documents through the Storage Access Framework.[^android-storage][^android-saf] Android's adoptable-storage documentation says adopted media is formatted and encrypted to work with a single device, unlike portable media.[^android-adoptable] ES-DE reflects those rules in practice: onboarding asks separately for an application-data directory and a ROM directory, allows either to be selected on an SD card, and warns that individual emulators may need scoped access to each system directory.[^esde-android-onboarding][^esde-scoped-storage]

The important profile boundary is therefore:

```text
transport locator / selected storage + relative content root + relative frontend layout
```

It is not one absolute path shared by Android, a card reader, Windows Explorer, and MTP. Microsoft WPD describes portable-device content as objects referenced by object identifiers and recursively enumerated from a device object, with names and parent identifiers exposed as properties.[^wpd-enumeration] That is materially different from opening a host filesystem path even when Explorer renders it with folder-like UI.

## ROM folder layout and Platform naming

### ES-DE's contract

ES-DE selects one ROM root (`%ROMPATH%`) and normally maps each system to `%ROMPATH%/<system-key>`. Its onboarding can generate all current directories plus informational `systems.txt` and per-system `systeminfo.txt` files. Those text files are conveniences, not runtime requirements.[^esde-onboarding][^esde-relative-root]

A representative subset of the current Android mappings is:

| Logical console | ES-DE directory/system key | ES-DE full name | Notes |
| --- | --- | --- | --- |
| Arcade | `arcade` | Arcade | Broad system with several possible emulator/ROM-set conventions |
| Nintendo Entertainment System | `nes` | Nintendo Entertainment System | Single archive or ROM file |
| Super Nintendo | `snes` | Nintendo SNES (Super Nintendo) | `sfc` and `snesna` are separate ES-DE systems, not aliases to silently merge |
| Nintendo 64 | `n64` | Nintendo 64 | Single archive or ROM file |
| Nintendo DS | `nds` | Nintendo DS | Single archive or ROM file |
| Nintendo 3DS | `n3ds` | Nintendo 3DS | The key is not simply `3ds` |
| Nintendo GameCube | `gc` | Nintendo GameCube | Disc image; multi-disc arrangements can use `.m3u` |
| Nintendo Wii | `wii` | Nintendo Wii | Separate from GameCube despite the shared Dolphin emulator |
| Sega Genesis | `genesis` | Sega Genesis | ES-DE also has `megadrive` and regional systems |
| Sega Saturn | `saturn` | Sega Saturn | Multi-disc arrangements can use `.m3u` |
| Sony PlayStation | `psx` | Sony PlayStation | ES-DE recommends `.chd` for single-disc and `.m3u` for multi-disc games |
| Sony PlayStation 2 | `ps2` | Sony PlayStation 2 | Supported forms are defined by the current Android config/emulator choice |
| Sony PSP | `psp` | Sony PlayStation Portable | Single disc image |
| Nintendo Switch | `switch` | Nintendo Switch | Current emulator integration is version-sensitive |

The authoritative details are the current Android `es_systems.xml` and the Android supported-systems table, not this illustrative subset.[^esde-android-config][^esde-supported-systems] Each ES-DE system record contains several independent identifiers:

- `<name>`: system identity and normally the directory key.
- `<fullname>`: user-facing display label.
- `<path>`: usually `%ROMPATH%/<key>`, but customizable.
- `<extension>`: files ES-DE scans as games.
- `<platform>`: one or more scraper platform identifiers; it may differ from `<name>` or be shared by multiple systems.
- `<theme>`: theme lookup key.
- `<command>` entries: emulator-specific launch behavior.

This is why a Device Profile should not use a single string called "Platform name" for all purposes. The app's canonical Platform identity, frontend system key, display label, scraper identifier, accepted transfer forms, and emulator launch integration are separate facts.

ES-DE recursively reflects subdirectories. Multi-file and multi-disc games may be grouped below a system directory and represented by an `.m3u` playlist. ES-DE also supports a directory whose name has a supported extension and which contains a same-named launch file, but this is a frontend-specific convention rather than a generic filesystem rule.[^esde-multifile]

Directory case matters in the Android workflow. ES-DE's Android FAQ specifically recommends generated lowercase system directories because some standalone emulators fail after being granted scoped access to an uppercase variant.[^esde-case]

### BIOS is not a universal sibling convention

Joey's convenience package adds a root `BIOS` directory, but describes that addition as its own convention on top of ES-DE naming.[^joey-folders] ES-DE launch integration and emulator documentation show that BIOS placement varies by emulator and system; some files belong in an emulator data directory and some arcade/system files may be expected beside ROMs. A reusable ROM layout should not infer that `ROMs/BIOS` or a card-root `BIOS` folder is universally consumed. This application's map also excludes emulator and BIOS setup, so any BIOS mapping should remain an explicit, optional compatibility fact rather than an implicit ROM rule.

## Frontend metadata and artwork

### ES-DE

The default Android arrangement is:

```text
<shared-storage>/
  ROMs/
    <system-key>/
      <game files and optional subdirectories>
  ES-DE/
    gamelists/
      <system-key>/gamelist.xml
    downloaded_media/
      <system-key>/
        3dboxes/
        backcovers/
        covers/
        custom/
        fanart/
        manuals/
        marquees/
        miximages/
        physicalmedia/
        screenshots/
        titlescreens/
        videos/
```

The application-data and game-media roots are configurable and can be split across internal storage and an SD card. ES-DE's Android FAQ recommends keeping `ES-DE`, especially `downloaded_media`, on internal storage for large collections because Android's SAF/MediaStore and common FAT/exFAT external storage can make startup very slow.[^esde-performance]

ES-DE stores game metadata in per-system `gamelist.xml`. A game path requires a leading `./` and is relative to the system's ROM directory. Editable values include name, sort name, description, rating, release date, developer, publisher, genre, players, status flags, play count/time, controller, and emulator override.[^esde-gamelist][^esde-metadata]

Artwork is deliberately not located through image tags in `gamelist.xml`. ES-DE matches it by system, media type, and ROM-relative name. For example:

```text
ROMs/c64/Multidisk/Last Ninja 2/Last Ninja 2.m3u
ES-DE/downloaded_media/c64/screenshots/Multidisk/Last Ninja 2/Last Ninja 2.jpg
ES-DE/downloaded_media/c64/videos/Multidisk/Last Ninja 2/Last Ninja 2.mp4
```

Supported image extensions are `.jpg`, `.png`, and `.webp`; supported video extensions are documented separately. The media filename must correspond exactly, including its relative subdirectory.[^esde-media]

This makes deterministic ES-DE export possible, but also creates an ownership question for later design: replacing `gamelist.xml` can overwrite user-edited names, favorites, completion state, play statistics, hidden/broken flags, and per-game emulator choices. A profile may describe format and placement, but sync policy must separately decide which fields the ROM Manager owns.

### Daijisho

Daijisho is platform-centric and folder-agnostic rather than tied to the ES-DE tree. Its first-party wiki directs the user to import/create a Platform, select one or more folders, and **Sync**. Platform and Player definitions are importable JSON; preview media can be edited or bulk-imported, and metadata can be imported from an EmulationStation `gamelist.xml` or DAT file.[^daijisho]

Decision relevance: an ES-DE-compatible ROM tree can also be selected in Daijisho, but that does not make Daijisho's Platform definitions, player regexes, artwork database, or sync state part of the ES-DE on-card contract. Daijisho integration would need its own frontend adapter or supported import artifact; copying ROMs into `ROMs/<es-de-key>` alone does not configure a Daijisho Platform.

### Pegasus

Pegasus provides a portable, frontend-owned sidecar contract. It discovers `metadata.pegasus.txt` (or `metadata.txt`) in configured game directories. Collection records define name, optional `shortname`, included extensions/files/directories, and launch command; game records define files, title, developer, publisher, genre/tags, descriptions, players, release date, rating, and optional launch override. Its documented common `shortname` values overlap many ES-DE keys but are not identical in every case (`3ds` versus ES-DE's `n3ds`, for example).[^pegasus-metadata]

Decision relevance: a profile can target Pegasus by writing explicit sidecars and assets, but should not assume the ES-DE system mapping is valid Pegasus metadata. On Android, launch commands may need raw paths or Android content URIs and per-directory emulator permission, which is emulator-specific.[^pegasus-android]

### RetroArch

RetroArch does not require a particular ROM directory layout. Its official guide says content can be stored anywhere RetroArch can access and shows system folders only as a practical example. Its frontend index is a `.lpl` JSON playlist whose item fields include ROM `path`, display `label`, core, checksum/serial, and `db_name`. Thumbnails are under `thumbnails/<playlist-name>/Named_Boxarts`, `Named_Snaps`, or `Named_Titles` and are matched to playlist labels (with documented filename sanitization/fallbacks).[^retroarch-playlists]

Decision relevance: RetroArch's playlist/database names, such as `Nintendo - Game Boy.lpl`, are another Platform namespace. They should be mapped explicitly rather than derived from an ES-DE folder key.

## AYN Odin 3: established facts and evidence gap

AYN's current product page establishes only the facts relevant here that the Odin 3 runs Android 15, uses UFS 3.1 internal storage, is sold with 128 GB through 1 TB internal-storage variants, and ships without games.[^ayn-odin3] AYN's public firmware page does not currently publish an Odin 3 manual or Odin 3 storage/file-transfer instructions.[^ayn-firmware] Searches of AYN's site and public code search did not locate a first-party manual that documents:

- whether and how Odin 3 firmware offers portable versus adopted microSD setup;
- supported microSD filesystems/capacities;
- the Android mount path and MTP storage labels for an inserted card;
- whether both internal shared storage and portable microSD are exposed over MTP;
- default USB mode, MTP quirks, or stable device/storage identifiers;
- any AYN-owned ROM directory, Platform naming, metadata, or artwork convention.

ES-DE's own Android documentation is the best current Odin 3-specific primary source found for frontend behavior. It lists Odin 3/Android 15 as supported and reports that the theme downloader needs an AYN Handheld Settings SELinux option enabled with default firmware settings.[^esde-odin3] This confirms an actual Odin 3 firmware quirk, but it does not establish a storage or ROM-layout convention.

Accordingly, an Odin 3 preset should initially be an Android 15 + chosen-frontend convention with a device label, not a claim that AYN prescribes the layout. Before declaring an Odin 3 profile validated, test a physical unit and record firmware/build plus:

- internal and microSD storage names and hierarchy over Windows MTP;
- whether a portable card prepared in a desktop reader is accepted unchanged;
- filesystem and file-size behavior using representative large disc images;
- ES-DE application-data and ROM-root selection on internal storage and microSD;
- lowercase system discovery and standalone-emulator scoped access;
- disconnect/reconnect behavior and whether target identity survives port/reboot changes;
- metadata/artwork visibility after direct-reader and MTP transfers.

## Decision-relevant profile fields

The evidence supports capturing the following dimensions. These are candidate facts, not a proposed schema.

| Dimension | Why it varies |
| --- | --- |
| Profile identity, provenance, and tested device/firmware | A frontend convention may be generic Android; device-specific exceptions need evidence and version scope. |
| Frontend and frontend configuration/version | Directory keys, extensions, platform IDs, and launch support evolve independently of hardware. |
| Transport capabilities | Filesystem/card-reader, host-mounted share, and MTP expose different identity and operation semantics. |
| Storage selector | Internal shared storage, portable removable storage, and adopted storage are not interchangeable. MTP needs a storage-object selector; filesystem targets need a selected root. |
| Logical ROM root relative to selected storage | Commonly `ROMs`, but user-selectable in ES-DE and folder-agnostic frontends. |
| Frontend data root and media root | May be distinct from ROM root and intentionally placed on internal storage for performance. |
| Per-Platform canonical identity and frontend system key | Prevents conflating app Platform identity with `psx`, `n3ds`, a display label, scraper key, or playlist name. |
| Relative ROM directory and case policy | ES-DE Android expects specific lowercase keys; custom frontend mappings can override them. |
| Accepted file extensions/forms and multifile policy | Scanning and launchability depend on frontend and emulator; archives, CHD, M3U, directory-as-game, and arcade sets differ. |
| Metadata format/path and field ownership | ES-DE XML, Pegasus sidecars, Daijisho imports, and RetroArch playlists have different contracts; user state must not be overwritten accidentally. |
| Artwork categories, path template, basename/sanitization rule, and extensions | ES-DE mirrors ROM-relative paths; RetroArch keys by playlist label; other frontends import media into app-owned state. |
| Permission/setup prerequisites | Android scoped directory grants and emulator/frontend storage permissions cannot be satisfied by file transfer alone. |
| Detection and validation evidence | Storage labels, object ancestry, marker files, expected directories, and read/write probes are safer than an absolute mount path alone. |

Two fields should specifically *not* be treated as universal identifiers: the current host mount path/drive letter and Android's `/storage/<volume-id>` path. Both are environment observations. Likewise, an MTP object identifier is useful while enumerating a connected session but should not be assumed to be a cross-session filesystem identity without transport-specific validation.

## Sources

[^joey-android]: Joey's Retro Handhelds, [Android Handheld Emulation Setup Guide](https://joeysretrohandhelds.com/guides/android-emulation-setup-guide/), "ROMs & BIOS" and "Frontends" (secondary source, accessed 2026-08-05).
[^joey-folders]: Joey's Retro Handhelds, [`joeys-rom-folders` README at `d0e4b5f`](https://github.com/JoeysRetroHandhelds/joeys-rom-folders/blob/d0e4b5fbb31a227135d829e56e09957ae2c6fe92/README.md) (secondary convenience package).
[^rgc-android]: Retro Game Corps, [Android Emulation Starter Guide](https://retrogamecorps.com/2022/03/13/android-emulation-starter-guide/), "Setup process" and "Prepare your ROM library" (secondary source, accessed 2026-08-05).
[^rgc-directories]: Retro Game Corps, [`ES-DE-Directories` README at `77626ca`](https://github.com/retrogamecorps/ES-DE-Directories/blob/77626ca25cdfbd8fe6b150d8d5fc171fc06607ba/README.md) (secondary convenience package).
[^ayn-odin3]: AYN, [AYN Odin 3 product page](https://www.ayntec.com/products/ayn-odin-3) and its [machine-readable product record](https://www.ayntec.com/products/ayn-odin-3.js) (primary source, accessed 2026-08-05).
[^ayn-firmware]: AYN, [Firmware page](https://www.ayntec.com/pages/software) (primary source, accessed 2026-08-05).
[^android-storage]: Android Developers, [Access app-specific files](https://developer.android.com/training/data-storage/app-specific) (primary platform documentation, accessed 2026-08-05).
[^android-saf]: Android Developers, [Access documents and other files from shared storage](https://developer.android.com/training/data-storage/shared/documents-files) (primary platform documentation, accessed 2026-08-05).
[^android-adoptable]: Android Open Source Project, [Adoptable storage](https://source.android.com/docs/core/storage/adoptable) (primary platform documentation, accessed 2026-08-05).
[^android-usb]: Google Android Help, [Transfer files between your computer & Android device](https://support.google.com/android/answer/9064445), "Move files with a USB cable" (primary product documentation, accessed 2026-08-05).
[^wpd-enumeration]: Microsoft, [Enumerating Content](https://learn.microsoft.com/en-us/windows/win32/wpd_sdk/enumerating-content), Windows Portable Devices documentation (primary host API documentation, accessed 2026-08-05).
[^esde-android-onboarding]: ES-DE, [Android documentation: First startup and onboarding at `9060f03d`](https://gitlab.com/es-de/emulationstation-de/-/blob/9060f03d1fb6595836924f070d8c36d38f4f6b82/ANDROID.md#first-startup-and-onboarding) (primary frontend documentation).
[^esde-android-storage]: ES-DE, [Android documentation: Splitting system directories across multiple storage devices at `9060f03d`](https://gitlab.com/es-de/emulationstation-de/-/blob/9060f03d1fb6595836924f070d8c36d38f4f6b82/ANDROID.md#splitting-system-directories-across-multiple-storage-devices) (primary frontend documentation).
[^esde-scoped-storage]: ES-DE, [Android documentation: Emulation on Android in general at `9060f03d`](https://gitlab.com/es-de/emulationstation-de/-/blob/9060f03d1fb6595836924f070d8c36d38f4f6b82/ANDROID.md#emulation-on-android-in-general) (primary frontend documentation).
[^esde-odin3]: ES-DE, [Android documentation: Known problems and Device compatibility at `9060f03d`](https://gitlab.com/es-de/emulationstation-de/-/blob/9060f03d1fb6595836924f070d8c36d38f4f6b82/ANDROID.md#ayn-odin-3) (primary frontend documentation).
[^esde-android-config]: ES-DE, [Android `es_systems.xml` at `9060f03d`](https://gitlab.com/es-de/emulationstation-de/-/blob/9060f03d1fb6595836924f070d8c36d38f4f6b82/resources/systems/android/es_systems.xml) (primary configuration).
[^esde-supported-systems]: ES-DE, [Android documentation: Supported game systems at `9060f03d`](https://gitlab.com/es-de/emulationstation-de/-/blob/9060f03d1fb6595836924f070d8c36d38f4f6b82/ANDROID.md#supported-game-systems) (primary frontend documentation).
[^esde-onboarding]: ES-DE, [User guide: Installation and first startup at `9060f03d`](https://gitlab.com/es-de/emulationstation-de/-/blob/9060f03d1fb6595836924f070d8c36d38f4f6b82/USERGUIDE.md#installation-and-first-startup) (primary frontend documentation).
[^esde-relative-root]: ES-DE, [User guide: Placing games into non-standard directories at `9060f03d`](https://gitlab.com/es-de/emulationstation-de/-/blob/9060f03d1fb6595836924f070d8c36d38f4f6b82/USERGUIDE.md#placing-games-into-non-standard-directories) (primary frontend documentation).
[^esde-network]: ES-DE, [User guide: Placing games and other resources on network shares at `9060f03d`](https://gitlab.com/es-de/emulationstation-de/-/blob/9060f03d1fb6595836924f070d8c36d38f4f6b82/USERGUIDE.md#placing-games-and-other-resources-on-network-shares) (primary frontend documentation).
[^esde-multifile]: ES-DE, [User guide: Multiple game files installation at `9060f03d`](https://gitlab.com/es-de/emulationstation-de/-/blob/9060f03d1fb6595836924f070d8c36d38f4f6b82/USERGUIDE.md#multiple-game-files-installation) and [Directories interpreted as files](https://gitlab.com/es-de/emulationstation-de/-/blob/9060f03d1fb6595836924f070d8c36d38f4f6b82/USERGUIDE.md#directories-interpreted-as-files) (primary frontend documentation).
[^esde-case]: ES-DE, [Android FAQ: standalone emulator access and lowercase directories at `9060f03d`](https://gitlab.com/es-de/emulationstation-de/-/blob/9060f03d1fb6595836924f070d8c36d38f4f6b82/FAQ-ANDROID.md#when-i-launch-a-game-using-a-standalone-emulator-why-does-it-say-the-game-file-could-not-be-opened) (primary frontend documentation).
[^esde-media]: ES-DE, [User guide: Manually copying game media files at `9060f03d`](https://gitlab.com/es-de/emulationstation-de/-/blob/9060f03d1fb6595836924f070d8c36d38f4f6b82/USERGUIDE.md#manually-copying-game-media-files) (primary frontend documentation).
[^esde-gamelist]: ES-DE, [User guide: Migrating from EmulationStation at `9060f03d`](https://gitlab.com/es-de/emulationstation-de/-/blob/9060f03d1fb6595836924f070d8c36d38f4f6b82/USERGUIDE.md#migrating-from-emulationstation) (primary frontend documentation).
[^esde-metadata]: ES-DE, [User guide: Metadata editor at `9060f03d`](https://gitlab.com/es-de/emulationstation-de/-/blob/9060f03d1fb6595836924f070d8c36d38f4f6b82/USERGUIDE.md#metadata-editor) (primary frontend documentation).
[^esde-performance]: ES-DE, [Android FAQ: startup performance at `9060f03d`](https://gitlab.com/es-de/emulationstation-de/-/blob/9060f03d1fb6595836924f070d8c36d38f4f6b82/FAQ-ANDROID.md#es-de-takes-a-very-long-time-to-start-is-there-a-way-to-improve-this) (primary frontend documentation).
[^daijisho]: Daijisho, [How to Use Daijisho wiki at `5166dd5`](https://github.com/TapiocaFox/Daijishou/wiki/How-to-Use-Daijish%C5%8D/5166dd5659dff40954bf0c7ac8eeaa8fb2dcbdbe), "Platforms", "Games", and "Import Scraped Media" (frontend-owned documentation).
[^pegasus-metadata]: Pegasus, [Metadata files](https://pegasus-frontend.org/docs/user-guide/meta-files/) ([documentation source repository at `6b32206`](https://github.com/mmatyas/pegasus-frontend/tree/6b322063a036db60cba5810fda82a3ce38f1e62f)) (primary frontend documentation, accessed 2026-08-05).
[^pegasus-android]: Pegasus, [Platform Notes: Android](https://pegasus-frontend.org/docs/user-guide/platform-android/) (primary frontend documentation, accessed 2026-08-05).
[^retroarch-playlists]: Libretro, [ROMs, Playlists, and Thumbnails](https://docs.libretro.com/guides/roms-playlists-thumbnails/) ([source repository at `f435a47`](https://github.com/libretro/docs/blob/f435a47d7ed39ccf989154afa0b12e7ff2b302f6/docs/guides/roms-playlists-thumbnails.md)) (official emulator/frontend documentation, accessed 2026-08-05).
