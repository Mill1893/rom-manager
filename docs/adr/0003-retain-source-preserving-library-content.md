# Retain source-preserving app-owned Library content

Successful imports copy strongly verified source objects into durable app-owned storage, preserving archives and loose files unchanged rather than depending on external locations or retaining only normalized ROM bytes. This makes Library and ROM Pack availability independent of Import Folders while preserving provenance, unknown members, and the ability to re-index content as format support improves; the accepted cost is storing original packaging and materializing ROM bytes when needed.

Exact duplicate source objects share storage, but differently packaged sources remain distinct even when they expose the same ROM. ROM content identity, not a Source Occurrence, is canonical; derived materializations are verified disposable cache, and loss of all healthy managed occurrences makes dependent content unavailable without silently changing its identity or ROM Pack selections.
