//! Draft-session state transitions for Lossy.
//!
//! This crate does not capture text or persist content. It decides which draft a normalized
//! event belongs to and rejects events that arrive after focus has moved elsewhere.

use std::{collections::HashMap, fmt};

use lossy_capture_core::{ContextKey, EventSequence, FocusEpoch};

/// Identifier assigned to one draft generation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DraftId(u64);

impl DraftId {
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Lifecycle state of a recoverable draft.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftStatus {
    Active,
    Suspended,
    Completed,
    Cleared,
}

impl DraftStatus {
    fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cleared)
    }
}

/// Current recovery snapshot for one context generation.
#[derive(Clone, Eq, PartialEq)]
pub struct DraftSnapshot {
    id: DraftId,
    context: ContextKey,
    generation: u32,
    status: DraftStatus,
    recoverable_content: String,
    editor_is_empty: bool,
}

impl fmt::Debug for DraftSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DraftSnapshot")
            .field("id", &self.id)
            .field("context", &self.context)
            .field("generation", &self.generation)
            .field("status", &self.status)
            .field("recoverable_content", &"[redacted]")
            .field("content_bytes", &self.recoverable_content.len())
            .field("editor_is_empty", &self.editor_is_empty)
            .finish()
    }
}

impl DraftSnapshot {
    pub fn id(&self) -> DraftId {
        self.id
    }

    pub fn context(&self) -> ContextKey {
        self.context
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn status(&self) -> DraftStatus {
        self.status
    }

    /// Returns the last non-empty content kept for recovery.
    pub fn recoverable_content(&self) -> &str {
        &self.recoverable_content
    }

    pub fn editor_is_empty(&self) -> bool {
        self.editor_is_empty
    }
}

/// A normalized event received from the capture pipeline.
#[derive(Clone, Eq, PartialEq)]
pub enum SessionEvent {
    Focused {
        sequence: EventSequence,
        context: ContextKey,
        focus_epoch: FocusEpoch,
    },
    TextObserved {
        sequence: EventSequence,
        context: ContextKey,
        focus_epoch: FocusEpoch,
        content: String,
    },
    Submitted {
        sequence: EventSequence,
        context: ContextKey,
        focus_epoch: FocusEpoch,
    },
    CapturePaused {
        sequence: EventSequence,
    },
}

impl fmt::Debug for SessionEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Focused {
                sequence,
                context,
                focus_epoch,
            } => formatter
                .debug_struct("Focused")
                .field("sequence", sequence)
                .field("context", context)
                .field("focus_epoch", focus_epoch)
                .finish(),
            Self::TextObserved {
                sequence,
                context,
                focus_epoch,
                content,
            } => formatter
                .debug_struct("TextObserved")
                .field("sequence", sequence)
                .field("context", context)
                .field("focus_epoch", focus_epoch)
                .field("content", &"[redacted]")
                .field("content_bytes", &content.len())
                .finish(),
            Self::Submitted {
                sequence,
                context,
                focus_epoch,
            } => formatter
                .debug_struct("Submitted")
                .field("sequence", sequence)
                .field("context", context)
                .field("focus_epoch", focus_epoch)
                .finish(),
            Self::CapturePaused { sequence } => formatter
                .debug_struct("CapturePaused")
                .field("sequence", sequence)
                .finish(),
        }
    }
}

impl SessionEvent {
    fn sequence(&self) -> EventSequence {
        match self {
            Self::Focused { sequence, .. }
            | Self::TextObserved { sequence, .. }
            | Self::Submitted { sequence, .. }
            | Self::CapturePaused { sequence } => *sequence,
        }
    }
}

/// Why an event did not change session state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IgnoreReason {
    OutOfOrderSequence,
    StaleFocus,
}

/// State changes the durable writer should persist.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineAction {
    DraftCreated(DraftSnapshot),
    DraftUpdated(DraftSnapshot),
    DraftSuspended(DraftSnapshot),
    DraftResumed(DraftSnapshot),
    DraftCompleted(DraftSnapshot),
    DraftCleared(DraftSnapshot),
    Ignored(IgnoreReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveTarget {
    context: ContextKey,
    focus_epoch: FocusEpoch,
}

/// Assigns text events to independent draft generations.
pub struct ContextEngine {
    active: Option<ActiveTarget>,
    drafts: HashMap<ContextKey, Vec<DraftSnapshot>>,
    next_draft_id: u64,
    last_sequence: Option<EventSequence>,
}

impl Default for ContextEngine {
    fn default() -> Self {
        Self {
            active: None,
            drafts: HashMap::new(),
            next_draft_id: 1,
            last_sequence: None,
        }
    }
}

impl ContextEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies one normalized event in sequence order.
    pub fn handle(&mut self, event: SessionEvent) -> Vec<EngineAction> {
        let sequence = event.sequence();
        if self
            .last_sequence
            .is_some_and(|last_sequence| sequence <= last_sequence)
        {
            return vec![EngineAction::Ignored(IgnoreReason::OutOfOrderSequence)];
        }
        self.last_sequence = Some(sequence);

        match event {
            SessionEvent::Focused {
                context,
                focus_epoch,
                ..
            } => self.focus(context, focus_epoch),
            SessionEvent::TextObserved {
                context,
                focus_epoch,
                content,
                ..
            } => self.observe_text(context, focus_epoch, content),
            SessionEvent::Submitted {
                context,
                focus_epoch,
                ..
            } => self.submit(context, focus_epoch),
            SessionEvent::CapturePaused { .. } => self.pause(),
        }
    }

    pub fn drafts_for(&self, context: ContextKey) -> &[DraftSnapshot] {
        self.drafts.get(&context).map_or(&[], Vec::as_slice)
    }

    pub fn active_context(&self) -> Option<ContextKey> {
        self.active.map(|target| target.context)
    }

    fn focus(&mut self, context: ContextKey, focus_epoch: FocusEpoch) -> Vec<EngineAction> {
        let target = ActiveTarget {
            context,
            focus_epoch,
        };

        if self.active == Some(target) {
            return Vec::new();
        }

        let mut actions = self.suspend_active();
        self.active = Some(target);

        if let Some(draft) = self.latest_mut(context)
            && draft.status == DraftStatus::Suspended
        {
            draft.status = DraftStatus::Active;
            actions.push(EngineAction::DraftResumed(draft.clone()));
        }

        actions
    }

    fn observe_text(
        &mut self,
        context: ContextKey,
        focus_epoch: FocusEpoch,
        content: String,
    ) -> Vec<EngineAction> {
        if !self.matches_active(context, focus_epoch) {
            return vec![EngineAction::Ignored(IgnoreReason::StaleFocus)];
        }

        if content.is_empty() {
            let Some(draft) = self.latest_mut(context) else {
                return Vec::new();
            };

            if draft.status.is_terminal() || draft.editor_is_empty {
                return Vec::new();
            }

            draft.status = DraftStatus::Cleared;
            draft.editor_is_empty = true;
            return vec![EngineAction::DraftCleared(draft.clone())];
        }

        let needs_new_generation = self
            .latest(context)
            .is_none_or(|draft| draft.status.is_terminal());

        if needs_new_generation {
            let draft = self.create_draft(context, content);
            return vec![EngineAction::DraftCreated(draft)];
        }

        let draft = self
            .latest_mut(context)
            .expect("a non-terminal draft was checked above");
        if draft.recoverable_content == content && !draft.editor_is_empty {
            return Vec::new();
        }

        draft.recoverable_content = content;
        draft.editor_is_empty = false;
        draft.status = DraftStatus::Active;
        vec![EngineAction::DraftUpdated(draft.clone())]
    }

    fn submit(&mut self, context: ContextKey, focus_epoch: FocusEpoch) -> Vec<EngineAction> {
        if !self.matches_active(context, focus_epoch) {
            return vec![EngineAction::Ignored(IgnoreReason::StaleFocus)];
        }

        let Some(draft) = self.latest_mut(context) else {
            return Vec::new();
        };
        if draft.status == DraftStatus::Completed {
            return Vec::new();
        }

        draft.status = DraftStatus::Completed;
        vec![EngineAction::DraftCompleted(draft.clone())]
    }

    fn pause(&mut self) -> Vec<EngineAction> {
        self.suspend_active()
    }

    fn suspend_active(&mut self) -> Vec<EngineAction> {
        let Some(active) = self.active.take() else {
            return Vec::new();
        };

        let Some(draft) = self.latest_mut(active.context) else {
            return Vec::new();
        };
        if draft.status != DraftStatus::Active {
            return Vec::new();
        }

        draft.status = DraftStatus::Suspended;
        vec![EngineAction::DraftSuspended(draft.clone())]
    }

    fn matches_active(&self, context: ContextKey, focus_epoch: FocusEpoch) -> bool {
        self.active
            == Some(ActiveTarget {
                context,
                focus_epoch,
            })
    }

    fn create_draft(&mut self, context: ContextKey, content: String) -> DraftSnapshot {
        let generation = self
            .latest(context)
            .map_or(1, |draft| draft.generation.saturating_add(1));
        let draft = DraftSnapshot {
            id: DraftId(self.next_draft_id),
            context,
            generation,
            status: DraftStatus::Active,
            recoverable_content: content,
            editor_is_empty: false,
        };
        self.next_draft_id = self.next_draft_id.saturating_add(1);
        self.drafts.entry(context).or_default().push(draft.clone());
        draft
    }

    fn latest(&self, context: ContextKey) -> Option<&DraftSnapshot> {
        self.drafts.get(&context).and_then(|drafts| drafts.last())
    }

    fn latest_mut(&mut self, context: ContextKey) -> Option<&mut DraftSnapshot> {
        self.drafts
            .get_mut(&context)
            .and_then(|drafts| drafts.last_mut())
    }
}

#[cfg(test)]
mod tests {
    use lossy_capture_core::{ContextConfidence, ContextSource, ResolvedContext};

    use super::*;

    const TEST_SECRET: [u8; 32] = [11; 32];

    fn context(entity: &str) -> ContextKey {
        let resolved = ResolvedContext::new(
            "whatsapp",
            Some("profile-1".into()),
            None,
            Some(entity.into()),
            "main-composer",
            None,
            ContextConfidence::High,
            ContextSource::BrowserCompanion,
        )
        .expect("synthetic context should be valid");
        ContextKey::derive(&TEST_SECRET, &resolved)
    }

    fn focus(sequence: u64, context: ContextKey, focus_epoch: u64) -> SessionEvent {
        SessionEvent::Focused {
            sequence,
            context,
            focus_epoch,
        }
    }

    fn text(sequence: u64, context: ContextKey, focus_epoch: u64, content: &str) -> SessionEvent {
        SessionEvent::TextObserved {
            sequence,
            context,
            focus_epoch,
            content: content.into(),
        }
    }

    #[test]
    fn switching_between_people_keeps_independent_drafts() {
        let user_a = context("user-a");
        let user_b = context("user-b");
        let mut engine = ContextEngine::new();

        engine.handle(focus(1, user_a, 1));
        engine.handle(text(2, user_a, 1, "Draft for A"));
        let actions = engine.handle(focus(3, user_b, 2));
        assert!(matches!(actions[0], EngineAction::DraftSuspended(_)));

        let stale = engine.handle(text(4, user_a, 1, "Must not leak into B"));
        assert_eq!(stale, vec![EngineAction::Ignored(IgnoreReason::StaleFocus)]);

        engine.handle(text(5, user_b, 2, "Draft for B"));
        let actions = engine.handle(focus(6, user_a, 3));
        assert!(matches!(actions[1], EngineAction::DraftResumed(_)));
        engine.handle(text(7, user_a, 3, "Draft for A continued"));

        let drafts_a = engine.drafts_for(user_a);
        let drafts_b = engine.drafts_for(user_b);
        assert_eq!(drafts_a.len(), 1);
        assert_eq!(drafts_b.len(), 1);
        assert_eq!(drafts_a[0].recoverable_content(), "Draft for A continued");
        assert_eq!(drafts_b[0].recoverable_content(), "Draft for B");
        assert_eq!(drafts_b[0].status(), DraftStatus::Suspended);
    }

    #[test]
    fn submission_starts_a_new_generation_for_the_next_message() {
        let user_a = context("user-a");
        let mut engine = ContextEngine::new();

        engine.handle(focus(1, user_a, 10));
        engine.handle(text(2, user_a, 10, "First message"));
        let completed = engine.handle(SessionEvent::Submitted {
            sequence: 3,
            context: user_a,
            focus_epoch: 10,
        });
        assert!(matches!(completed[0], EngineAction::DraftCompleted(_)));

        engine.handle(text(4, user_a, 10, "Second message"));

        let drafts = engine.drafts_for(user_a);
        assert_eq!(drafts.len(), 2);
        assert_eq!(drafts[0].generation(), 1);
        assert_eq!(drafts[0].status(), DraftStatus::Completed);
        assert_eq!(drafts[1].generation(), 2);
        assert_eq!(drafts[1].recoverable_content(), "Second message");
    }

    #[test]
    fn clearing_keeps_recoverable_text_and_next_input_is_new() {
        let user_a = context("user-a");
        let mut engine = ContextEngine::new();

        engine.handle(focus(1, user_a, 4));
        engine.handle(text(2, user_a, 4, "Accidentally cleared"));
        let cleared = engine.handle(text(3, user_a, 4, ""));
        assert!(matches!(cleared[0], EngineAction::DraftCleared(_)));

        let first = &engine.drafts_for(user_a)[0];
        assert_eq!(first.recoverable_content(), "Accidentally cleared");
        assert!(first.editor_is_empty());

        engine.handle(text(4, user_a, 4, "A new draft"));
        let drafts = engine.drafts_for(user_a);
        assert_eq!(drafts.len(), 2);
        assert_eq!(drafts[0].status(), DraftStatus::Cleared);
        assert_eq!(drafts[1].generation(), 2);
    }

    #[test]
    fn out_of_order_events_are_ignored() {
        let user_a = context("user-a");
        let mut engine = ContextEngine::new();

        engine.handle(focus(10, user_a, 1));
        engine.handle(text(12, user_a, 1, "Newest"));
        let ignored = engine.handle(text(11, user_a, 1, "Older"));

        assert_eq!(
            ignored,
            vec![EngineAction::Ignored(IgnoreReason::OutOfOrderSequence)]
        );
        assert_eq!(engine.drafts_for(user_a)[0].recoverable_content(), "Newest");
    }

    #[test]
    fn pausing_suspends_only_the_active_draft() {
        let user_a = context("user-a");
        let mut engine = ContextEngine::new();

        engine.handle(focus(1, user_a, 1));
        engine.handle(text(2, user_a, 1, "Keep me"));
        let actions = engine.handle(SessionEvent::CapturePaused { sequence: 3 });

        assert!(matches!(actions[0], EngineAction::DraftSuspended(_)));
        assert_eq!(engine.active_context(), None);
        assert_eq!(
            engine.drafts_for(user_a)[0].status(),
            DraftStatus::Suspended
        );
    }

    #[test]
    fn debug_output_never_contains_draft_content() {
        let user_a = context("user-a");
        let event = text(1, user_a, 1, "private synthetic draft");
        let event_debug = format!("{event:?}");
        assert!(!event_debug.contains("private synthetic draft"));
        assert!(event_debug.contains("[redacted]"));

        let mut engine = ContextEngine::new();
        engine.handle(focus(1, user_a, 1));
        let actions = engine.handle(text(2, user_a, 1, "another private draft"));
        let action_debug = format!("{:?}", actions[0]);
        assert!(!action_debug.contains("another private draft"));
        assert!(action_debug.contains("[redacted]"));
    }
}
