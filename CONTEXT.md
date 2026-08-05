# ROM Management

The domain of maintaining a canonical game-file collection and curating selections of it for removable-media gaming devices.

## Language

**Library**:
The canonical collection of user-supplied Games and their playable content managed on the desktop.
_Avoid_: Collection, catalog

**Game**:
A playable title scoped to one Platform and shown as one Library entry. Releases and other playable forms of the title belong to the Game rather than appearing as separate Games. A Game has stable local identity across metadata corrections and may be provisional when its identity is supplied locally rather than established by a catalog match.
_Avoid_: Title, Library item

**Release**:
An identifiable official or unofficial version of a Game, distinguished by facts such as region, language, revision, build, demo, prototype, or modification. A Release has stable local identity across metadata corrections. A derived Release retains its exact base and Patch lineage. Dump quality, archive encoding, and image format do not create a Release.
_Avoid_: Edition, ROM version

**Patch**:
A representation-aware transformation that derives a Release from an exact base. A patched Release is eligible for a ROM Pack only after the output ROM Set has been materialized and given strong content identity.
_Avoid_: Mod, ROM hack file

**ROM Set**:
The expected content for one runnable representation of one Release, including ordered multi-file or multi-disc content and required dependencies. Its exact content identity is the ordered membership, roles, and dependency structure of its ROMs. A ROM Set may be retained while incomplete but cannot belong to a ROM Pack until complete. Materially different representations are distinct but related ROM Sets, and distinct Releases retain distinct ROM Sets even when they reuse identical ROMs.
_Avoid_: Playable variant, ROM bundle

**ROM**:
A representation-aware content component of one or more ROM Sets. Exact ROM identity requires the same representation and byte content; names, paths, archive bytes, and catalog matches are not proof of equality.
_Avoid_: ROM file, source file

**Source Occurrence**:
A filesystem file or archive member from which a ROM's bytes can be read. Multiple Source Occurrences may supply the same ROM.
_Avoid_: ROM, duplicate ROM

**Source Container**:
Packaging, such as an archive, that exposes one or more Source Occurrences while retaining its own provenance. A Source Container is not a Game, Release, ROM Set, or ROM identity.
_Avoid_: ROM Set, bundle

**ROM Pack**:
A curated selection of exact, complete ROM Sets from the Library intended for transfer to one or more devices. It may contain multiple ROM Sets from one Game, including locally identified content, and includes the deduplicated closure of their required dependencies.
_Avoid_: Playlist, bundle

**Device Profile**:
Reusable, explicit Platform folder and layout rules that describe how a kind of device expects ROMs to be arranged. A Device Profile may serve many Media Targets, while each Media Target has one active Device Profile.
_Avoid_: Device preset, target configuration

**Media Target**:
A selected storage root with stable application identity that is reconciled with a ROM Pack using one active Device Profile. The same Media Target may be reached through multiple Transport Bindings without becoming a different destination.
_Avoid_: Device, media, destination

**Transport Binding**:
A remembered means of reaching a Media Target through a particular transport. It carries connection-specific identity, location evidence, and observed capabilities but is not itself the Media Target.
_Avoid_: Connection, mount path

**Destination Role**:
A named purpose fulfilled by a Media Target within a Device Profile, such as ROM content or frontend metadata. Roles on different physical storage roots belong to separate Media Targets even when they share a Device Profile.
_Avoid_: Storage type, target type

**Platform**:
An emulated game system used to classify games and determine their target folder, including consoles, handhelds, arcade systems, and home computers.
_Avoid_: Console, system

**Catalog Match**:
An accepted relationship between a local Game, Release, or ROM Set and a provider catalog assertion. It records whether acceptance was automatic from exact evidence or confirmed by the user and remains distinct from ROM content identity.
_Avoid_: Metadata match, verified ROM

**Match Suggestion**:
An unaccepted provider candidate produced from filename, title, partial-set, conflicting, or other uncertain evidence. It does not change local identity until the user accepts it as a Catalog Match.
_Avoid_: Possible match

**Local Override**:
An authoritative user-supplied metadata value or semantic relationship that takes precedence over provider data until the user explicitly reverts it.
_Avoid_: Manual edit

**Managed ROM**:
A ROM on a Media Target whose relationship to the Library and prior synchronization is known strongly enough for the application to update or remove it safely.
_Avoid_: Tracked file

**Sync Plan**:
A preview of the additions, retained files, and removals needed to make a Media Target match a selected ROM Pack.
_Avoid_: Transfer queue, sync job
