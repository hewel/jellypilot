//! Deterministic stateful property-testing helper with counterexample shrinking.

/// SplitMix64 deterministic PRNG for repeatable stateful property tests.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    /// Creates a deterministic generator from `seed`.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(seed.wrapping_add(0x9e37_79b9_7f4a_7c15))
    }

    /// Advances and returns the next pseudo-random 64-bit integer.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Returns a value in `0..upper`. Panics when `upper == 0`.
    #[must_use]
    pub fn below(&mut self, upper: usize) -> usize {
        assert!(upper > 0, "upper bound must be positive");
        (self.next_u64() % (upper as u64)) as usize
    }

    /// Returns a pseudo-random boolean.
    #[must_use]
    pub fn bool(&mut self) -> bool {
        (self.next_u64() & 1) == 1
    }

    /// Selects one element from a non-empty slice.
    #[must_use]
    pub fn choose<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// Runs `seeds` generated sequences of `steps` actions and shrinks the first failure.
pub fn check_stateful<A: Clone + std::fmt::Debug>(
    seeds: u64,
    steps: usize,
    mut gen_action: impl FnMut(&mut Rng) -> A,
    mut run: impl FnMut(&[A]) -> Result<(), String>,
) {
    for seed in 0..seeds {
        let mut rng = Rng::new(seed);
        let actions: Vec<A> = (0..steps).map(|_| gen_action(&mut rng)).collect();
        if let Err(initial_error) = run(&actions) {
            let (minimal, error) = shrink(actions, initial_error, &mut run);
            panic!(
                "stateful property failed on seed {seed} (shrunk to {} steps):\nactions: {minimal:#?}\nerror: {error}",
                minimal.len()
            );
        }
    }
}

fn shrink<A: Clone>(
    mut current: Vec<A>,
    mut last_error: String,
    run: &mut impl FnMut(&[A]) -> Result<(), String>,
) -> (Vec<A>, String) {
    let mut chunk = (current.len() / 2).max(1);
    while chunk > 0 && current.len() > 1 {
        let mut changed = false;
        let mut index = 0;
        while index < current.len() && current.len() > 1 {
            let end = (index + chunk).min(current.len());
            if end - index == current.len() {
                break;
            }
            let mut candidate = Vec::with_capacity(current.len() - (end - index));
            candidate.extend_from_slice(&current[..index]);
            candidate.extend_from_slice(&current[end..]);
            if let Err(error) = run(&candidate) {
                current = candidate;
                last_error = error;
                changed = true;
            } else {
                index += chunk;
            }
        }
        if !changed {
            chunk /= 2;
        } else if chunk > current.len() / 2 {
            chunk = (current.len() / 2).max(1);
        }
    }
    (current, last_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Synthetic {
        Noop,
        Arm,
        Disarm,
        Trigger,
    }

    #[test]
    fn rng_is_deterministic_across_replays() {
        let mut first = Rng::new(42);
        let mut second = Rng::new(42);
        let a: Vec<u64> = (0..16).map(|_| first.next_u64()).collect();
        let b: Vec<u64> = (0..16).map(|_| second.next_u64()).collect();
        assert_eq!(a, b);
    }

    #[test]
    fn shrink_reduces_failing_trace_to_minimal_trigger_pair() {
        let mut rng = Rng::new(7);
        let actions: Vec<Synthetic> = (0..60)
            .map(|_| match rng.below(4) {
                0 => Synthetic::Noop,
                1 => Synthetic::Arm,
                2 => Synthetic::Disarm,
                _ => Synthetic::Trigger,
            })
            .collect();
        let mut check = |trace: &[Synthetic]| -> Result<(), String> {
            let mut armed = false;
            for step in trace {
                match step {
                    Synthetic::Noop => {}
                    Synthetic::Arm => armed = true,
                    Synthetic::Disarm => armed = false,
                    Synthetic::Trigger if armed => return Err("triggered while armed".to_owned()),
                    Synthetic::Trigger => {}
                }
            }
            Ok(())
        };
        let initial = check(&actions).expect_err("seed 7 sequence should trigger failure");
        let (shrunk, error) = shrink(actions, initial, &mut check);
        assert_eq!(error, "triggered while armed");
        assert_eq!(shrunk, vec![Synthetic::Arm, Synthetic::Trigger]);
    }
}
