# Rule: Be Conservative When Writing Style Tests

This document defines JellyPilot's policy on writing style-related tests. `AGENTS.md` points here; do not duplicate these rules there.

## Policy

- **Prefer behavior and user-observable outcomes**: Verify behavior, accessibility, state transitions, and user-visible content first. Assert that an error is visible or a control is disabled, not the CSS properties that achieve it.
- **Avoid testing implementation details**: Do not assert on exact colors, margins, padding, font sizes, CSS classes, DOM nesting, or animation timing unless explicitly contractual.

## High-Value Exceptions

Style tests are only acceptable for:
1. **Design System Primitives**: Visual states that are part of the component API.
2. **Critical Responsive Layouts**: Areas that commonly regress and affect usability.
3. **Themes**: Dark/light mode switching.
4. **Accessibility Visual States**: Focus, disabled, error, selected, or contrast-critical states.
5. **Usability Layouts**: Overflow, truncation, scrolling, modal layering, or tooltip positioning.
6. **Sparse Regression Snapshots**: Stable, critical screens or components (use sparingly; do not substitute for behavior tests).

## Pre-Test Checklist

Before writing a style test, ask:
1. Would a user notice or be blocked if this visual detail broke?
2. Is this visual behavior part of the component/product contract?
3. Is this test less brittle than manual/screenshot review?
4. Will it remain valid if the styling implementation changes but the UX stays correct?

*If the answer to any of these is not clearly yes, do not write the style test.*
