//! Contract suites that every implementation of a flare trait runs.
//!
//! Each suite is a set of public async functions. A function takes an
//! implementation and a small fixture, checks one clause of the trait's
//! contract and returns `Result<(), Violation>`. A function returns an error
//! and never panics, because this is library code (BASE-4). The calling test
//! unwraps.
//!
//! Run each function against a new store, seeded with the records that the
//! fixture's `seeds` lists. A later crate calls every function of a suite
//! from its own tests, on its own executor.
//!
//! The functions whose names start with `concurrent` take a [`RunTogether`]
//! runner, which drives two calls so that they run at the same moment, and
//! they await the future that it returns. Pass [`Threads`] to run each call
//! on its own thread with [`block_on`]. A store whose futures need the
//! context of a runtime passes a runner of its own that awaits both calls on
//! that runtime, for example with the runtime's `join`, and runs the clause
//! on that runtime too.

use std::fmt;

pub mod authenticator;
mod executor;
pub mod file_store;
pub mod grant_store;
pub mod page_store;
pub mod record_store;
pub mod response_store;
pub mod share_store;
pub mod team_store;
pub mod token_store;

pub use executor::{RunTogether, Threads, block_on};

/// Defines [`Clause`] with one sentence per clause.
macro_rules! clauses {
    ($($variant:ident => $text:literal,)*) => {
        /// A clause of a trait's contract.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[non_exhaustive]
        pub enum Clause {
            $(#[doc = $text] $variant,)*
        }

        impl fmt::Display for Clause {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(match self { $(Self::$variant => $text,)* })
            }
        }
    };
}

clauses! {
    ReadReturnsWhatWasSeeded => "a read returns the record that was seeded",
    WriteReturnsANewBlobAndReplacesTheBody => "a write with the current blob returns a new blob and replaces the body",
    AStaleBlobConflictsAndChangesNothing => "a write with a stale blob conflicts and leaves the record unchanged",
    AnOldBlobConflictsAfterAWrite => "a write with the blob from before an earlier write conflicts",
    ABlobNeverRepeatsForOneRecord => "a blob never repeats for one record, so a write that names the first blob conflicts after the body changed from A to B and back to A",
    ARejectedWriteChangesNothing => "a rejected write leaves the record unchanged, and an unknown path answers not found and a stale blob answers conflict before a store rejects anything",
    ConcurrentWritesFromOneBlobGiveOneConflict => "two writes from one blob that run together give one success and one conflict, and the record keeps the winner's body",
    AnUnknownPathIsNotFound => "a read or a write of an unknown path is not found",
    ReadAllAnswersEachPathAsReadDoes => "reading several paths together answers each path, in order, as a read of that path alone does",
    VersionNamesAreNeverReused => "a page never gets the same version name twice, and an older version keeps its content",
    ConcurrentPublishesGetTwoVersions => "two publishes that run together give two different versions, and the page lists every one",
    ConcurrentSavesOfOneNameGiveOneNameTaken => "two saves of one binding name that run together give one success and one name taken, and the binding is the saved one",
    ConcurrentMovesFromOnePinGiveOneMoved => "two moves from one pin that run together give one success and one moved that names the winner's version",
    MoveBindingRefusesAStalePin => "moving a binding from a pin it does not have answers moved with the current pin and changes nothing",
    AMoveRecordsItsMover => "a move keeps the provenance of its mover beside the provenance of the save",
    CheckFitSeesAVanishedRecord => "checking a stored binding refuses a record that no longer exists, and changes nothing",
    PublishThenLatestReturnsThePage => "publishing a page then asking for its latest version returns the page",
    RepublishAddsAVersionNewestFirst => "a republish adds a version, versions list newest first and an older version keeps its content",
    PagesListsPublishedNames => "the page list names each published page",
    UnknownNamesAreNotFound => "an unknown page, version or binding is not found",
    ProvenanceIsReturnedUnchanged => "provenance comes back unchanged from every read",
    AnUnresolvedOriginIsRefused => "an origin that names no existing page version or binding is refused and nothing is stored",
    ABindingRoundTrips => "a saved binding reads back with its pin, fills and provenance",
    ATakenNameAndAnUnknownVersionAreRefused => "a taken binding name and an unknown pinned version are refused and change nothing",
    AMisfitIsRefusedOnSave => "a binding whose records do not fit its page is refused and not saved",
    ARepublishLeavesAPinUnchanged => "a republish leaves a binding's pin where it was",
    MoveBindingChangesOnlyThePin => "moving a binding changes only its pin, toward an older or a newer version",
    MoveBindingRefusesAMisfit => "moving a binding to a version its records do not fit is refused and the pin stays",
    MoveBindingRefusesAVanishedRecord => "moving a binding whose bound record no longer exists is refused and the pin stays",
    BindingsOfListsAPagesBindings => "the bindings of a page lists the bindings pinned to that page",
    ATwoFillBindingIsCheckedAgainstOneState => "a binding's fills are read together at one state of the records, so a binding that fits only a mix of two states is refused",
    AcceptedCredentialGivesTheSamePrincipal => "an accepted credential gives the same principal every time",
    RejectedCredentialIsRejected => "a credential the authenticator does not accept is rejected",
    AccessFollowsTheResource => "a reachable resource is allowed and an unreachable one is denied, for read and for write",
    ACreatedShareReadsBack => "a created share reads back with its owner, title, kind, creation time and version 1",
    TheFirstVersionHoldsTheCreatedContent => "the first version of a created share holds the content it was created with",
    ATakenShareIdChangesNothing => "creating a share with a taken id answers id taken and leaves the first share and its content unchanged",
    ConcurrentCreatesOfOneIdGiveOneIdTaken => "two creates of one id that run together give one success and one id taken",
    PublishGivesTheNextVersion => "publishing gives the next version number and moves the share's latest to it",
    APublishLeavesOlderVersionsUnchanged => "a publish leaves every older version unchanged, with its content, publisher and time",
    EveryVersionReadsBack => "every version from 1 to the latest reads back",
    ConcurrentPublishesGetConsecutiveVersions => "two publishes that run together give two consecutive versions that each read back with their own content",
    PublishingToAnUnknownShareCreatesNothing => "publishing to an unknown share answers not found and creates nothing",
    AnUnknownShareOrVersionIsNotFound => "an unknown share, and a version above the latest, answer not found",
    TheOwnersListHoldsOnlyTheirSharesNewestFirst => "the owner's list holds exactly that owner's shares, newest first",
    AShareIsListedOnceCreated => "a share is in its owner's list as soon as it is created",
    TheShareLimitPerOwnerHolds => "a create by an owner with 500 shares answers too many shares and writes nothing, the 501st by another owner succeeds, and two creates at 499 give one success and one too many shares",
    TheVersionLimitHolds => "a publish to a share with 100 versions answers too many versions and writes nothing, and two publishes at 99 that run together give one success and one too many versions",
    AShareAlwaysHasItsFirstVersion => "a share is never written without its version 1, and a version above the latest answers not found",
    AGrantReadsBack => "a grant reads back with its role, its granter and its time",
    AGrantFromTheCurrentRoleReplacesIt => "granting again from the current role replaces the grant and leaves one entry",
    AGrantFromAStaleRoleIsChanged => "a grant from a role that is not current answers changed with the current role and writes nothing",
    ConcurrentGrantsFromNoGrantGiveOneChanged => "two grants from no grant that run together give one success and one changed",
    ARevokeIsVisibleToEveryRead => "a revoke removes the grant, and the next get, share list and person list show it gone",
    ARevokeFromAStaleRoleIsChanged => "a revoke from a role that is not current answers changed and keeps the grant",
    AGrantAndARevokeTogetherLeaveAConsistentState => "a grant and a revoke from one role that run together leave one of two consistent states",
    AGrantUnderAStaleAdminIsRefused => "a grant under an admin who no longer holds the Admin grant answers stale authority and writes nothing",
    AGrantUnderACurrentAdminIsApplied => "a grant under an admin who holds the Admin grant is applied",
    ARevokeUnderAStaleAdminIsRefused => "a revoke under an admin who no longer holds the Admin grant answers stale authority and keeps the grant",
    AnAdminsGrantNeverLandsAfterTheirRevoke => "a grant under an admin and the revoke of that admin that run together never leave a grant the admin wrote after the revoke",
    TheGrantLimitHolds => "the 257th new grantee answers too many, an existing grantee at the limit can change role, and a revoke frees a place",
    ConcurrentGrantsAtTheLimitGiveOneTooMany => "two grants at 255 grantees that run together give one success and one too many",
    AShareListHoldsOnlyItsGrants => "a share's list holds only that share's grants, and a grant on one share is not visible on another",
    APersonsListHoldsTheirGrantsNewestFirst => "a person's list holds that person's grants across shares, newest first, with the truncation flag",
    ARoleChangeKeepsTheFirstGrantTime => "a role change keeps the grant's time of first grant",
    ARevokeAndARegrantLeaveNoStaleEntry => "a revoke and a re-grant with the same role do not leave a stale list entry or a wrong-role entry",
    AGrantOnAnUnseenShareIsApplied => "a grant on a share the store holds no grants for is applied, and the grant store never answers not found",
    ALeaveAppliesUnderTheExpectedRoleAlone => "a revoke under the grantee's own authority applies under the expected role alone, including for an admin ending their own grant",
    StaleAuthorityComesBeforeChanged => "a revoke under a revoked admin answers stale authority and keeps the grant, and stale authority wins over changed",
    AListShowsItsTruncationAndOrder => "a list shows its truncation flag and its order",
    APersonsGrantsPageInOrderPastTheListingLimit => "a person's grants read in pages after a key come back once each and in order past the listing limit, a grant added during the walk repeats or skips none, and a revoked key starts at the next grant",
    AStoredFileReadsBack => "a stored file reads back with its media type and bytes",
    AFileIsNotFoundUnderAnotherShare => "a file stored under one share is not found under another share",
    ATakenFileIdChangesNothing => "storing to a taken id answers id taken and leaves the first file unchanged",
    ConcurrentPutsOfOneIdGiveOneIdTaken => "two puts of one id that run together give one success and one id taken",
    AnUnknownFileIsNotFound => "an unknown file answers not found",
    TheLargestFileReadsBack => "a file of the largest allowed size reads back",
    ACodeIsUsedOnce => "a login code gives its owner and port once, and a second use is refused",
    AnExpiredCodeIsRefused => "a code used before its expiry works, and one used at or after its expiry is refused",
    AnUnknownCodeIsRefused => "a code that was never issued is refused",
    ConcurrentUsesOfOneCodeGiveOneSuccess => "two uses of one code that run together give one success and one refusal",
    ATokenReadsBack => "a created token reads back with its owner, label, digest, creation time, a 90-day expiry and no revoked time",
    AnUnknownTokenIsNotFound => "an unknown token id answers not found",
    ARevokeKeepsItsFirstTime => "a revoke sets the revoked time, and a second revoke keeps the first time",
    ARevokeByAnotherOwnerIsNotFound => "a revoke by someone other than the owner answers not found and changes nothing, and the owner's revoke applies",
    ANewTokenWithTheSameLabelRevokesTheOld => "a new token revokes the owner's active token with the same label, and leaves other labels and other owners' tokens active",
    ALinkReadsBackUnderItsFlare => "a link reads back under its flare with its response, version and time",
    PositionsIncreaseAndTheHeadFollows => "each new link gets a larger position, and the head is the last one given",
    ARepeatedLinkKeepsItsFirstPosition => "a repeated link of one response and flare answers the first link and adds nothing",
    ConcurrentLinksGetDistinctPositions => "two links that run together get two different positions, both at or below the head",
    AFlaresListHoldsOnlyNewerLinksUpToTheBound => "a flare's list holds the links above the given position and at most the bound",
    AFlaresListHoldsOnlyItsLinks => "a flare's list holds only links to that flare",
    AResponsesListNamesEveryFlare => "a response's list names every flare it links to",
    TheLimitBoundsAListOldestFirst => "a flare's list holds at most the limit, oldest first",
    ACreatedTeamReadsBack => "a created team reads back active, with its owner as its only member",
    ATeamNameInUseIsTaken => "a team name is taken while its team is active or deleting, and a refused create writes nothing",
    MemberPairsAgree => "after adds and removes, a team's member list and each person's team list agree",
    APresentMemberIsNotAddedTwice => "adding a present member answers false and writes nothing",
    AnAbsentMemberIsNotRemoved => "removing an absent member answers false and writes nothing",
    AFullTeamRefusesAMember => "a team with the member limit refuses another member and writes nothing",
    AnInactiveTeamRefusesAMember => "a deleted or missing team refuses a member and writes nothing",
    ADeletingTeamKeepsItsOwner => "a deleting team reads back deleting with its owner, and marking it again changes nothing",
    AForgottenTeamFreesItsName => "forgetting a deleting team with no members removes it and frees its name, and an active team or one with members is not forgotten",
    ATransferNeedsTheOwnerAndAMember => "a transfer makes a member the owner of an active team, and is refused from a non-owner, to a non-member or on a deleting team",
    AGroupGrantKeepsItsAudience => "a group grant reads back with the audience it was written with, in the share's list and in the group's list",
    AnEntryReadsBackOnlyForItsReaderAndFlare => "an index entry reads back for its reader on its flare, and for no other reader or flare",
    EachEntryTakesANewPositionAndTheHeadFollows => "each new index entry takes a position above every earlier link and entry, and the head is the last one given",
    ARepeatedEntryKeepsItsFirstPosition => "a repeated index entry of one reader, response and flare keeps its first position and moves no position",
    UnindexRemovesOnlyTheReadersEntriesForTheResponse => "unindexing a reader from a response removes that reader's entries for it on every flare, and leaves other responses and other readers",
    AReadersListHoldsOnlyNewerEntriesUpToTheBound => "a reader's list on a flare holds the entries above the given position and at most the bound, oldest first, at most the limit",
    AnIndexOfManyEntriesWritesEach => "one index call of 120 entries writes each of them with its own position",
    PairsNameTheLinksFromEachResponseToTheFlare => "pairs answers the link from each named response to the flare, and nothing for a response without one",
    ConcurrentIndexesGetDistinctPositions => "two index calls that run together give their entries distinct positions, all at or below the head",
    FixtureIsUnusable => "the fixture holds a value that its type refuses",
    TheStoreFailedUnexpectedly => "the store returned an error that no clause expects",
}

/// A clause that an implementation breaks.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("contract clause broken: {clause}: {detail}")]
pub struct Violation {
    clause: Clause,
    detail: String,
}

impl Violation {
    pub(crate) fn new(clause: Clause, detail: impl fmt::Debug) -> Self {
        Self {
            clause,
            detail: format!("{detail:?}"),
        }
    }

    /// The clause that failed.
    #[must_use]
    pub fn clause(&self) -> Clause {
        self.clause
    }

    /// What the implementation did instead.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

/// Returns `Ok` when `condition` holds, and a [`Violation`] of `clause`
/// with `detail` when it does not.
pub(crate) fn ensure(
    condition: bool,
    clause: Clause,
    detail: impl FnOnce() -> String,
) -> Result<(), Violation> {
    if condition {
        Ok(())
    } else {
        Err(Violation::new(clause, detail()))
    }
}

/// Turns a refusal by a fixture value into a [`Violation`].
pub(crate) fn fixture<T, E: fmt::Debug>(parsed: Result<T, E>) -> Result<T, Violation> {
    parsed.map_err(|error| Violation::new(Clause::FixtureIsUnusable, error))
}

/// Turns an error that no clause expects into a [`Violation`].
pub(crate) fn unexpected<T, E: fmt::Debug>(result: Result<T, E>) -> Result<T, Violation> {
    result.map_err(|error| Violation::new(Clause::TheStoreFailedUnexpectedly, error))
}

/// A record that a fixture asks the caller to seed into a new record store.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SeedRecord {
    path: crate::RecordPath,
    record_type: crate::RecordType,
    body: crate::RecordBody,
}

impl SeedRecord {
    /// Builds a seed from a path, a type and a body.
    pub(crate) fn new(
        path: crate::RecordPath,
        record_type: crate::RecordType,
        body: crate::RecordBody,
    ) -> Self {
        Self {
            path,
            record_type,
            body,
        }
    }

    /// Where to seed the record.
    #[must_use]
    pub fn path(&self) -> &crate::RecordPath {
        &self.path
    }

    /// The type to seed.
    #[must_use]
    pub fn record_type(&self) -> &crate::RecordType {
        &self.record_type
    }

    /// The body to seed.
    #[must_use]
    pub fn body(&self) -> &crate::RecordBody {
        &self.body
    }
}

/// Builds a seed from text. A fixture's constants are valid, and a refusal
/// is reported as [`Clause::FixtureIsUnusable`].
pub(crate) fn seed(path: &str, record_type: &str, body: &str) -> Result<SeedRecord, Violation> {
    Ok(SeedRecord::new(
        fixture(crate::RecordPath::try_from(path))?,
        fixture(crate::RecordType::try_from(record_type))?,
        fixture(crate::RecordBody::try_from(body))?,
    ))
}
