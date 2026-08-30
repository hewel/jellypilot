//! Visual variants shared by JellyPilot iced widgets.

/// Button variants: filled actions, quiet tonal controls, and text/icon
/// buttons. Buttons never cast a shadow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    /// Quiet control: transparent at rest, `surfaceContainerHigh` fill on hover.
    Tonal,
    /// Tonal control in its selected/on state: always filled with
    /// `surfaceContainerHigh`.
    TonalActive,
    Text,
    Icon,
}

/// Container surface roles. A surface is exactly one of: flush with the app
/// background (`Canvas`), a docked block separated only by the shell
/// hairlines (`Block`), or a floating layer with the high shadow (`Raised`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceVariant {
    /// Flush with the window: opaque `background`, square, no border/shadow.
    Canvas,
    /// Docked block (sidebar, player bar): opaque `surfaceContainerLow`,
    /// square, no border/shadow.
    Block,
    /// Floating layer (cards, toasts, popovers): opaque
    /// `surfaceContainerHigh`, radius `lg`, `raised_high` shadow.
    Raised,
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
