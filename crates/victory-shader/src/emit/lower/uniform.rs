//! What is the same in every lane of a wave.

use super::*;

/// VGPR values that are the same in every lane of the console wave holding them (`all` is
/// [`all_lane_blocks`], `threads` the group's): made, lane by lane, of scalars, constants and
/// such values, or read from one dword of shared memory, by an instruction all the lanes that
/// arrive run, or all or none of them (no guard, or one they agree on). The compiler keeps
/// wave-uniform data in VGPRs and fetches it with `v_readfirstlane`.
///
/// - The wave number is one (`tid >> 6` of the entry `v0`, in a group whose x size is a
///   multiple of 64).
/// - So is what `ds_read_b32` reads at such an address. Two host waves read one after the other
///   what the console's wave reads at once, but the group meets at every shared-memory access
///   then, or round a branch that only reads ([`Lower::split_lds`]): no write comes between.
/// - A value joined by a phi is where every predecessor is a block all lanes run, or one all the
///   arriving lanes or none of them come from (their way there is made of conditions they agree
///   on: SCC values, compares of such values, masks joined of those). At a loop's header too:
///   a value carried round is taken to be one, and the assumption dropped once an argument
///   is not (a greatest fixed point).
/// - A value made in a loop is not one outside it when the lanes may leave the loop in
///   different rounds: each then holds its own last round's.
///
/// After fxceless/victory-fork #156 (the structure builder's scans over the group's waves).
pub(super) fn lane_uniform(p: &Program, st: &Structured, all: &[bool], threads: Option<[u32; 3]>) -> HashSet<Value> {
    let is_mask = |v: Value| st.masks.contains_key(&v) || st.highs.contains_key(&v);
    let entry_v0 = p.inputs.iter().find(|(r, _)| *r == Reg::V(0)).map(|x| x.1);
    let alone = threads.is_some_and(|t| t.iter().product::<u32>() <= 64);
    let waves_whole = threads.is_some_and(|t| t[0] != 0 && t[0].is_multiple_of(64));
    let order = crate::ir::reverse_postorder(&p.blocks);
    let loops = natural_loops(p, &order);
    // (block defining a value, for the uses outside its loops)
    let mut made_in: HashMap<Value, usize> = HashMap::new();
    let mut used_in: HashMap<Value, Vec<usize>> = HashMap::new();
    // the V-register phis, and the lane-mask phis, assumed alike until shown not to be
    let mut assumed_v: HashSet<Value> = HashSet::new();
    let mut assumed_m: HashSet<Value> = HashSet::new();
    // and the compares (a loop's header comes before the compare that closes the loop)
    let mut assumed_c: HashSet<Value> = HashSet::new();
    for (b, block) in p.blocks.iter().enumerate() {
        for phi in &block.phis {
            made_in.insert(phi.value, b);
            for &a in &phi.args {
                used_in.entry(a).or_default().push(b);
            }
            if matches!(phi.reg, Reg::V(_)) {
                assumed_v.insert(phi.value);
            }
            if st.masks.contains_key(&phi.value) {
                assumed_m.insert(phi.value);
            }
        }
        for inst in &block.insts {
            for w in &inst.writes {
                made_in.insert(w.1, b);
            }
            if inst.name().starts_with("v_cmp") {
                assumed_c.extend(inst.writes.iter().map(|w| w.1));
            }
            for r in &inst.reads {
                used_in.entry(r.1).or_default().push(b);
            }
        }
    }
    // values found to leave a loop with lanes of different rounds
    let mut parted: HashSet<Value> = HashSet::new();
    // Whether a condition is the same in every lane of the wave.
    let agreed = |b: Bool, same: &HashSet<Value>, cmp: &HashSet<Value>, masks: &HashSet<Value>| {
        st.bools.support(b).iter().all(|a| match a {
            Atom::Uniform(_) => true,
            Atom::Lane(v) => cmp.contains(v) || same.contains(v),
            Atom::Phi(v) => masks.contains(v),
            _ => false,
        })
    };
    loop {
        let mut same: HashSet<Value> = HashSet::new();
        // values written by compares whose operands are all alike (the lane's bit is the wave's)
        let mut cmp: HashSet<Value> = HashSet::new();
        for &b in &order {
            let block = &p.blocks[b];
            let whole = block.preds.iter().all(|&q| all.get(q).copied().unwrap_or(false));
            // every predecessor is left for here by all of the arriving lanes or none of them
            let together = block.preds.iter().all(|&q| {
                agreed(st.reach[q], &same, &assumed_c, &assumed_m)
                    && match &st.exits[q] {
                        LaneExit::Branch { when, .. } => agreed(*when, &same, &assumed_c, &assumed_m),
                        LaneExit::Indirect => false,
                        _ => true,
                    }
            });
            for phi in &block.phis {
                let one = matches!(phi.reg, Reg::V(_))
                    && (whole && phi.args.iter().all(|a| same.contains(a))
                        // (its arguments are checked once the pass is done)
                        || together && assumed_v.contains(&phi.value));
                if one && !parted.contains(&phi.value) {
                    same.insert(phi.value);
                }
            }
            for (i, inst) in block.insts.iter().enumerate() {
                let name = inst.name();
                let guard = st.guards[b][i];
                if guard.is_some_and(|g| !agreed(g, &same, &assumed_c, &assumed_m)) {
                    continue; // lanes outside EXEC keep another value
                }
                let ops: Vec<Operand> = match &inst.fields {
                    Fields::Valu(v)
                        if v.dpp.is_none()
                            && v.sdwa.is_none()
                            && !name.starts_with("v_mbcnt")
                            && !name.starts_with("v_cmpx")
                            && !name.contains("lane")
                            && !name.contains("movrel")
                            && !name.contains("swaprel") =>
                    {
                        v.src.clone()
                    }
                    // one dword of shared memory, the same for every lane once no other
                    // wave of the group may write it still (the halves of a wave alone in
                    // its group meet between its writes and its reads)
                    Fields::Ds { dst: Some(_), addr: Some(addr), gds: false, .. }
                        if name == "ds_read_b32" && (alone || lds_settled(p, (b, i))) =>
                    {
                        vec![*addr]
                    }
                    _ => continue,
                };
                let wave_number = waves_whole
                    && name == "v_lshrrev_b32"
                    && matches!(&ops[..], [Operand::Const(134..=159), Operand::Vector { reg: 0, .. }])
                    && entry_v0.is_some()
                    && inst.read(Reg::V(0)) == entry_v0;
                let alike = wave_number
                    || ops.iter().all(|op| match *op {
                        Operand::Vector { reg, dwords } => {
                            // every dword of the operand (not the old destination behind it,
                            // which is there for lanes outside EXEC)
                            let of = reg..reg.saturating_add(dwords);
                            let read: Vec<Value> = inst
                                .reads
                                .iter()
                                .filter(|r| matches!(r.0, Reg::V(x) if of.contains(&x)))
                                .map(|r| r.1)
                                .collect();
                            !read.is_empty() && read.iter().all(|x| same.contains(x))
                        }
                        // a scalar is one value; a lane mask is not, unless a compare of such
                        // values made it
                        Operand::Scalar { code, .. } if code != NULL => {
                            inst.read(Reg::S(code)).is_none_or(|x| !is_mask(x) || same.contains(&x))
                        }
                        _ => true,
                    });
                if !alike || !implicit_reads_alike(inst, &ops, &same, &is_mask) {
                    continue;
                }
                // where the lanes agree not to run it, they keep what they had
                if guard.is_some()
                    && !inst.writes.iter().all(|w| match w.0 {
                        Reg::V(_) => inst.read(w.0).is_some_and(|old| same.contains(&old)),
                        _ => true,
                    })
                {
                    continue;
                }
                if name.starts_with("v_cmp") {
                    cmp.extend(inst.writes.iter().map(|w| w.1));
                }
                same.extend(
                    inst.writes.iter().filter(|w| w.0 != Reg::Scc && !parted.contains(&w.1)).map(|w| w.1),
                );
            }
        }
        // what was assumed: dropped where it did not hold
        let mut changed = false;
        for block in &p.blocks {
            for phi in &block.phis {
                if assumed_v.contains(&phi.value)
                    && same.contains(&phi.value)
                    && !phi.args.iter().all(|a| same.contains(a))
                {
                    assumed_v.remove(&phi.value);
                    changed = true;
                }
                if assumed_m.contains(&phi.value)
                    && !phi
                        .args
                        .iter()
                        .all(|a| st.masks.get(a).is_some_and(|&m| agreed(m, &same, &assumed_c, &assumed_m)))
                {
                    assumed_m.remove(&phi.value);
                    changed = true;
                }
            }
        }
        let held = assumed_c.len();
        assumed_c.retain(|v| cmp.contains(v));
        changed |= assumed_c.len() != held;
        // loops some lanes of a wave may leave before others
        for body in &loops {
            let agree = (0..p.blocks.len()).filter(|&q| body[q]).all(|q| {
                agreed(st.reach[q], &same, &assumed_c, &assumed_m)
                    && match &st.exits[q] {
                        LaneExit::Branch { when, .. } => agreed(*when, &same, &assumed_c, &assumed_m),
                        LaneExit::Indirect => false,
                        _ => true,
                    }
            });
            if agree {
                continue;
            }
            for &v in &same {
                let made = made_in.get(&v).is_some_and(|&b| body[b]);
                let out = |v: Value| used_in.get(&v).is_some_and(|u| u.iter().any(|&b| !body[b]));
                if made && out(v) && parted.insert(v) {
                    changed = true;
                }
            }
        }
        if !changed {
            return same;
        }
    }
}

/// The natural loops' blocks: for every edge back to a block that dominates its start, the
/// blocks that come to the edge without passing that block.
pub(super) fn natural_loops(p: &Program, order: &[usize]) -> Vec<Vec<bool>> {
    let n = p.blocks.len();
    let mut place = vec![usize::MAX; n];
    for (k, &b) in order.iter().enumerate() {
        place[b] = k;
    }
    let mut out = Vec::new();
    for (header, block) in p.blocks.iter().enumerate() {
        for &latch in &block.preds {
            if place[latch] == usize::MAX || place[latch] < place[header] {
                continue;
            }
            let mut body = vec![false; n];
            body[header] = true;
            let mut todo = vec![latch];
            while let Some(b) = todo.pop() {
                if !std::mem::replace(&mut body[b], true) {
                    todo.extend(&p.blocks[b].preds);
                }
            }
            out.push(body);
        }
    }
    out
}

/// Whether what `inst` reads without naming it as a source (`ops`) is the same in every lane
/// holding it (`same`): the accumulator of `v_fmac`/`v_mac`/`v_dot*c`, the half of the old
/// destination a 16-bit write keeps, VCC. The old value of a destination written whole is
/// listed among the reads for lanes outside EXEC only; these lanes are all in it.
pub(super) fn implicit_reads_alike(
    inst: &Inst,
    ops: &[Operand],
    same: &HashSet<Value>,
    is_mask: &impl Fn(Value) -> bool,
) -> bool {
    let name = inst.name();
    let named = |r: u8| {
        ops.iter().any(|op| {
            matches!(*op, Operand::Vector { reg, dwords } if r >= reg && r < reg.saturating_add(dwords))
        })
    };
    // Instructions that write one 32-bit (or wider) result made of their sources alone.
    let sixteen = ["_f16", "_u16", "_i16", "_b16", "f16_", "i16_", "u16_"];
    let keeps = name.contains("mac")
        || name.contains("dot2c")
        || name.contains("dot4c")
        || name.contains("dot8c")
        || name.contains("_mix")
        || sixteen.iter().any(|t| name.contains(t));
    let written = |r: u8| inst.writes.iter().any(|w| w.0 == Reg::V(r));
    inst.reads.iter().all(|&(reg, v)| match reg {
        Reg::V(r) if named(r) => true, // looked at as a source
        Reg::V(r) if !keeps && written(r) => true,
        Reg::V(_) => same.contains(&v),
        Reg::S(code) if code == EXEC || code == EXEC + 1 => {
            !ops.iter().any(|op| matches!(*op, Operand::Scalar { code: c, .. } if c == code))
        }
        Reg::S(_) => !is_mask(v) || same.contains(&v),
        _ => true,
    })
}

/// Whether the shared memory a read at `read` (block, instruction) sees can no longer change
/// while the thread group runs it: no shared-memory write follows it on any path (the read's
/// own wave writes nothing after), and every write before it reaches it only through an
/// `s_barrier` (the group's other waves have written theirs before any wave reads). Then two
/// host waves that read the same address get the same dwords, whenever each gets there.
pub(super) fn lds_settled(p: &Program, read: (usize, usize)) -> bool {
    let writes = |i: &Inst| {
        let n = i.name();
        matches!(i.fields, Fields::Ds { .. })
            && !(n.starts_with("ds_read")
                || n.starts_with("ds_swizzle")
                || n.starts_with("ds_permute")
                || n.starts_with("ds_bpermute")
                || n == "ds_nop")
    };
    let barrier = |i: &Inst| i.name() == "s_barrier";
    let n = p.blocks.len();
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (b, block) in p.blocks.iter().enumerate() {
        for &q in &block.preds {
            if q < n && !succ[q].contains(&b) {
                succ[q].push(b);
            }
        }
    }
    // a write after the read, on any path (around a loop too)
    let after = (read.0, read.1 + 1);
    if reaches(p, &succ, after, &|b, i| writes(&p.blocks[b].insts[i]), None) {
        return false;
    }
    // a write before it that reaches it without a barrier between
    for (b, block) in p.blocks.iter().enumerate() {
        for (i, inst) in block.insts.iter().enumerate() {
            let hit = |bb, ii| (bb, ii) == read;
            if writes(inst) && reaches(p, &succ, (b, i + 1), &hit, Some(&barrier)) {
                return false;
            }
        }
    }
    true
}

/// Whether a path from instruction `start` (block, index) on meets an instruction `hit`
/// holds, before one `cut` holds (paths that pass such an instruction stop there).
pub(super) fn reaches(
    p: &Program,
    succ: &[Vec<usize>],
    start: (usize, usize),
    hit: &dyn Fn(usize, usize) -> bool,
    cut: Option<&dyn Fn(&Inst) -> bool>,
) -> bool {
    let mut entered = vec![false; p.blocks.len()];
    let mut stack = vec![start];
    while let Some((b, from)) = stack.pop() {
        let mut stopped = false;
        for (i, inst) in p.blocks[b].insts.iter().enumerate().skip(from) {
            if hit(b, i) {
                return true;
            }
            if cut.is_some_and(|cut| cut(inst)) {
                stopped = true;
                break;
            }
        }
        if !stopped {
            for &s in &succ[b] {
                if !entered[s] {
                    entered[s] = true;
                    stack.push((s, 0));
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod settled_tests {
    /// A wave64 program of `words` (`crate::ir::build`'s console form).
    fn wave64(words: &[u32]) -> crate::ir::Program {
        let code: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        crate::ir::build(&code, true, 8).unwrap()
    }

    /// DS `op vdst, addr, data0` at offset 0.
    const fn ds(op: u32, vdst: u32, addr: u32, data0: u32) -> [u32; 2] {
        [0x36 << 26 | op << 18, vdst << 24 | data0 << 8 | addr]
    }
    const V_MOV_V1_S0: u32 = 0x7E02_0200; // v_mov_b32 v1, s0
    const S_BARRIER: u32 = 0xBF8A_0000;
    const DS_WRITE_B32: u32 = 0x0D;
    const DS_READ_B32: u32 = 0x36;

    /// The site of the program's `ds_read_b32` and the value it loads into v3.
    fn the_read(p: &crate::ir::Program) -> ((usize, usize), crate::ir::Value) {
        for (b, block) in p.blocks.iter().enumerate() {
            for (i, inst) in block.insts.iter().enumerate() {
                if inst.name() == "ds_read_b32" {
                    return ((b, i), inst.write(crate::ir::Reg::V(3)).unwrap());
                }
            }
        }
        panic!("no ds_read_b32");
    }

    #[test]
    fn a_shared_read_at_a_uniform_address_after_a_barrier_is_uniform() {
        // every wave writes, the group meets, then each reads one address
        let mut words = vec![V_MOV_V1_S0];
        words.extend(ds(DS_WRITE_B32, 0, 1, 2));
        words.push(S_BARRIER);
        words.extend(ds(DS_READ_B32, 3, 1, 0));
        words.push(0xBF81_0000);
        let p = wave64(&words);
        let st = crate::structure::structure(&p).unwrap();
        let all = vec![true; p.blocks.len()];
        let (site, loaded) = the_read(&p);
        assert!(super::lds_settled(&p, site));
        assert!(super::lane_uniform(&p, &st, &all, Some([128, 1, 1])).contains(&loaded));
    }

    #[test]
    fn a_shared_read_another_wave_may_race_is_not_uniform() {
        // no barrier between the writes and the read: another wave may
        // still be writing while the two halves of this one read
        let mut words = vec![V_MOV_V1_S0];
        words.extend(ds(DS_WRITE_B32, 0, 1, 2));
        words.extend(ds(DS_READ_B32, 3, 1, 0));
        words.push(0xBF81_0000);
        let p = wave64(&words);
        let st = crate::structure::structure(&p).unwrap();
        let all = vec![true; p.blocks.len()];
        let (site, loaded) = the_read(&p);
        assert!(!super::lds_settled(&p, site));
        assert!(!super::lane_uniform(&p, &st, &all, Some([128, 1, 1])).contains(&loaded));
        // a write after the read: the second half may read it, the first not
        let mut words = vec![V_MOV_V1_S0];
        words.extend(ds(DS_WRITE_B32, 0, 1, 2));
        words.push(S_BARRIER);
        words.extend(ds(DS_READ_B32, 3, 1, 0));
        words.extend(ds(DS_WRITE_B32, 0, 1, 3));
        words.push(0xBF81_0000);
        let p = wave64(&words);
        let (site, _) = the_read(&p);
        assert!(!super::lds_settled(&p, site));
    }
}
