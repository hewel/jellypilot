//! Visual variants shared by JellyPilot iced widgets.

/// Button variants from `src/components/ui/Button.styles.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Outlined,
    Text,
    Icon,
}

/// Card surface variants from `src/components/ui/Card.styles.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceVariant {
    Elevated,
    Filled,
}

/// Semantic text roles used by JellyPilot's Panda component styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextVariant {
    OnSurface,
    OnSurfaceVariant,
    Primary,
    Secondary,
    Tertiary,
    Warning,
    Error,
}

/// Status badge variants from `src/components/ui/StatusBadge.styles.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeVariant {
    Success,
    Warning,
    Neutral,
}

/// Text input variants from `src/components/ui/FieldControl.styles.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldVariant {
    Filled,
}
