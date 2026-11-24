//! The states of a host texture's subresources: as they really are (between
//! submissions), and as one list finds and leaves them. Most textures are
//! used whole and have one state; a pass that reads one mip or slice and
//! writes another needs them told apart.

use windows::Win32::Graphics::Direct3D12::*;

/// The states of a texture's subresources: one for all, or one each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum States {
    All(u32),
    Each(Vec<u32>),
}

impl States {
    pub(crate) fn at(&self, sub: u32) -> u32 {
        match self {
            States::All(s) => *s,
            States::Each(each) => each.get(sub as usize).copied().unwrap_or(0),
        }
    }

    fn of(each: Vec<u32>) -> Self {
        match each.first() {
            Some(&first) if each.iter().all(|&s| s == first) => States::All(first),
            _ => States::Each(each),
        }
    }
}

/// Which subresources of a texture a use is of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Subs {
    All,
    /// Their indices.
    Some(Vec<u32>),
}

impl Subs {
    /// Mips `first..first + mips` of slices `slice..slice + slices` of a
    /// texture of `of` (mips, slices); counts of -1 are the rest. All of it
    /// if that is what they come to, or if they name nothing.
    pub(crate) fn part(
        (first, mips): (u32, u32),
        (slice, slices): (u32, u32),
        of: (u32, u32),
    ) -> Subs {
        if first >= of.0 || slice >= of.1 || mips == 0 || slices == 0 {
            return Subs::All;
        }
        let (mips, slices) = (mips.min(of.0 - first), slices.min(of.1 - slice));
        if (mips, slices) == of {
            return Subs::All;
        }
        Subs::Some(
            (slice..slice + slices)
                .flat_map(|s| (first..first + mips).map(move |m| s * of.0 + m))
                .collect(),
        )
    }

    /// Subresource `sub` of a texture of `count`.
    pub(crate) fn one(sub: u32, count: u32) -> Subs {
        if count <= 1 || sub >= count {
            Subs::All
        } else {
            Subs::Some(vec![sub])
        }
    }
}

/// (mips, slices) of a texture whose subresources are told apart; `None` for
/// one whose are not: multi-planar ones (depth with stencil), whose views
/// and copies here are of both planes.
pub(crate) fn shape(d: &D3D12_RESOURCE_DESC) -> Option<(u32, u32)> {
    if matches!(d.Format.0, 19..=22 | 44..=47) {
        return None;
    }
    let slices = if d.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D {
        1
    } else {
        u32::from(d.DepthOrArraySize)
    };
    Some((u32::from(d.MipLevels).max(1), slices.max(1)))
}

/// A change of state a list records: of one subresource or
/// (`D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES`) all.
pub(crate) type Change = (u32, u32, u32);

const ALL: u32 = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;

/// What one list does with a texture's states: for what it uses, the state
/// it expects to find and the one it leaves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Uses {
    All(u32, u32),
    Each(Vec<Option<(u32, u32)>>),
}

impl Uses {
    /// A list that has not used the texture yet, of `count` subresources.
    fn none(count: u32) -> Self {
        Uses::Each(vec![None; count as usize])
    }

    /// Moves `subs` to `wanted` in `uses` (made if there is none yet): what
    /// is to be recorded. The first use of a subresource records nothing,
    /// the list then expects to find it there; unless the use says the
    /// state it comes from (`from`: a barrier of the title's), which the
    /// list then expects to find.
    pub(crate) fn go(
        uses: &mut Option<Uses>,
        count: u32,
        subs: &Subs,
        from: Option<u32>,
        wanted: u32,
    ) -> Vec<Change> {
        let first = |from: Option<u32>| (from.unwrap_or(wanted), wanted);
        let change = |sub: u32, (before, after): (u32, u32)| {
            (before != after).then_some((sub, before, after))
        };
        match (&mut *uses, subs) {
            (None, Subs::All) => {
                let (find, leave) = first(from);
                *uses = Some(Uses::All(find, leave));
                change(ALL, (find, leave)).into_iter().collect()
            }
            (Some(Uses::All(_, last)), Subs::All) => {
                let before = std::mem::replace(last, wanted);
                change(ALL, (before, wanted)).into_iter().collect()
            }
            _ => {
                let each = match uses.take().unwrap_or_else(|| Uses::none(count)) {
                    Uses::All(a, b) => vec![Some((a, b)); count as usize],
                    Uses::Each(each) => each,
                };
                let mut each = each;
                let mut changes = Vec::new();
                let mut go = |sub: u32| {
                    let Some(slot) = each.get_mut(sub as usize) else {
                        return;
                    };
                    let before = match slot {
                        Some((_, last)) => std::mem::replace(last, wanted),
                        None => {
                            *slot = Some(first(from));
                            from.unwrap_or(wanted)
                        }
                    };
                    changes.extend(change(sub, (before, wanted)));
                };
                match subs {
                    Subs::All => (0..count).for_each(&mut go),
                    Subs::Some(subs) => subs.iter().copied().for_each(&mut go),
                }
                // Used whole and alike: one state again.
                *uses = Some(match each.first() {
                    Some(&Some(one)) if each.iter().all(|s| *s == Some(one)) => {
                        Uses::All(one.0, one.1)
                    }
                    _ => Uses::Each(each),
                });
                joined(changes, count)
            }
        }
    }

    /// The state all of the texture is in by now, if it is one.
    #[cfg(test)]
    fn whole(&self) -> Option<u32> {
        match self {
            Uses::All(_, last) => Some(*last),
            Uses::Each(each) => {
                let one = (*each.first()?)?.1;
                each.iter().all(|s| s.is_some_and(|s| s.1 == one)).then_some(one)
            }
        }
    }

    /// From `real` to what the list expects to find, and the states it
    /// leaves.
    pub(crate) fn enter(&self, real: &States) -> (Vec<Change>, States) {
        match self {
            Uses::All(find, leave) => (between(real, &States::All(*find)), States::All(*leave)),
            Uses::Each(each) => {
                let find = each.iter().zip(0..).map(|(s, i)| s.map_or(real.at(i), |s| s.0)).collect();
                let leave = each.iter().zip(0..).map(|(s, i)| s.map_or(real.at(i), |s| s.1)).collect();
                (between(real, &States::of(find)), States::of(leave))
            }
        }
    }
}

/// The changes from `a` to `b`.
pub(crate) fn between(a: &States, b: &States) -> Vec<Change> {
    match (a, b) {
        (States::All(a), States::All(b)) => {
            if a == b {
                Vec::new()
            } else {
                vec![(ALL, *a, *b)]
            }
        }
        (States::Each(each), _) | (_, States::Each(each)) => {
            let count = each.len() as u32;
            let changes = (0..count)
                .map(|i| (i, a.at(i), b.at(i)))
                .filter(|(_, a, b)| a != b)
                .collect();
            joined(changes, count)
        }
    }
}

/// `changes` as one, if they are the same for all `count` subresources.
fn joined(changes: Vec<Change>, count: u32) -> Vec<Change> {
    match changes.first() {
        Some(&(_, a, b))
            if changes.len() == count as usize && changes.iter().all(|c| (c.1, c.2) == (a, b)) =>
        {
            vec![(ALL, a, b)]
        }
        _ => changes,
    }
}

/// `changes` of `texture` as barriers (which borrow it).
pub(crate) fn barriers(texture: &ID3D12Resource, changes: &[Change]) -> Vec<D3D12_RESOURCE_BARRIER> {
    let state = |s: u32| D3D12_RESOURCE_STATES(s as i32);
    changes
        .iter()
        .map(|&(sub, before, after)| {
            let mut barrier = crate::host::transition(texture, state(before), state(after));
            // SAFETY: a transition barrier, just made; plain numbers.
            unsafe { (*barrier.Anonymous.Transition).Subresource = sub };
            barrier
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRV: u32 = 0xc0;
    const UAV: u32 = 0x8;
    const RT: u32 = 0x4;

    #[test]
    fn parts_are_indices_mips_first() {
        assert_eq!(Subs::part((1, 2), (1, 1), (4, 3)), Subs::Some(vec![5, 6]));
        assert_eq!(Subs::part((3, u32::MAX), (0, u32::MAX), (4, 2)), Subs::Some(vec![3, 7]));
        assert_eq!(Subs::part((0, u32::MAX), (0, 3), (4, 3)), Subs::All);
        assert_eq!(Subs::part((4, 1), (0, 1), (4, 3)), Subs::All, "nothing named");
        assert_eq!(Subs::one(2, 3), Subs::Some(vec![2]));
        assert_eq!(Subs::one(0, 1), Subs::All);
    }

    #[test]
    fn a_texture_used_whole_has_one_state() {
        let mut u = None;
        assert_eq!(Uses::go(&mut u, 4, &Subs::All, None, SRV), []);
        assert_eq!(Uses::go(&mut u, 4, &Subs::All, None, SRV), []);
        assert_eq!(Uses::go(&mut u, 4, &Subs::All, None, RT), [(ALL, SRV, RT)]);
        let (changes, left) = u.unwrap().enter(&States::All(UAV));
        assert_eq!(changes, [(ALL, UAV, SRV)]);
        assert_eq!(left, States::All(RT));
    }

    /// A chain that reads mip n and writes mip n + 1: each mip changes once,
    /// and no mip is ever asked to be in two states.
    #[test]
    fn a_mip_chain_moves_one_mip_at_a_time() {
        let mut u = None;
        let mut recorded = Vec::new();
        for n in 0..3 {
            recorded.extend(Uses::go(&mut u, 4, &Subs::one(n, 4), None, SRV));
            recorded.extend(Uses::go(&mut u, 4, &Subs::one(n + 1, 4), None, UAV));
        }
        assert_eq!(recorded, [(1, UAV, SRV), (2, UAV, SRV)]);
        // Then the whole chain is read: only the last mip is not yet.
        assert_eq!(Uses::go(&mut u, 4, &Subs::All, None, SRV), [(3, UAV, SRV)]);
        let u = u.unwrap();
        assert_eq!(u.whole(), Some(SRV));
        let (changes, left) = u.enter(&States::All(SRV));
        assert_eq!(changes, [(1, SRV, UAV), (2, SRV, UAV), (3, SRV, UAV)]);
        assert_eq!(left, States::All(SRV));
    }

    /// A barrier of the title's says where the texture comes from: it is
    /// recorded in its place, and the list expects that state.
    #[test]
    fn a_title_barrier_is_recorded_where_it_stands() {
        let mut u = None;
        assert_eq!(Uses::go(&mut u, 1, &Subs::All, Some(SRV), RT), [(ALL, SRV, RT)]);
        let (changes, left) = u.unwrap().enter(&States::All(RT));
        assert_eq!(changes, [(ALL, RT, SRV)]);
        assert_eq!(left, States::All(RT));
        // Of one mip: the others stay as they really are.
        let mut u = None;
        assert_eq!(Uses::go(&mut u, 2, &Subs::one(1, 2), Some(SRV), RT), [(1, SRV, RT)]);
        let (changes, left) = u.unwrap().enter(&States::All(SRV));
        assert_eq!(changes, []);
        assert_eq!(left, States::Each(vec![SRV, RT]));
        // Where the tracked state is another than it says, that one counts.
        let mut u = Some(Uses::All(UAV, UAV));
        assert_eq!(Uses::go(&mut u, 2, &Subs::All, Some(SRV), RT), [(ALL, UAV, RT)]);
    }

    #[test]
    fn states_join_when_they_are_alike() {
        let each = States::Each(vec![SRV, RT]);
        assert_eq!(between(&each, &States::All(SRV)), [(1, RT, SRV)]);
        assert_eq!(between(&States::All(SRV), &each), [(1, SRV, RT)]);
        let mut u = None;
        Uses::go(&mut u, 2, &Subs::one(0, 2), None, RT);
        Uses::go(&mut u, 2, &Subs::one(1, 2), None, RT);
        assert_eq!(u, Some(Uses::All(RT, RT)));
        assert_eq!(u.unwrap().enter(&each).0, [(0, SRV, RT)]);
    }

    /// Depth with a stencil plane is not told apart, 3D slices are no
    /// subresources.
    #[test]
    fn shapes() {
        let mut d = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            DepthOrArraySize: 3,
            MipLevels: 4,
            ..Default::default()
        };
        d.Format.0 = 28;
        assert_eq!(shape(&d), Some((4, 3)));
        d.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE3D;
        assert_eq!(shape(&d), Some((4, 1)));
        d.Format.0 = 44;
        assert_eq!(shape(&d), None);
    }
}
