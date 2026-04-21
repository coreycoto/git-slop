# Backlog Project Contract

`git-slop` uses a single GitHub Project named `git-slop`.

Project rules:

- views: `Backlog`, `Epics`
- fields: `Status`, `Priority`, `Queue Order`
- `Status` options: `Todo`, `In Progress`, `Done`
- `Priority` options: `Now`, `Next`, `Later`
- epics stay in the project, may carry `Priority`, and are not queue-ordered work items
- roadmap phases live as epic issues, not as milestones
- milestones are quarter commitments only

Operational note:

- `Status` is a built-in GitHub Projects field and should be reused
- `Priority` and `Queue Order` are the custom fields this repo expects to create or sync
- epics must not receive `Queue Order`
