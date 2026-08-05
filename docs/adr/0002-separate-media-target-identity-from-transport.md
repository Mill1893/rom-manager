# Separate Media Target identity from transport

A Media Target represents one selected storage root and has stable application identity anchored by an app-owned marker; it is not identified by a mount path, drive letter, MTP object ID, device name, or connection. The same Media Target may have multiple Transport Bindings, each with connection-specific identity evidence and observed capabilities, while one active Device Profile supplies portable, relative layout rules. This separation allows removable storage to remain the same destination when its path or transport changes without trusting weak locators or pretending filesystem and MTP behavior are equivalent.

## Consequences

First-time binding, ambiguous matches, missing markers, and marker conflicts require explicit user confirmation. Device Profiles cannot contain target-specific paths or identifiers, and separate physical storage roots remain separate Media Targets even when they use the same profile. Disconnection, profile changes, and interrupted operations invalidate current planning state until the target is refreshed.
