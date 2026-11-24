# Teams and group grants

A team is a named group of people that a share can be granted to. A group
grant gives Reader on a share to a team or to everyone signed in. This page
gives the life of a team, of a member and of a group grant.

## Deleting a team

- `flare teams delete <name> --yes` removes the team's grant from every
  share it was granted on, then removes its members, then removes the team.
  The name is then free, and a new team of that name gets none of the old
  team's access.
- The server finds the team's grants through the team's list of grants
  across shares, its team-to-shares index (see Stored items below).
- The delete first marks the team `deleting`. From then on the team opens
  nothing, and its name stays taken until the delete finishes.
- One `DELETE /api/teams/<name>` removes at most 200 grants and answers
  `finished: false` when more remain. `flare teams delete` repeats the call
  until the answer says `finished: true`, at most 1,000 times (200,000
  grants), and otherwise names the command that goes on.
- A delete that fails midway answers `flare::teams::unavailable` and leaves
  the team `deleting`. The owner runs the same delete again, and it goes on
  from the grants that remain.
- The team's activity index entries for each share go before that share's
  grant does, so a delete that stops leaves every remaining grant listed
  for the repeat.

## Transferring a team

- `flare teams transfer <name> --to <person> --yes` hands the team to one
  of its members, who becomes its owner. The former owner stays a member
  and may leave.
- Only the owner transfers. A member gets `flare::teams::forbidden` (403),
  and anyone outside the team gets `flare::teams::not_found` (404).
- A person who is not a member gets `flare::teams::not_a_member` (409). The
  CLI's recovery is `flare teams add <name> --with <person>`.
- A new owner who already owns 20 active teams gets
  `flare::teams::too_many_owned` (409), and the team keeps its owner.
- A repeat that finds the person already the owner answers the team as it
  is.

## Commands and routes

| Command | Route | Who may |
| --- | --- | --- |
| `flare teams create <name>` | `POST /api/teams` | A person the server's `creators` setting allows |
| `flare teams add <name> --with <people>` | `POST /api/teams/<name>/members` | The team's owner |
| `flare teams remove <name> --with <people> --yes` | `DELETE /api/teams/<name>/members` | The team's owner, or a member removing only themself |
| `flare teams list` | `GET /api/teams` | Anyone signed in; it lists their own teams |
| `flare teams show <names>` | `GET /api/teams/<name>` | The team's owner and members |
| `flare teams transfer <name> --to <person> --yes` | `PUT /api/teams/<name>/owner` | The team's owner |
| `flare teams delete <name> --yes` | `DELETE /api/teams/<name>` | The team's owner |
| `flare share` or `flare grant` with `--with team:<name>` | `POST /api/shares`, `POST /api/grants/<id>` | The share's owner; an Admin who is a member of the team |
| `flare share` or `flare grant` with `--with everyone`, or `flare grant` with `--everyone` | the same | The share's owner |
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
- `flare grants` and `GET /api/grants/<id>` show a share's Admin the grant
  to a team the Admin does not belong to as `team:*`, so no one outside a
  team learns its name. The share's owner sees every name.
- A name that is not a team the caller belongs to answers
  `flare::teams::not_found` (404). The CLI's recovery is
  `flare teams list`, and its alternative is to ask the team's owner to add
  the caller.
- The owner who tries to leave their own team gets
  `flare::teams::owner_cannot_leave` (409). The owner transfers the team
  to another member first, or deletes the team. The CLI's recovery is
  `flare teams delete <name> --yes`.

## Limits

| Limit | Counts | When it is hit |
| --- | --- | --- |
| 256 members | The people in one team, the owner included | `flare::teams::too_many_members` (409); the add writes nothing |
| 20 teams owned | The active teams one person owns | `flare::teams::too_many_owned` (409); the create or the transfer writes nothing |
| 32 teams joined | The active teams one person belongs to | `flare::teams::too_many_joined` (409); the add writes nothing |
| 200 grants a call | The grants one `DELETE /api/teams/<name>` removes | The answer says `finished: false`; the next call goes on |
| 1,000 calls | The delete calls one `flare teams delete` makes | The command prints `finished: false` and names itself as the next step |

- The owner frees room by removing members or deleting a team.
- The owned and joined counts are read before the write. Writes that race
  can each pass the check, and each racing write can take the person one
  team over the limit.
- A team may hold grants on any number of shares. Each share holds at most
  256 grants, and each team grant counts as one of them.

## Who a group grant opens a share to

- A team grant opens the share to each present member of the team while the
  team is active.
- An everyone grant stores the audience that the server's admit rule had
  when the grant was made:

| Admit rule when granted | Stored audience | Opens the share to |
| --- | --- | --- |
| `domain` | that domain | Each signed-in address at that domain |
| `emails` | `admitted` | Each signed-in person, while the admit rule is `emails`; adding an address to the list widens the grant to that person |
| `anyone` | none | Nothing; the grant is refused with `flare::grants::everyone_unbounded` (409) |

- Sign-in still applies: the admit rule decides who can sign in, and a
  group grant never opens a share to a person who cannot sign in.
- A person's direct grant, their team grants and an everyone grant all give
  Reader at most. The highest role a person holds through any of them
  applies.
- Activity reads a team's and everyone's activity index entries as well as
  the caller's own, and checks the caller's standing on each flare and
  each response. [activity.md](activity.md) gives the group entries.

## Team

| State | Entered when | Leaves to | Stored |
| --- | --- | --- | --- |
| absent | Always, and again once a delete finishes | active | Nothing |
| active | A creator creates it with a name no team has | deleting | `TEAM#<name>` `META` with owner, created, state and member count |
| deleting | The owner deletes it | absent | The same item with state `deleting`, while the grants and then the member pairs are removed |

- A transfer changes the owner of an active team and leaves it active.

## Member pair

| State | Entered when | Leaves to | Stored |
| --- | --- | --- | --- |
| absent | Always | present | Nothing |
| present | The create adds the owner, or the owner adds the person | absent | `TEAM#<name>` `M#<email>` and `MEMBER#<email>` `T#<name>`, written in one transaction with the member count |
| absent | The owner removes the person, the person leaves, or a delete finishes | present, after a new add to an active team | Both items deleted in one transaction with the member count |

- The owner cannot leave their own team, and the attempt answers
  `flare::teams::owner_cannot_leave`. The owner transfers or deletes the
  team instead.

## Group grant

| State | Entered when | Leaves to | Stored |
| --- | --- | --- | --- |
| absent | Always | active | Nothing |
| active | The share's owner, or an allowed Admin, grants a team or everyone | absent | `G#team:<name>` or `G#everyone` on the share, plus an entry in the team-to-shares index under `TEAM#<name>`, or under `EVERYONE`, written in one transaction; an everyone grant and its entry hold the audience |
| absent | The grant is revoked, or the team's delete removes it | active, after a new grant | The grant and its index entry deleted in one transaction |

- While its team is `deleting`, a team grant opens nothing, and
  `flare grants <id>` shows it with state `dormant` until the delete
  removes it.

## Stored items

| Item | Key | Holds | Written | Removed |
| --- | --- | --- | --- | --- |
| Team-to-shares index entry | `TEAM#<name>`, `E#<inverted grant time>#<share id>` | The role (always Reader), the granter's address and the grant time | With the team grant, in one transaction | With the grant, by a revoke or by the team's delete |
| Team activity entry | `ACT#team:<name>`, `A#<flare>#<position>`, and its marker `R#<response>#<flare>` | The response, the flare, the flare's version and the link time | When the team holds both a response and the flare it responds to | When the team's grant on the response ends |
| Everyone activity entry | `ACT#everyone`, as above | As above | When everyone holds both shares | When the everyone grant on the response ends |

- The server reads the team-to-shares index to list a member's shares with
  `flare shares --with-me`, and to find the grants a delete removes. No
  route returns the index itself.
- An operator with access to the AWS account can read every item. The
  granter's address in an index entry is personal data.

## Crash points

1. A create whose outcome is unknown is repeated. The team item and the
   owner's member pair go in one transaction, so a repeat finds the team
   whole or finds nothing. A repeat by the team's owner answers the active
   team as it is. A create by anyone else of a name in use answers
   `flare::teams::name_taken`.
2. An add, remove or transfer whose outcome is unknown is repeated. The two
   pair items and the count change together, so they never disagree, and a
   repeat of a finished add, remove or transfer changes nothing.
3. A delete that stops after it marks the team `deleting` answers
   `flare::teams::unavailable`. The team opens nothing. The owner repeats
   the delete, which removes the grants that remain, then the member pairs,
   then the team.
4. A delete that stops after removing a share's activity entries and
   before removing that share's grant leaves the grant listed, and the
   repeat removes it.
5. A grant and its index entry are written in one transaction, and a
   revoke deletes both in one transaction, so an index entry never
   outlives its grant.
6. A team grant reads the team's creation time before and after its
   write. When the team was deleted in between, the grant removes itself
   and answers `flare::teams::not_found`. When that removal fails too, the
   grant stays under a name with no team; the next create of that name
   removes every grant still listed under it before it creates the team.

## Invariants

- Each request reads standing with consistent reads, so a removal, a
  transfer or a delete applies to the next request.
- A group grant never opens a share to a person the admit rule does not
  admit. An everyone grant also checks the audience stored on the grant.
- A team created with a freed name holds no grant that an earlier team of
  that name held.
- An everyone grant gives Reader only, and only the share's owner makes or
  ends it.
