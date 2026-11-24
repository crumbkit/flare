//! A team's item and member pairs, to and from DynamoDB's attribute maps,
//! and the transactions that write them.

use crate::dynamodb::items::{
    Item, ItemKind, Malformed, attr, keyed, number, read_number, read_text, text,
};
use crate::dynamodb::keys::Key;
use crate::dynamodb::transact::{
    ABSENT, ItemRole, PRESENT, Transaction, delete, key_map, names, put, values,
};
use aws_sdk_dynamodb::types::{AttributeValue, ConditionCheck, Delete, TransactWriteItem, Update};
use flare_core::{Email, MAX_MEMBERS, Team, TeamName, TeamState, Timestamp};

const ACTIVE: &str = "active";
const DELETING: &str = "deleting";

fn state_name(state: TeamState) -> &'static str {
    match state {
        TeamState::Active => ACTIVE,
        TeamState::Deleting => DELETING,
    }
}

/// A team's item.
pub(super) fn team_item(team: &Team) -> Item {
    keyed(
        &Key::team(team.name()),
        [
            (attr::OWNER, text(team.owner().as_str())),
            (attr::CREATED, number(team.created().as_secs())),
            (attr::STATE, text(state_name(team.state()))),
            (
                attr::COUNT,
                number(u64::try_from(team.members()).unwrap_or(u64::MAX)),
            ),
        ],
    )
}

/// The team that the item of team `name` holds.
pub(super) fn team_from(item: &Item, name: &TeamName) -> Result<Team, Malformed> {
    let kind = ItemKind::Team;
    let owner =
        Email::try_from(read_text(item, attr::OWNER, kind)?).map_err(|_| Malformed(kind))?;
    let state = match read_text(item, attr::STATE, kind)? {
        ACTIVE => TeamState::Active,
        DELETING => TeamState::Deleting,
        _ => return Err(Malformed(kind)),
    };
    let members =
        usize::try_from(read_number(item, attr::COUNT, kind)?).map_err(|_| Malformed(kind))?;
    Ok(Team::new(
        name.clone(),
        owner,
        Timestamp::from_secs(read_number(item, attr::CREATED, kind)?),
        state,
        members,
    ))
}

/// Puts both items of the pair of `member` in `team`, each only if absent.
fn put_pair(
    transaction: &mut Transaction,
    table: &str,
    team: &TeamName,
    member: &Email,
) -> Option<()> {
    let (in_team, in_person) = Key::member_pair(team, member);
    for key in [in_team, in_person] {
        transaction.push(
            ItemRole::MemberItem,
            put(table, keyed(&key, []), Some(ABSENT))?,
        );
    }
    Some(())
}

/// The transaction of a create: the team's item and its owner's pair, each
/// only if absent, so a name that was ever used refuses it.
pub(super) fn create(table: &str, team: &Team) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    transaction.push(
        ItemRole::TeamItem,
        put(table, team_item(team), Some(ABSENT))?,
    );
    put_pair(&mut transaction, table, team.name(), team.owner())?;
    Some(transaction)
}

/// An update of the team's item with `expression`, under `condition`.
fn update_team(
    table: &str,
    team: &TeamName,
    (expression, condition): (&str, &str),
    name_pairs: &[(&str, &str)],
    value_pairs: Vec<(&str, AttributeValue)>,
) -> Option<TransactWriteItem> {
    let update = Update::builder()
        .table_name(table)
        .set_key(Some(key_map(&Key::team(team))))
        .update_expression(expression)
        .condition_expression(condition)
        .set_expression_attribute_names(Some(names(name_pairs)))
        .set_expression_attribute_values(Some(values(value_pairs)))
        .build()
        .ok()?;
    Some(TransactWriteItem::builder().update(update).build())
}

/// The transaction of an add: one more in the count of an active team below
/// [`MAX_MEMBERS`], and both items of the pair, each only if absent.
pub(super) fn add(table: &str, team: &TeamName, member: &Email) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    transaction.push(
        ItemRole::TeamItem,
        update_team(
            table,
            team,
            ("ADD #n :one", "#s = :active AND #n < :max"),
            &[("#n", attr::COUNT), ("#s", attr::STATE)],
            vec![
                (":one", number(1)),
                (":active", text(ACTIVE)),
                (":max", number(u64::try_from(MAX_MEMBERS).ok()?)),
            ],
        )?,
    );
    put_pair(&mut transaction, table, team, member)?;
    Some(transaction)
}

/// The transaction of a remove: both items of the pair, the first only if
/// present, and one less in the team's count.
pub(super) fn remove(table: &str, team: &TeamName, member: &Email) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    let (in_team, in_person) = Key::member_pair(team, member);
    let first = Delete::builder()
        .table_name(table)
        .set_key(Some(key_map(&in_team)))
        .condition_expression(PRESENT)
        .build()
        .ok()?;
    transaction.push(
        ItemRole::MemberItem,
        TransactWriteItem::builder().delete(first).build(),
    );
    transaction.push(
        ItemRole::MemberItem,
        TransactWriteItem::builder()
            .delete(delete(table, &in_person)?)
            .build(),
    );
    transaction.push(
        ItemRole::TeamItem,
        update_team(
            table,
            team,
            ("ADD #n :less", PRESENT),
            &[("#n", attr::COUNT)],
            vec![(":less", AttributeValue::N("-1".to_owned()))],
        )?,
    );
    Some(transaction)
}

/// The transaction that marks a team deleting.
pub(super) fn mark_deleting(table: &str, team: &TeamName) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    transaction.push(
        ItemRole::TeamItem,
        update_team(
            table,
            team,
            ("SET #s = :deleting", PRESENT),
            &[("#s", attr::STATE)],
            vec![(":deleting", text(DELETING))],
        )?,
    );
    Some(transaction)
}

/// The transaction that removes a deleting team with no members, which
/// frees its name.
pub(super) fn forget(table: &str, team: &TeamName) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    let removed = Delete::builder()
        .table_name(table)
        .set_key(Some(key_map(&Key::team(team))))
        .condition_expression("#s = :deleting AND #n = :none")
        .set_expression_attribute_names(Some(names(&[("#s", attr::STATE), ("#n", attr::COUNT)])))
        .set_expression_attribute_values(Some(values(vec![
            (":deleting", text(DELETING)),
            (":none", number(0)),
        ])))
        .build()
        .ok()?;
    transaction.push(
        ItemRole::TeamItem,
        TransactWriteItem::builder().delete(removed).build(),
    );
    Some(transaction)
}

/// The transaction of a transfer: the owner of an active team that `from`
/// owns becomes `to`, while `to`'s member pair is present.
pub(super) fn transfer(
    table: &str,
    team: &TeamName,
    from: &Email,
    to: &Email,
) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    transaction.push(
        ItemRole::TeamItem,
        update_team(
            table,
            team,
            ("SET #o = :to", "#s = :active AND #o = :from"),
            &[("#o", attr::OWNER), ("#s", attr::STATE)],
            vec![
                (":to", text(to.as_str())),
                (":from", text(from.as_str())),
                (":active", text(ACTIVE)),
            ],
        )?,
    );
    let (in_team, _) = Key::member_pair(team, to);
    let check = ConditionCheck::builder()
        .table_name(table)
        .set_key(Some(key_map(&in_team)))
        .condition_expression(PRESENT)
        .build()
        .ok()?;
    transaction.push(
        ItemRole::MemberItem,
        TransactWriteItem::builder().condition_check(check).build(),
    );
    Some(transaction)
}
