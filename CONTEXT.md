# ROM Management

The domain of maintaining a canonical game-file collection and curating selections of it for removable-media gaming devices.

## Language

**Library**:
The canonical collection of user-supplied Games and their playable content managed on the desktop. Successfully imported content remains available from durable app-owned storage without depending on its external source location.
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
The expected content for one runnable representation of one Release, including ordered multi-file or multi-disc content and required dependencies. Its exact content identity is the ordered membership, roles, and dependency structure of its ROMs. A ROM Set is incomplete when expected structure or membership is absent, unavailable when complete but any required ROM or dependency is unavailable, and available only when its complete dependency closure can be materialized. Materially different representations are distinct but related ROM Sets, and distinct Releases retain distinct ROM Sets even when they reuse identical ROMs.
_Avoid_: Playable variant, ROM bundle

**ROM**:
A representation-aware content component of one or more ROM Sets. Exact ROM identity requires the same representation semantics, behavior-bearing structure, and logical byte content; source compression layout, descriptor formatting, names, paths, archive bytes, and catalog matches are not proof of equality. No Source Occurrence is canonical: a ROM is available when at least one healthy managed occurrence can reproduce it and unavailable otherwise.
_Avoid_: ROM file, source file

**Source Occurrence**:
A filesystem file or archive member from which a ROM's bytes can be read. Multiple Source Occurrences may supply the same ROM. Library availability depends only on occurrences retained in app-owned storage, not on external locations from which they were imported.
_Avoid_: ROM, duplicate ROM

**Source Container**:
Packaging, such as an archive, that exposes one or more Source Occurrences while retaining its own provenance. Imported Source Containers are retained unchanged in app-owned storage; extracted or transformed content is derived materialization. A Source Container is not a Game, Release, ROM Set, or ROM identity.
_Avoid_: ROM Set, bundle

**Import Folder**:
A remembered external filesystem root scanned only on explicit request to discover import candidates. It may provide an authoritative default Platform, but traversal does not follow filesystem indirections. Moving, changing, or losing an Import Folder does not alter content already retained in the Library.
_Avoid_: Watched folder, Library folder

**Import Candidate**:
An external file or Source Container discovered for possible import. It is not Library content until its Platform and ROM Set grouping are established and its bytes are copied, strongly verified, and committed to app-owned storage.
_Avoid_: Pending ROM, Library content

**Origin Observation**:
Provenance evidence that an external location contained a particular import candidate during a completed scan. It may later be moved, missing, or superseded without changing the identity or availability of imported Library content.
_Avoid_: Source Occurrence, managed copy

**Materialization Cache**:
Disposable, strongly verified derived bytes that accelerate reading a ROM or producing a target form from durable Library content. Cache presence does not establish ROM availability, and eviction does not change Library identity or state.
_Avoid_: Library storage, source copy

**ROM Pack**:
A curated selection of exact, complete ROM Sets from the Library intended for transfer to one or more devices. It may contain multiple ROM Sets from one Game, including locally identified content, and includes the deduplicated closure of their required dependencies. Its exact selections survive metadata corrections and loss of availability; unavailable content blocks planning and sync rather than causing substitution or deselection.
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
A Target Artifact whose relationship to the Library was established by verified application placement or user-approved adoption. Recognition without adoption does not grant authority to remove it.
_Avoid_: Tracked file

**Managed Artifact Manifest**:
A versioned record of Managed ROM authority and verification evidence stored on its Media Target and mirrored in the local Library. Disagreement between the copies prevents destructive action until the affected content is reconciled explicitly.
_Avoid_: Sync database, target cache

**Target Artifact**:
The concrete bytes at a canonical relative path produced from Library content for one Media Target under its active Device Profile. Retention requires both strong equality with the expected bytes and canonical placement; equal bytes elsewhere are a relocation candidate or duplicate rather than the expected Target Artifact.
_Avoid_: Target file, transferred ROM

**Metadata Projection**:
The export-eligible effective Library fields mapped to one frontend entry for a canonical Target Artifact path under a Device Profile. Management authority and desired state apply to individual mapped fields, while the containing frontend document remains shared with frontend-owned and user-edited state.
_Avoid_: Generated metadata file, managed gamelist

**Sync Plan**:
An immutable preview of the additions, replacements, retentions, adoptions, and removals needed to make the Target Artifacts and Metadata Projections applicable to one Media Target match a selected ROM Pack under its active Device Profile. A Sync Plan applies to exactly one Media Target; unrecognized content and shared frontend state are preserved and shown separately.
_Avoid_: Transfer queue, sync job

**Plan Approval**:
The single-use authority to execute one exact Sync Plan, binding that plan's digest, the acknowledged count of permanent managed removals, and the Media Target, Device Profile, Transport Binding, and inventory evidence it was built against. An approval is invalidated by evidence that any of those changed, never by elapsed time, and it is the sole authority for the adoptions its plan names.
_Avoid_: Confirmation, consent token, permission

**Effective-Equivalence Key**:
The comparison key under which two target paths may resolve to the same object on some supported host. It folds case, Unicode normalization, and the trailing dots and spaces Win32 path parsing discards, and is deliberately a conservative superset of any one host's lookup rule — it may report a collision the host would not, but must never miss one it would. It informs planning; it never replaces an atomic create-if-absent as proof that a path was free.
_Avoid_: Normalized path, canonical key, path hash

**Residue**:
Content left at a named path by an interrupted operation that the application could not verify as its own, and therefore did not delete. Residue is disclosed to the user and carries no management authority: on the next planning pass it is simply content the Managed Artifact Manifest does not name.
_Avoid_: Orphan, temp file, leftover
