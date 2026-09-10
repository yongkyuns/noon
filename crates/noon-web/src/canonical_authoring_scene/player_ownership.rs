//! Exclusive ownership of the existing canonical execution player.
//!
//! | Phase | Authoring context | Presentation endpoint | Valid exits |
//! | --- | --- | --- | --- |
//! | Unstarted | Authors/queries the store; may bootstrap one player | None | Active, Transferred |
//! | Active | Drives, queries and coherently publishes through its player | None | Transferred |
//! | Transferred | Rejects live queries, edits, drives and a second transfer | Sole owner of the moved player; may drive/query/publish and return it | Returned |
//! | Returned | Drives/queries/publishes through the retained player; may declare a continuation | None | Transferred, Unstarted at an explicit stale-run boundary |
//!
//! Active and Returned are deliberately distinct: only a dormant returned runtime
//! may be superseded after direct authoring changes. A stale returned observation
//! reads authored state, but does not itself replace the runtime. Callback failure,
//! pending/completed segments, wake clocks, and publication revisions stay in their
//! existing owners; they are not additional ownership phases. Return and re-lease
//! move the same non-Clone player, never construct a second runtime. A failed
//! validation leaves this state untouched and returns the rejected player to its caller. No transition waits for GPU retirement.

use crate::semantic_execution_player::ExecutionPlayerIdentity;
use crate::SemanticExecutionPlayer;
use noon_core::{SceneRevision, SemanticNodeId, SemanticStore};
use std::{cell::RefCell, rc::Rc};

pub(super) enum PlayerOwnership {
    Unstarted,
    Active(SemanticExecutionPlayer),
    /// Identities captured from the player before moving it to the endpoint.
    /// Revisions are not frozen here: valid execution may publish while leased.
    Transferred(ExecutionPlayerIdentity),
    Returned(SemanticExecutionPlayer),
}

/// Ownership failures remain typed until the language boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlayerReturnError {
    NotLeased,
    ForeignScene,
    StaleLease,
}

impl std::fmt::Display for PlayerReturnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotLeased => "semantic execution player is not leased by this context",
            Self::ForeignScene => "semantic execution player belongs to another authoring scene",
            Self::StaleLease => {
                "semantic execution player does not match the current runtime/transport lease"
            }
        })
    }
}

impl std::error::Error for PlayerReturnError {}

/// A fallible ownership transfer must return the rejected capability, not drop it.
/// Boxing is confined to this failure path; successful handoffs still only move.
pub(super) struct RejectedPlayerReturn {
    pub(super) reason: PlayerReturnError,
    pub(super) player: Box<SemanticExecutionPlayer>,
}

impl std::fmt::Debug for RejectedPlayerReturn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RejectedPlayerReturn")
            .field("reason", &self.reason)
            .field("identity", &self.player.ownership_identity())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Display for RejectedPlayerReturn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.reason.fmt(f)
    }
}

impl std::error::Error for RejectedPlayerReturn {}

impl PlayerOwnership {
    pub(super) fn local(&self) -> Option<&SemanticExecutionPlayer> {
        match self {
            Self::Active(player) | Self::Returned(player) => Some(player),
            Self::Unstarted | Self::Transferred(_) => None,
        }
    }

    pub(super) fn local_mut(&mut self) -> Option<&mut SemanticExecutionPlayer> {
        match self {
            Self::Active(player) | Self::Returned(player) => Some(player),
            Self::Unstarted | Self::Transferred(_) => None,
        }
    }

    pub(super) fn is_unstarted(&self) -> bool {
        matches!(self, Self::Unstarted)
    }

    pub(super) fn is_transferred(&self) -> bool {
        matches!(self, Self::Transferred(_))
    }

    #[cfg(test)]
    pub(super) fn is_returned(&self) -> bool {
        matches!(self, Self::Returned(_))
    }

    pub(super) fn browser_name(&self) -> &'static str {
        match self {
            Self::Unstarted => "none",
            Self::Active(_) => "active",
            Self::Transferred(_) => "transferred",
            Self::Returned(_) => "returned",
        }
    }

    pub(super) fn prepare_for_run(&mut self, authored: SceneRevision) -> Result<(), String> {
        match self {
            Self::Transferred(_) => {
                return Err("live execution session is running in the semantic engine".into());
            }
            Self::Active(player) if player.scene_revision() != authored => {
                return Err("authored scene changed while live execution is active".into());
            }
            Self::Returned(player) if player.scene_revision() != authored => {
                *self = Self::Unstarted;
            }
            Self::Unstarted | Self::Active(_) | Self::Returned(_) => {}
        }
        Ok(())
    }

    pub(super) fn returned_is_stale(&self, authored: SceneRevision) -> bool {
        matches!(self, Self::Returned(player) if player.scene_revision() != authored)
    }

    /// All fallible preparation must finish before ownership is moved.
    pub(super) fn transfer(&mut self) -> Result<SemanticExecutionPlayer, String> {
        let identity = match self {
            Self::Active(player) | Self::Returned(player) => player.ownership_identity(),
            Self::Unstarted => return Err("begin live execution before transferring it".into()),
            Self::Transferred(_) => {
                return Err("live execution session is running in the semantic engine".into());
            }
        };
        match std::mem::replace(self, Self::Transferred(identity)) {
            Self::Active(player) | Self::Returned(player) => Ok(player),
            Self::Unstarted | Self::Transferred(_) => unreachable!("validated local owner"),
        }
    }

    /// Borrowed preflight also allows internal callers to retain a rejected player.
    pub(super) fn validate_return(
        &self,
        player: &SemanticExecutionPlayer,
        store: &Rc<RefCell<SemanticStore>>,
        root: SemanticNodeId,
    ) -> Result<(), PlayerReturnError> {
        let Self::Transferred(expected) = self else {
            return Err(PlayerReturnError::NotLeased);
        };
        if !player.belongs_to_authoring_scene(store, root) {
            return Err(PlayerReturnError::ForeignScene);
        }
        if player.ownership_identity() != *expected {
            return Err(PlayerReturnError::StaleLease);
        }
        Ok(())
    }
}
