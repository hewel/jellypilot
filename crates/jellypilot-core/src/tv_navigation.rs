//! Directional navigation shared by native TV presentations.

/// Logical remote actions, independent of keyboard or device key codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Input {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    PlayPause,
}

/// A grid edge leads to another region rather than wrapping into an unrelated row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GridMove {
    Item(u32),
    Rail,
    Header,
}

/// Moves within a finite five-column library, including an incomplete final row.
#[must_use]
pub fn grid_move(index: u32, total: u32, input: Input) -> GridMove {
    if total == 0 {
        return GridMove::Header;
    }
    let index = index.min(total - 1);
    let column = index % 5;
    match input {
        Input::Left if column == 0 => GridMove::Rail,
        Input::Left => GridMove::Item(index - 1),
        Input::Right if column < 4 && index + 1 < total => GridMove::Item(index + 1),
        Input::Up if index < 5 => GridMove::Header,
        Input::Up => GridMove::Item(index - 5),
        Input::Down if index / 5 < (total - 1) / 5 => {
            GridMove::Item(index.saturating_add(5).min(total - 1))
        }
        _ => GridMove::Item(index),
    }
}

/// Scroll just enough to expose the complete focused card, retaining the current offset otherwise.
#[must_use]
pub fn reveal_offset(top: f32, bottom: f32, offset: f32, viewport_height: f32) -> f32 {
    if top < offset {
        top.max(0.0)
    } else if bottom > offset + viewport_height {
        (bottom - viewport_height).max(0.0)
    } else {
        offset.max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_navigation_does_not_wrap_rows_and_reaches_partial_tail() {
        assert_eq!(grid_move(4, 12, Input::Right), GridMove::Item(4));
        assert_eq!(grid_move(5, 12, Input::Left), GridMove::Rail);
        assert_eq!(grid_move(4, 12, Input::Down), GridMove::Item(9));
        assert_eq!(grid_move(9, 12, Input::Down), GridMove::Item(11));
        assert_eq!(grid_move(11, 12, Input::Down), GridMove::Item(11));
        assert_eq!(grid_move(2, 12, Input::Up), GridMove::Header);
        assert_eq!(grid_move(0, 0, Input::Down), GridMove::Header);
    }

    #[test]
    fn focused_card_remains_visible_without_resetting_prior_scroll() {
        assert_eq!(reveal_offset(540.0, 1040.0, 100.0, 700.0), 340.0);
        assert_eq!(reveal_offset(540.0, 1040.0, 340.0, 700.0), 340.0);
        assert_eq!(reveal_offset(0.0, 500.0, 340.0, 700.0), 0.0);
    }
}
