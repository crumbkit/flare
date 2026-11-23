# Teams and group grants

A team is a named group of people that a share can be granted to. A group
grant gives Reader on a share to a team or to everyone signed in. This page
gives the life of a team, of a member and of a group grant.

## Deleting a team

- Deleting a team leaves its grants on each share as dormant grants.
- A dormant grant opens the share to nobody.
- `flare grants <id>` lists a dormant grant first, marked `dormant`.
- The share's owner removes a dormant grant with
  `flare revoke <id> --with team:<name> --yes`.
- A team name is never reused. No one can create a team with the name of a
  deleted team, so no new team can take over the old team's grants.
- `flare teams delete` says both facts in its output.

## Commands and routes

| Command | Route | Who may |
| --- | --- | --- |
| `flare teams create <name>` | `POST /api/teams` | A person the server's `creators` setting allows |
| `flare teams add <name> --with <people>` | `POST /api/teams/<name>/members` | The team's owner |
| `flare teams remove <name> --with <people> --yes` | `DELETE /api/teams/<name>/members` | The team's owner, or a member removing only themself |
| `flare teams list` | `GET /api/teams` | Anyone signed in; it lists their own teams |
| `flare teams show <names>` | `GET /api/teams/<name>` | The team's owner and members |
| `flare teams delete <name> --yes` | `DELETE /api/teams/<name>` | The team's owner |
| `flare share` or `flare grant` with `--with team:<name>` | `POST /api/shares`, `POST /api/grants/<id>` | The share's owner; an Admin who is a member of the team |
| `flare share` or `flare grant` with `--everyone` | the same, with `everyone: true` | The share's owner |
| `flare revoke <id> --with <who> --yes` | `DELETE /api/grants/<id>` | The owner for any grant; a person for their own grant; an Admin for the person grants an Admin may end and for a team the Admin belongs to |
| `flare grants <id>` | `GET /api/grants/<id>` | The share's owner and its Admins |
| `flare shares --with-me --everyone` | `GET /api/shares?scope=with-me&everyone=true` | Anyone signed in |

- A team name has 1 to 40 characters from `a` to `z`, `0` to `9` and `-`.
- A server whose teams come from another directory answers each team
  change with `flare::teams::managed_elsewhere` (403).
- `flare shares --with-me` lists shares granted to the caller directly and
  through their active teams. It adds the shares granted to everyone only
  with `--everyone`.
- A revoke of a grantee that holds no grant changes nothing and succeeds.

## Limits

| Limit | Counts | When it is hit |
| --- | --- | --- |
| 256 members | The people in one team, the owner included | `flare::teams::too_many_members` (409); the add writes nothing |
| 20 teams owned | The active teams one person owns | `flare::teams::too_many_owned` (409); the create writes nothing |
| 32 teams joined | The active teams one person belongs to | `flare::teams::too_many_joined` (409); the add writes nothing |

- The owner frees room by removing members or deleting a team.
- The owned and joined counts are read before the write. Two writes that
  race can each pass the check, so a person can end one over a limit.

## Who a group grant opens a share to

- A team grant opens the share to each present member of the team while the
  team is active.
- An everyone grant stores the audience that the server's admit rule had
  when the grant was made:

| Admit rule when granted | Stored audience | Opens the share to |
| --- | --- | --- |
| `domain` | that domain | Each signed-in address at that domain |
| `emails` | `admitted` | Each signed-in person, while the admit rule is `emails` |
| `anyone` | none | Nothing; the grant is refused with `flare::grants::everyone_unbounded` (409) |

- Sign-in still applies: the admit rule decides who can sign in, and a
  group grant never opens a share to a person who cannot sign in.
- A person's direct grant, their team grants and an everyone grant all give
  Reader at most. The highest role a person holds through any of them
  applies.
- Activity on a flare checks the caller's standing on each flare and each
  response, so a group grant opens the responses it covers.

## Team

| State | Entered when | Leaves to | Stored |
| --- | --- | --- | --- |
| absent | Always, until a create | active | Nothing |
| active | A creator creates it with a name never used before | deleted | `TEAM#<name>` `META` with owner, created, state and member count |
| deleted | The owner deletes it | none | The same item with state `deleted`; the member pairs are removed |

## Member pair

| State | Entered when | Leaves to | Stored |
| --- | --- | --- | --- |
| absent | Always | present | Nothing |
| present | The create adds the owner, or the owner adds the person | absent | `TEAM#<name>` `M#<email>` and `MEMBER#<email>` `T#<name>`, written in one transaction with the member count |
| absent | The owner removes the person, the person leaves, or the team is deleted | present, after a new add to an active team | Both items deleted in one transaction with the member count |

- The owner cannot leave their own team. The owner deletes the team
  instead.

## Group grant

| State | Entered when | Leaves to | Stored |
| --- | --- | --- | --- |
| absent | Always | active | Nothing |
| active | The share's owner, or an allowed Admin, grants a team or everyone | dormant, absent | `G#team:<name>` or `G#everyone` on the share, plus a listing entry under `TEAM#<name>` or `EVERYONE`, written in one transaction; an everyone grant and its entry hold the audience |
| dormant | The team is deleted | absent | The grant stays and opens nothing |
| absent | The grant is revoked | active, after a new grant | The grant and its listing entry deleted in one transaction |

## Crash points

1. A create whose outcome is unknown is repeated. The team item and the
   owner's member pair go in one transaction, so a repeat finds the team
   whole or finds nothing.
2. An add or remove whose outcome is unknown is repeated. The two pair
   items and the count change together, so they never disagree, and a
   repeat of a finished add or remove changes nothing.
3. A delete that marks the team deleted and stops before removing the
   member pairs leaves the grants dormant already. The owner repeats the
   delete, which removes the remaining pairs.
4. A grant and its listing entry are written in one transaction, and a
   revoke deletes both in one transaction, so a listing entry never
   outlives its grant.

## Invariants

- Each request reads standing with consistent reads, so a removal or a
  delete applies to the next request.
- A group grant never opens a share to a person the admit rule does not
  admit. An everyone grant also checks the audience stored on the grant.
- A deleted team's name is never reused.
- An everyone grant gives Reader only, and only the share's owner makes or
  ends it.
