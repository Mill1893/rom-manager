# Issue tracker: GitHub

Issues and specs for this repo live as GitHub issues. Use the `gh` CLI for all operations.

## Conventions

- **Create an issue**: `gh issue create --title "..." --body "..."`.
- **Read an issue**: `gh issue view <number> --comments`, including labels.
- **List issues**: `gh issue list --state open --json number,title,body,labels,comments` with appropriate label and state filters.
- **Comment on an issue**: `gh issue comment <number> --body "..."`.
- **Apply or remove labels**: `gh issue edit <number> --add-label "..."` or `--remove-label "..."`.
- **Close an issue**: `gh issue close <number> --comment "..."`.

Infer the repository from `git remote -v`; `gh` does this automatically inside the clone.

## Pull requests as a triage surface

**PRs as a request surface: no.**

GitHub shares one number space across issues and pull requests. Resolve an ambiguous number with `gh pr view <number>` and fall back to `gh issue view <number>`.

## Publishing and fetching tickets

When a skill says to publish to the issue tracker, create a GitHub issue. When a skill says to fetch a ticket, run `gh issue view <number> --comments`.

## Wayfinding operations

The map is a single issue with child issues as tickets.

- **Map**: Create an issue labelled `wayfinder:map` containing Destination, Notes, Decisions so far, Not yet specified, and Out of scope sections.
- **Child ticket**: Link an issue to the map using GitHub sub-issues. If sub-issues are unavailable, add the child to a task list in the map body and put `Part of #<map>` at the top of the child body. Label it `wayfinder:research`, `wayfinder:prototype`, `wayfinder:grilling`, or `wayfinder:task`.
- **Blocking**: Prefer GitHub's native issue dependencies. Add an edge with `gh api --method POST repos/<owner>/<repo>/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-database-id>`, where the database id comes from `gh api repos/<owner>/<repo>/issues/<number> --jq .id`. If dependencies are unavailable, put `Blocked by: #<number>` at the top of the child body.
- **Frontier query**: List the map's open children, then exclude children with open blockers or assignees. The first remaining child in map order is next.
- **Claim**: Run `gh issue edit <number> --add-assignee @me` before doing any work.
- **Resolve**: Post the answer as a resolution comment, close the issue, then append a one-line gist and link under the map's Decisions so far section.
