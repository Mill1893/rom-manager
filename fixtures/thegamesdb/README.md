# TheGamesDB fixtures

**Hand-authored from the public API schema. No response from TheGamesDB has
been recorded here, and none ever should be.**

Issue #29 is explicit that returned provider content is user-requested material
for private in-app display — it may not be bundled, checked in, put in
diagnostics, exported, or transferred to a Media Target. Committing a real
response would violate that in the most permanent way available, so these files
describe the *shape* the adapter must handle using invented content.

The same applies to artwork: any placeholder here is project-owned, not
provider-supplied.

| Fixture | Shape it exercises |
| --- | --- |
| `match-unique.json` | One Platform-consistent result — the only shape that may auto-match |
| `match-ambiguous.json` | Several candidates; must produce Suggestions, never a match |
| `not-found.json` | A definitive empty result, cacheable for 24 hours |
| `error-auth.json` | A rejected key |
| `error-quota.json` | Exhausted allowance, with `Retry-After` |
| `malformed.json` | Valid JSON in a shape the adapter cannot read |
| `allowance.json` | The non-consuming allowance endpoint |
