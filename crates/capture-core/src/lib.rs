//! Privacy-conscious types shared by Lossy's capture adapters and context engine.

use std::fmt;

/// A focus generation assigned whenever the foreground editing target changes.
pub type FocusEpoch = u64;

/// A process-wide sequence assigned after capture events are normalized.
pub type EventSequence = u64;

/// How confidently an adapter identified an editing context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextConfidence {
    High,
    Medium,
    Low,
}

/// The adapter class that supplied a context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextSource {
    AppIntegration,
    BrowserCompanion,
    EditorCompanion,
    AppSpecificUia,
    GenericUia,
}

/// Errors raised when an adapter supplies a context that cannot be isolated safely.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextError {
    EmptyAppId,
    EmptyEditorRole,
    MissingIsolationId,
}

impl fmt::Display for ContextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyAppId => "app_id must not be empty",
            Self::EmptyEditorRole => "editor_role must not be empty",
            Self::MissingIsolationId => {
                "low-confidence contexts require an isolation_id to prevent merging"
            }
        };

        formatter.write_str(message)
    }
}

impl std::error::Error for ContextError {}

/// In-memory context data reported by a capture adapter.
///
/// The identifiers can contain sensitive metadata. The custom `Debug` implementation
/// intentionally redacts them, and callers should derive a [`ContextKey`] before storage.
#[derive(Clone, Eq, PartialEq)]
pub struct ResolvedContext {
    app_id: String,
    profile_id: Option<String>,
    container_id: Option<String>,
    entity_id: Option<String>,
    editor_role: String,
    isolation_id: Option<String>,
    confidence: ContextConfidence,
    source: ContextSource,
}

impl ResolvedContext {
    /// Creates a validated context.
    ///
    /// A low-confidence context must include a per-surface isolation ID. This makes the safe
    /// fallback an extra draft rather than an accidental merge with another conversation.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        app_id: impl Into<String>,
        profile_id: Option<String>,
        container_id: Option<String>,
        entity_id: Option<String>,
        editor_role: impl Into<String>,
        isolation_id: Option<String>,
        confidence: ContextConfidence,
        source: ContextSource,
    ) -> Result<Self, ContextError> {
        let app_id = app_id.into();
        let editor_role = editor_role.into();

        if app_id.trim().is_empty() {
            return Err(ContextError::EmptyAppId);
        }
        if editor_role.trim().is_empty() {
            return Err(ContextError::EmptyEditorRole);
        }
        if confidence == ContextConfidence::Low && isolation_id.as_deref().is_none_or(str::is_empty)
        {
            return Err(ContextError::MissingIsolationId);
        }

        Ok(Self {
            app_id,
            profile_id,
            container_id,
            entity_id,
            editor_role,
            isolation_id,
            confidence,
            source,
        })
    }

    pub fn confidence(&self) -> ContextConfidence {
        self.confidence
    }

    pub fn source(&self) -> ContextSource {
        self.source
    }
}

impl fmt::Debug for ResolvedContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedContext")
            .field("identifiers", &"[redacted]")
            .field("confidence", &self.confidence)
            .field("source", &self.source)
            .finish()
    }
}

/// A local, non-reversible identifier used to associate events with a context.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct ContextKey([u8; 32]);

impl ContextKey {
    /// Derives a key using a per-installation secret protected by Windows DPAPI.
    pub fn derive(installation_secret: &[u8; 32], context: &ResolvedContext) -> Self {
        let mut canonical = Vec::new();
        append_component(&mut canonical, 1, Some(&context.app_id));
        append_component(&mut canonical, 2, context.profile_id.as_deref());
        append_component(&mut canonical, 3, context.container_id.as_deref());
        append_component(&mut canonical, 4, context.entity_id.as_deref());
        append_component(&mut canonical, 5, Some(&context.editor_role));
        append_component(&mut canonical, 6, context.isolation_id.as_deref());

        Self(*blake3::keyed_hash(installation_secret, &canonical).as_bytes())
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for ContextKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "ContextKey({:02x}{:02x}{:02x}{:02x}…)",
            self.0[0], self.0[1], self.0[2], self.0[3]
        )
    }
}

fn append_component(buffer: &mut Vec<u8>, tag: u8, value: Option<&str>) {
    buffer.push(tag);
    match value {
        Some(value) => {
            buffer.push(1);
            let bytes = value.as_bytes();
            let length = u32::try_from(bytes.len()).expect("context component is too large");
            buffer.extend_from_slice(&length.to_le_bytes());
            buffer.extend_from_slice(bytes);
        }
        None => buffer.push(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SECRET: [u8; 32] = [7; 32];

    fn known_context(entity: &str, role: &str) -> ResolvedContext {
        ResolvedContext::new(
            "whatsapp",
            Some("profile-1".into()),
            None,
            Some(entity.into()),
            role,
            None,
            ContextConfidence::High,
            ContextSource::BrowserCompanion,
        )
        .expect("known context should be valid")
    }

    #[test]
    fn stable_context_derives_the_same_key() {
        let first = ContextKey::derive(&TEST_SECRET, &known_context("user-a", "composer"));
        let second = ContextKey::derive(&TEST_SECRET, &known_context("user-a", "composer"));

        assert_eq!(first, second);
    }

    #[test]
    fn conversation_and_editor_role_are_part_of_the_key() {
        let user_a = ContextKey::derive(&TEST_SECRET, &known_context("user-a", "composer"));
        let user_b = ContextKey::derive(&TEST_SECRET, &known_context("user-b", "composer"));
        let reply = ContextKey::derive(&TEST_SECRET, &known_context("user-a", "reply"));

        assert_ne!(user_a, user_b);
        assert_ne!(user_a, reply);
    }

    #[test]
    fn low_confidence_contexts_require_isolation() {
        let result = ResolvedContext::new(
            "unknown-app",
            None,
            None,
            None,
            "composer",
            None,
            ContextConfidence::Low,
            ContextSource::GenericUia,
        );

        assert!(matches!(result, Err(ContextError::MissingIsolationId)));
    }

    #[test]
    fn debug_output_redacts_context_identifiers() {
        let context = known_context("private-conversation", "composer");
        let output = format!("{context:?}");

        assert!(!output.contains("private-conversation"));
        assert!(!output.contains("profile-1"));
        assert!(output.contains("[redacted]"));
    }
}
