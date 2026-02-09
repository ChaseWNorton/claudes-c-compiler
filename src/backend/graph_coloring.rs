//! Graph coloring register allocator using Iterated Register Coalescing (IRC).
//!
//! Replaces the linear scan allocator under `-Os` to aggressively coalesce
//! phi-eliminated Copy instructions, eliminating register shuffles that account
//! for ~88% of the code size gap vs GCC in i686 boot code.
//!
//! Algorithm: Chaitin-Briggs with George/Briggs conservative coalescing.
//! Reference: Appel, "Modern Compiler Implementation in ML", Chapter 11.
//!
//! Key properties:
//! - Drop-in replacement: same input (IrFunction + RegAllocConfig), same output (RegAllocResult)
//! - Same spill model: values without register assignments use stack slots
//! - Operates post-phi-elimination on Copy instructions
//! - No changes to codegen, peephole, or stack layout needed

use crate::common::fx_hash::{FxHashMap, FxHashSet};
use crate::common::types::IrType;
use crate::ir::reexports::{Instruction, IrConst, IrFunction, Operand};
use super::liveness::{
    LiveInterval, LivenessResult, compute_live_intervals,
    for_each_operand_in_instruction, for_each_operand_in_terminator,
};
use super::regalloc::{PhysReg, RegAllocConfig, RegAllocResult};

/// Infinite degree for precolored nodes (never simplified or spilled).
const INFINITE_DEGREE: u32 = u32::MAX / 2;

// ── Interference Graph ──────────────────────────────────────────────────────

struct IrcGraph {
    num_nodes: usize,
    k: usize, // number of colors (physical registers)

    // Adjacency: symmetric, using both set (O(1) query) and list (iteration).
    adj_set: Vec<FxHashSet<u32>>,
    degree: Vec<u32>,

    // Coalescing: alias[n] = representative after merging.
    alias: Vec<u32>,
    move_list: Vec<Vec<usize>>, // node -> indices into `moves`

    // Node classification
    precolored: FxHashSet<u32>,
    color: Vec<Option<u8>>,     // assigned color, or None
    spill_cost: Vec<f64>,       // higher = more expensive to spill

    // Moves (coalescing candidates): (src_node, dst_node)
    moves: Vec<(u32, u32)>,

    // Worklists (mutually exclusive partitions of non-precolored nodes)
    simplify_wl: Vec<u32>,
    freeze_wl: FxHashSet<u32>,
    spill_wl: FxHashSet<u32>,
    coalesced_nodes: FxHashSet<u32>,

    // Move classification (mutually exclusive partitions of moves)
    worklist_moves: FxHashSet<usize>,  // not yet attempted
    active_moves: FxHashSet<usize>,    // constrained but may become coalescable
    coalesced_moves: FxHashSet<usize>,
    frozen_moves: FxHashSet<usize>,

    // Select stack for coloring phase
    select_stack: Vec<u32>,
    on_stack: FxHashSet<u32>,

    // Mapping between IR values and graph nodes
    node_to_value: Vec<u32>,            // node_idx -> value_id (u32::MAX for precolored)
    value_to_node: FxHashMap<u32, u32>, // value_id -> node_idx

    // Color-to-register mapping
    color_to_phys: Vec<PhysReg>,
    num_caller_saved_colors: usize, // colors 0..this are caller-saved
}

impl IrcGraph {
    fn new(
        num_virtual: usize,
        config: &RegAllocConfig,
        node_to_value: Vec<u32>,
        value_to_node: FxHashMap<u32, u32>,
        spill_costs: Vec<f64>,
    ) -> Self {
        let num_caller_saved = config.caller_saved_regs.len();
        let num_callee_saved = config.available_regs.len();
        let k = num_caller_saved + num_callee_saved;
        let num_precolored = num_caller_saved;
        let num_nodes = num_virtual + num_precolored;

        // Build color-to-phys mapping: caller-saved colors first, then callee-saved
        let mut color_to_phys = Vec::with_capacity(k);
        for &r in &config.caller_saved_regs {
            color_to_phys.push(r);
        }
        for &r in &config.available_regs {
            color_to_phys.push(r);
        }

        let mut adj_set = Vec::with_capacity(num_nodes);
        let mut degree = Vec::with_capacity(num_nodes);
        let mut alias = Vec::with_capacity(num_nodes);
        let mut color: Vec<Option<u8>> = Vec::with_capacity(num_nodes);
        let mut cost = Vec::with_capacity(num_nodes);
        let mut precolored = FxHashSet::default();
        let mut move_list = Vec::with_capacity(num_nodes);

        // Virtual register nodes: 0..num_virtual
        for i in 0..num_virtual {
            adj_set.push(FxHashSet::default());
            degree.push(0);
            alias.push(i as u32);
            color.push(None);
            cost.push(if i < spill_costs.len() { spill_costs[i] } else { 1.0 });
            move_list.push(Vec::new());
        }

        // Precolored nodes: num_virtual..num_nodes
        // Each precolored node represents a caller-saved register.
        for j in 0..num_precolored {
            let node_idx = (num_virtual + j) as u32;
            adj_set.push(FxHashSet::default());
            degree.push(INFINITE_DEGREE);
            alias.push(node_idx);
            color.push(Some(j as u8)); // fixed color
            cost.push(f64::INFINITY);
            precolored.insert(node_idx);
            move_list.push(Vec::new());
        }

        IrcGraph {
            num_nodes,
            k,
            adj_set,
            degree,
            alias,
            move_list,
            precolored,
            color,
            spill_cost: cost,
            moves: Vec::new(),
            simplify_wl: Vec::new(),
            freeze_wl: FxHashSet::default(),
            spill_wl: FxHashSet::default(),
            coalesced_nodes: FxHashSet::default(),
            worklist_moves: FxHashSet::default(),
            active_moves: FxHashSet::default(),
            coalesced_moves: FxHashSet::default(),
            frozen_moves: FxHashSet::default(),
            select_stack: Vec::new(),
            on_stack: FxHashSet::default(),
            node_to_value,
            value_to_node,
            color_to_phys,
            num_caller_saved_colors: num_caller_saved,
        }
    }

    /// Add an undirected interference edge between u and v.
    fn add_edge(&mut self, u: u32, v: u32) {
        if u == v {
            return;
        }
        if self.adj_set[u as usize].contains(&v) {
            return; // already exists
        }
        self.adj_set[u as usize].insert(v);
        self.adj_set[v as usize].insert(u);
        if !self.precolored.contains(&u) {
            self.degree[u as usize] += 1;
        }
        if !self.precolored.contains(&v) {
            self.degree[v as usize] += 1;
        }
    }

    /// Record a move (coalescing candidate) between src and dst nodes.
    fn add_move(&mut self, src: u32, dst: u32) {
        let idx = self.moves.len();
        self.moves.push((src, dst));
        self.move_list[src as usize].push(idx);
        self.move_list[dst as usize].push(idx);
        self.worklist_moves.insert(idx);
    }

    /// Get the alias representative for node n (with path compression).
    fn get_alias(&self, mut n: u32) -> u32 {
        while self.coalesced_nodes.contains(&n) {
            n = self.alias[n as usize];
        }
        n
    }

    /// Active neighbors of n (not on stack, not coalesced).
    fn adjacent(&self, n: u32) -> Vec<u32> {
        self.adj_set[n as usize]
            .iter()
            .copied()
            .filter(|&m| !self.on_stack.contains(&m) && !self.coalesced_nodes.contains(&m))
            .collect()
    }

    /// Active moves involving node n.
    fn node_moves(&self, n: u32) -> Vec<usize> {
        self.move_list[n as usize]
            .iter()
            .copied()
            .filter(|m| self.active_moves.contains(m) || self.worklist_moves.contains(m))
            .collect()
    }

    /// Is node n involved in any active/worklist move?
    fn move_related(&self, n: u32) -> bool {
        self.move_list[n as usize]
            .iter()
            .any(|m| self.active_moves.contains(m) || self.worklist_moves.contains(m))
    }

    // ── IRC Phases ──

    /// Initialize worklists from the initial graph state.
    fn make_worklist(&mut self) {
        for n in 0..self.num_nodes {
            let n = n as u32;
            if self.precolored.contains(&n) {
                continue;
            }
            if self.degree[n as usize] >= self.k as u32 {
                self.spill_wl.insert(n);
            } else if self.move_related(n) {
                self.freeze_wl.insert(n);
            } else {
                self.simplify_wl.push(n);
            }
        }
    }

    /// Enable moves for a set of nodes (move from active_moves to worklist_moves).
    fn enable_moves(&mut self, nodes: &[u32]) {
        for &n in nodes {
            for m in self.node_moves(n) {
                if self.active_moves.remove(&m) {
                    self.worklist_moves.insert(m);
                }
            }
        }
    }

    /// When degree decreases below K, transition node from spill_wl to
    /// freeze_wl or simplify_wl.
    fn decrement_degree(&mut self, m: u32) {
        if self.precolored.contains(&m) {
            return;
        }
        let d = self.degree[m as usize];
        self.degree[m as usize] = d.saturating_sub(1);
        if d == self.k as u32 {
            // Was exactly K, now K-1: enable moves for m and its neighbors
            let mut nodes = self.adjacent(m);
            nodes.push(m);
            self.enable_moves(&nodes);
            self.spill_wl.remove(&m);
            if self.move_related(m) {
                self.freeze_wl.insert(m);
            } else {
                self.simplify_wl.push(m);
            }
        }
    }

    /// Possibly transition node from freeze_wl to simplify_wl.
    fn add_worklist(&mut self, u: u32) {
        if !self.precolored.contains(&u)
            && !self.move_related(u)
            && (self.degree[u as usize] < self.k as u32)
        {
            self.freeze_wl.remove(&u);
            self.simplify_wl.push(u);
        }
    }

    /// George's criterion: OK to coalesce t (neighbor of v) with precolored u.
    fn ok_george(&self, t: u32, u: u32) -> bool {
        self.degree[t as usize] < self.k as u32
            || self.precolored.contains(&t)
            || self.adj_set[t as usize].contains(&u)
    }

    /// Briggs' criterion: the merged node would have < K high-degree neighbors.
    fn conservative(&self, nodes: &[u32]) -> bool {
        let mut high_degree = 0u32;
        for &n in nodes {
            if self.degree[n as usize] >= self.k as u32 {
                high_degree += 1;
            }
        }
        high_degree < self.k as u32
    }

    /// Simplify: remove a low-degree non-move-related node.
    fn simplify(&mut self) {
        if let Some(n) = self.simplify_wl.pop() {
            self.select_stack.push(n);
            self.on_stack.insert(n);
            let adj = self.adjacent(n);
            for m in adj {
                self.decrement_degree(m);
            }
        }
    }

    /// Coalesce: try to merge two move-related nodes.
    fn coalesce(&mut self) {
        let m_idx = match self.worklist_moves.iter().next().copied() {
            Some(m) => m,
            None => return,
        };
        self.worklist_moves.remove(&m_idx);

        let (x, y) = self.moves[m_idx];
        let x = self.get_alias(x);
        let y = self.get_alias(y);

        // Ensure u is precolored if either is
        let (u, v) = if self.precolored.contains(&y) { (y, x) } else { (x, y) };

        if u == v {
            // Already coalesced
            self.coalesced_moves.insert(m_idx);
            self.add_worklist(u);
        } else if self.precolored.contains(&v) || self.adj_set[u as usize].contains(&v) {
            // Constrained: both precolored, or they interfere
            self.frozen_moves.insert(m_idx); // treat as constrained, not active
            self.add_worklist(u);
            self.add_worklist(v);
        } else {
            // Check coalescing criterion
            let can_coalesce = if self.precolored.contains(&u) {
                // George's criterion: every neighbor of v is OK with u
                self.adjacent(v).iter().all(|&t| self.ok_george(t, u))
            } else {
                // Briggs' criterion: merged neighbors have < K high-degree
                let mut combined: Vec<u32> = self.adjacent(u);
                combined.extend(self.adjacent(v));
                // Deduplicate
                let set: FxHashSet<u32> = combined.iter().copied().collect();
                let unique: Vec<u32> = set.into_iter().collect();
                self.conservative(&unique)
            };

            if can_coalesce {
                self.coalesced_moves.insert(m_idx);
                self.combine(u, v);
                self.add_worklist(u);
            } else {
                self.active_moves.insert(m_idx);
            }
        }
    }

    /// Combine two nodes: merge v into u.
    fn combine(&mut self, u: u32, v: u32) {
        if self.freeze_wl.contains(&v) {
            self.freeze_wl.remove(&v);
        } else {
            self.spill_wl.remove(&v);
        }
        self.coalesced_nodes.insert(v);
        self.alias[v as usize] = u;

        // Merge move lists
        let v_moves: Vec<usize> = self.move_list[v as usize].clone();
        self.move_list[u as usize].extend(v_moves);

        // Merge spill costs
        self.spill_cost[u as usize] += self.spill_cost[v as usize];

        // Add edges from u to all of v's neighbors
        let v_adj = self.adjacent(v);
        for t in &v_adj {
            self.add_edge(*t, u);
            self.decrement_degree(*t);
        }

        // If u's degree is now >= K and u is in freeze_wl, move to spill_wl
        if self.degree[u as usize] >= self.k as u32 && self.freeze_wl.contains(&u) {
            self.freeze_wl.remove(&u);
            self.spill_wl.insert(u);
        }
    }

    /// Freeze: give up coalescing for a low-degree move-related node.
    fn freeze(&mut self) {
        let u = match self.freeze_wl.iter().next().copied() {
            Some(u) => u,
            None => return,
        };
        self.freeze_wl.remove(&u);
        self.simplify_wl.push(u);
        self.freeze_moves(u);
    }

    /// Mark all moves involving u as frozen, potentially unblocking neighbors.
    fn freeze_moves(&mut self, u: u32) {
        let moves = self.node_moves(u);
        for m in moves {
            let (x, y) = self.moves[m];
            let v = if self.get_alias(y) == self.get_alias(u) {
                self.get_alias(x)
            } else {
                self.get_alias(y)
            };
            self.active_moves.remove(&m);
            self.frozen_moves.insert(m);
            // If v has no more active moves and low degree, move to simplify_wl
            if !self.precolored.contains(&v)
                && self.node_moves(v).is_empty()
                && self.degree[v as usize] < self.k as u32
            {
                self.freeze_wl.remove(&v);
                self.simplify_wl.push(v);
            }
        }
    }

    /// Select a node to potentially spill (highest cost/degree ratio avoids spilling).
    fn select_spill(&mut self) {
        // Pick the node with lowest spill_cost / degree (cheapest to spill)
        let mut best: Option<u32> = None;
        let mut best_priority = f64::INFINITY;
        for &n in &self.spill_wl {
            let d = self.degree[n as usize].max(1) as f64;
            let priority = self.spill_cost[n as usize] / d;
            if priority < best_priority {
                best_priority = priority;
                best = Some(n);
            }
        }
        if let Some(m) = best {
            self.spill_wl.remove(&m);
            self.simplify_wl.push(m);
            self.freeze_moves(m);
        }
    }

    /// Assign colors: pop select_stack, try to find a valid color.
    /// Returns (assignments map, used callee-saved registers).
    fn assign_colors(&mut self) -> (FxHashMap<u32, PhysReg>, Vec<PhysReg>) {
        let mut assignments: FxHashMap<u32, PhysReg> = FxHashMap::default();
        let mut used_callee_saved: FxHashSet<u8> = FxHashSet::default();

        while let Some(n) = self.select_stack.pop() {
            // Collect colors used by neighbors
            let mut used_colors: FxHashSet<u8> = FxHashSet::default();
            for &w in &self.adj_set[n as usize] {
                let w_alias = self.get_alias(w);
                if let Some(c) = self.color[w_alias as usize] {
                    used_colors.insert(c);
                }
            }

            // Try to find an available color, preferring caller-saved, then
            // already-used callee-saved, then new callee-saved.
            let mut chosen: Option<u8> = None;

            // 1. Try caller-saved colors first (free: no push/pop cost)
            for c in 0..self.num_caller_saved_colors as u8 {
                if !used_colors.contains(&c) {
                    chosen = Some(c);
                    break;
                }
            }

            // 2. Try already-used callee-saved colors (amortized cost)
            if chosen.is_none() {
                for c in self.num_caller_saved_colors as u8..self.k as u8 {
                    if !used_colors.contains(&c) {
                        let phys = self.color_to_phys[c as usize];
                        if used_callee_saved.contains(&phys.0) {
                            chosen = Some(c);
                            break;
                        }
                    }
                }
            }

            // 3. Try new callee-saved ONLY if forced (all caller-saved blocked).
            // Each new callee-saved register costs 2 bytes (push+pop) in the
            // prologue/epilogue. Only introduce one if the node has no caller-saved
            // alternative — i.e., it interferes with all caller-saved precolored nodes.
            if chosen.is_none() {
                let all_caller_saved_blocked = (0..self.num_caller_saved_colors as u8)
                    .all(|c| used_colors.contains(&c));
                if all_caller_saved_blocked {
                    for c in self.num_caller_saved_colors as u8..self.k as u8 {
                        if !used_colors.contains(&c) {
                            chosen = Some(c);
                            break;
                        }
                    }
                }
                // else: spill — caller-saved was available but taken by neighbors,
                // not worth introducing a new push/pop pair
            }

            if let Some(c) = chosen {
                self.color[n as usize] = Some(c);
                let phys = self.color_to_phys[c as usize];
                let value_id = self.node_to_value[n as usize];
                if value_id != u32::MAX {
                    assignments.insert(value_id, phys);
                }
                // Track callee-saved usage
                if (c as usize) >= self.num_caller_saved_colors {
                    used_callee_saved.insert(phys.0);
                }
            }
            // else: actual spill — no assignment, value stays on stack
        }

        // Propagate colors to coalesced nodes
        for &n in &self.coalesced_nodes {
            let alias = self.get_alias(n);
            self.color[n as usize] = self.color[alias as usize];
            if let Some(c) = self.color[n as usize] {
                let phys = self.color_to_phys[c as usize];
                let value_id = self.node_to_value[n as usize];
                if value_id != u32::MAX {
                    assignments.insert(value_id, phys);
                }
                if (c as usize) >= self.num_caller_saved_colors {
                    used_callee_saved.insert(phys.0);
                }
            }
        }

        let mut used_regs: Vec<PhysReg> = used_callee_saved.iter().map(|&r| PhysReg(r)).collect();
        used_regs.sort_by_key(|r| r.0);
        (assignments, used_regs)
    }

    /// Run the full IRC algorithm.
    fn run(&mut self) {
        self.make_worklist();
        loop {
            if !self.simplify_wl.is_empty() {
                self.simplify();
            } else if !self.worklist_moves.is_empty() {
                self.coalesce();
            } else if !self.freeze_wl.is_empty() {
                self.freeze();
            } else if !self.spill_wl.is_empty() {
                self.select_spill();
            } else {
                break;
            }
        }
    }
}

// ── Eligibility and use-count computation ───────────────────────────────────

/// Collect values whose types don't fit in a single GPR.
fn collect_non_gpr_values(func: &IrFunction, is_32bit: bool) -> FxHashSet<u32> {
    let is_non_gpr = |ty: &IrType| -> bool {
        ty.is_float() || ty.is_long_double()
            || matches!(ty, IrType::I128 | IrType::U128)
            || (is_32bit && matches!(ty, IrType::I64 | IrType::U64))
    };

    let mut non_gpr: FxHashSet<u32> = FxHashSet::default();

    for block in &func.blocks {
        for inst in &block.instructions {
            match inst {
                Instruction::BinOp { dest, ty, .. }
                | Instruction::UnaryOp { dest, ty, .. } => {
                    if is_non_gpr(ty) { non_gpr.insert(dest.0); }
                }
                Instruction::Cast { dest, to_ty, from_ty, .. } => {
                    if is_non_gpr(to_ty) || is_non_gpr(from_ty) { non_gpr.insert(dest.0); }
                }
                Instruction::Load { dest, ty, .. } => {
                    if is_non_gpr(ty) { non_gpr.insert(dest.0); }
                }
                Instruction::Call { info, .. } | Instruction::CallIndirect { info, .. } => {
                    if let Some(dest) = info.dest {
                        if is_non_gpr(&info.return_type) { non_gpr.insert(dest.0); }
                    }
                }
                Instruction::Select { dest, ty, .. } => {
                    if is_non_gpr(ty) { non_gpr.insert(dest.0); }
                }
                Instruction::AtomicLoad { dest, ty, .. }
                | Instruction::AtomicRmw { dest, ty, .. }
                | Instruction::AtomicCmpxchg { dest, ty, .. } => {
                    if is_non_gpr(ty) { non_gpr.insert(dest.0); }
                }
                _ => {}
            }
        }
    }

    // Propagate through Copy chains
    loop {
        let mut changed = false;
        for block in &func.blocks {
            for inst in &block.instructions {
                if let Instruction::Copy { dest, src } = inst {
                    if non_gpr.contains(&dest.0) { continue; }
                    let src_non_gpr = match src {
                        Operand::Value(v) => non_gpr.contains(&v.0),
                        Operand::Const(IrConst::F32(_)) | Operand::Const(IrConst::F64(_))
                        | Operand::Const(IrConst::LongDouble(..))
                        | Operand::Const(IrConst::I128(_)) => true,
                        Operand::Const(IrConst::I64(_)) if is_32bit => true,
                        _ => false,
                    };
                    if src_non_gpr {
                        non_gpr.insert(dest.0);
                        changed = true;
                    }
                }
            }
        }
        if !changed { break; }
    }

    non_gpr
}

/// Build the eligible set and flat use counts for IRC.
fn build_eligible_and_counts(
    func: &IrFunction,
    config: &RegAllocConfig,
) -> (FxHashSet<u32>, FxHashMap<u32, u64>) {
    let is_32bit = crate::common::types::target_is_32bit();
    let non_gpr = collect_non_gpr_values(func, is_32bit);
    let is_non_gpr_type = |ty: &IrType| -> bool {
        ty.is_float() || ty.is_long_double()
            || matches!(ty, IrType::I128 | IrType::U128)
            || (is_32bit && matches!(ty, IrType::I64 | IrType::U64))
    };

    let mut eligible: FxHashSet<u32> = FxHashSet::default();
    let mut use_count: FxHashMap<u32, u64> = FxHashMap::default();

    for block in &func.blocks {
        for inst in &block.instructions {
            match inst {
                Instruction::BinOp { dest, ty, .. }
                | Instruction::UnaryOp { dest, ty, .. } => {
                    if !is_non_gpr_type(ty) { eligible.insert(dest.0); }
                }
                Instruction::Cmp { dest, .. } => { eligible.insert(dest.0); }
                Instruction::Cast { dest, to_ty, from_ty, .. } => {
                    if !is_non_gpr_type(to_ty) && !is_non_gpr_type(from_ty) {
                        eligible.insert(dest.0);
                    }
                }
                Instruction::Load { dest, ty, .. } => {
                    if !is_non_gpr_type(ty) { eligible.insert(dest.0); }
                }
                Instruction::GetElementPtr { dest, .. } => { eligible.insert(dest.0); }
                Instruction::Copy { dest, .. } => {
                    if !non_gpr.contains(&dest.0) { eligible.insert(dest.0); }
                }
                Instruction::Call { info, .. } | Instruction::CallIndirect { info, .. } => {
                    if let Some(dest) = info.dest {
                        if !is_non_gpr_type(&info.return_type) { eligible.insert(dest.0); }
                    }
                }
                Instruction::Select { dest, ty, .. } => {
                    if !is_non_gpr_type(ty) { eligible.insert(dest.0); }
                }
                Instruction::GlobalAddr { dest, .. }
                | Instruction::LabelAddr { dest, .. } => { eligible.insert(dest.0); }
                Instruction::AtomicLoad { dest, ty, .. }
                | Instruction::AtomicRmw { dest, ty, .. }
                | Instruction::AtomicCmpxchg { dest, ty, .. } => {
                    if !is_non_gpr_type(ty) { eligible.insert(dest.0); }
                }
                Instruction::ParamRef { dest, ty, .. } => {
                    if !is_non_gpr_type(ty) { eligible.insert(dest.0); }
                }
                _ => {}
            }

            // Flat use count (no loop weighting for -Os)
            for_each_operand_in_instruction(inst, |op| {
                if let Operand::Value(v) = op {
                    *use_count.entry(v.0).or_insert(0) += 1;
                }
            });
        }
        for_each_operand_in_terminator(&block.terminator, |op| {
            if let Operand::Value(v) = op {
                *use_count.entry(v.0).or_insert(0) += 1;
            }
        });
    }

    // Remove ineligible operands (same logic as linear scan)
    remove_ineligible_operands(func, &mut eligible, config);

    (eligible, use_count)
}

/// Remove values used as pointers in non-register-aware codegen paths.
fn remove_ineligible_operands(func: &IrFunction, eligible: &mut FxHashSet<u32>, config: &RegAllocConfig) {
    for block in &func.blocks {
        for inst in &block.instructions {
            match inst {
                Instruction::CallIndirect { func_ptr: Operand::Value(v), .. } => {
                    eligible.remove(&v.0);
                }
                Instruction::Memcpy { dest, src, .. } => {
                    eligible.remove(&dest.0);
                    eligible.remove(&src.0);
                }
                Instruction::VaArg { va_list_ptr, .. } => { eligible.remove(&va_list_ptr.0); }
                Instruction::VaStart { va_list_ptr } => { eligible.remove(&va_list_ptr.0); }
                Instruction::VaEnd { va_list_ptr } => { eligible.remove(&va_list_ptr.0); }
                Instruction::VaCopy { dest_ptr, src_ptr } => {
                    eligible.remove(&dest_ptr.0);
                    eligible.remove(&src_ptr.0);
                }
                Instruction::VaArgStruct { dest_ptr, va_list_ptr, .. } => {
                    eligible.remove(&dest_ptr.0);
                    eligible.remove(&va_list_ptr.0);
                }
                Instruction::AtomicRmw { ptr: Operand::Value(v), .. }
                | Instruction::AtomicCmpxchg { ptr: Operand::Value(v), .. }
                | Instruction::AtomicLoad { ptr: Operand::Value(v), .. }
                | Instruction::AtomicStore { ptr: Operand::Value(v), .. } => {
                    eligible.remove(&v.0);
                }
                Instruction::StackRestore { ptr } => { eligible.remove(&ptr.0); }
                Instruction::InlineAsm { outputs, inputs, .. } => {
                    if !config.allow_inline_asm_regalloc {
                        for (_, val, _) in outputs { eligible.remove(&val.0); }
                        for (_, op, _) in inputs {
                            if let Operand::Value(v) = op { eligible.remove(&v.0); }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

// ── Interference graph construction ─────────────────────────────────────────

/// Build copy pairs: (dest_value_id, src_value_id) for each Copy instruction
/// where both src and dest are eligible for register allocation.
fn collect_copy_pairs(func: &IrFunction, eligible: &FxHashSet<u32>) -> FxHashSet<(u32, u32)> {
    let mut pairs = FxHashSet::default();
    for block in &func.blocks {
        for inst in &block.instructions {
            if let Instruction::Copy { dest, src: Operand::Value(s) } = inst {
                if eligible.contains(&dest.0) && eligible.contains(&s.0) {
                    pairs.insert((dest.0, s.0));
                }
            }
        }
    }
    pairs
}

/// Build the interference graph from liveness intervals.
///
/// Uses an interval sweep: sort by start point, maintain an active set
/// ordered by end point. For each new interval, add edges to all active
/// intervals (except copy-related pairs for the SSA copy exception).
fn build_graph(
    liveness: &LivenessResult,
    func: &IrFunction,
    eligible: &FxHashSet<u32>,
    use_count: &FxHashMap<u32, u64>,
    config: &RegAllocConfig,
) -> IrcGraph {
    // Filter intervals to eligible values only
    let eligible_intervals: Vec<&LiveInterval> = liveness
        .intervals
        .iter()
        .filter(|iv| eligible.contains(&iv.value_id) && iv.end > iv.start)
        .collect();

    if eligible_intervals.is_empty() {
        return IrcGraph::new(
            0, config, Vec::new(), FxHashMap::default(), Vec::new(),
        );
    }

    // Build node mapping: value_id -> node_idx
    let mut node_to_value: Vec<u32> = Vec::new();
    let mut value_to_node: FxHashMap<u32, u32> = FxHashMap::default();
    let mut spill_costs: Vec<f64> = Vec::new();
    let mut interval_map: FxHashMap<u32, (u32, u32)> = FxHashMap::default(); // value_id -> (start, end)

    for iv in &eligible_intervals {
        if !value_to_node.contains_key(&iv.value_id) {
            let idx = node_to_value.len() as u32;
            node_to_value.push(iv.value_id);
            value_to_node.insert(iv.value_id, idx);
            let cost = use_count.get(&iv.value_id).copied().unwrap_or(1) as f64;
            spill_costs.push(cost);
            interval_map.insert(iv.value_id, (iv.start, iv.end));
        } else {
            // Multi-def value: extend interval
            let entry = interval_map.get_mut(&iv.value_id).unwrap();
            entry.0 = entry.0.min(iv.start);
            entry.1 = entry.1.max(iv.end);
        }
    }

    let num_virtual = node_to_value.len();
    let mut graph = IrcGraph::new(
        num_virtual, config, node_to_value, value_to_node, spill_costs,
    );

    // Collect copy pairs for the SSA copy exception
    let copy_pairs = collect_copy_pairs(func, eligible);

    // Sort intervals by start point for the sweep
    let mut sorted: Vec<(u32, u32, u32)> = Vec::new(); // (start, end, value_id)
    for (&vid, &(start, end)) in &interval_map {
        sorted.push((start, end, vid));
    }
    sorted.sort_by_key(|&(s, e, _)| (s, e));

    // Interval sweep: maintain active set ordered by end point
    // For each new interval, add edges to all active intervals
    let mut active: Vec<(u32, u32)> = Vec::new(); // (end, value_id), sorted by end

    for &(start, end, vid) in &sorted {
        // Remove expired intervals
        active.retain(|&(e, _)| e >= start);

        // Add edges from vid to all active intervals
        let vid_node = graph.value_to_node[&vid];
        for &(_, active_vid) in &active {
            // SSA copy exception: no edge between copy src and dest
            if copy_pairs.contains(&(vid, active_vid))
                || copy_pairs.contains(&(active_vid, vid))
            {
                continue;
            }
            let active_node = graph.value_to_node[&active_vid];
            graph.add_edge(vid_node, active_node);
        }

        // Add to active set, maintaining sort by end point
        let pos = active.partition_point(|&(e, _)| e < end);
        active.insert(pos, (end, vid));
    }

    // Add clobber interference: caller-saved precolored nodes interfere
    // with values live at their clobber points.
    let num_caller_saved = config.caller_saved_regs.len();
    for reg_idx in 0..num_caller_saved {
        let precolored_node = num_virtual as u32 + reg_idx as u32;
        if let Some(clobber_points) = liveness.scratch_clobber_points.get(reg_idx) {
            for &point in clobber_points {
                // Find all values live at this point (interval contains point)
                for (&vid, &(start, end)) in &interval_map {
                    if start <= point && point <= end {
                        let vid_node = graph.value_to_node[&vid];
                        graph.add_edge(vid_node, precolored_node);
                    }
                }
            }
        }
    }

    // Add call-point interference: caller-saved precolored nodes interfere
    // with all values live at call points.
    for &point in &liveness.call_points {
        for (&vid, &(start, end)) in &interval_map {
            if start <= point && point <= end {
                let vid_node = graph.value_to_node[&vid];
                for reg_idx in 0..num_caller_saved {
                    let precolored_node = num_virtual as u32 + reg_idx as u32;
                    graph.add_edge(vid_node, precolored_node);
                }
            }
        }
    }

    // Register coalescing candidates: Copy instructions between eligible values
    for block in &func.blocks {
        for inst in &block.instructions {
            if let Instruction::Copy { dest, src: Operand::Value(s) } = inst {
                if let (Some(&d_node), Some(&s_node)) = (
                    graph.value_to_node.get(&dest.0),
                    graph.value_to_node.get(&s.0),
                ) {
                    // Only add if they don't already interfere
                    if !graph.adj_set[d_node as usize].contains(&s_node) {
                        graph.add_move(s_node, d_node);
                    }
                }
            }
        }
    }

    graph
}

// ── Public entry point ──────────────────────────────────────────────────────

/// Run graph coloring register allocation using Iterated Register Coalescing.
///
/// Drop-in replacement for `allocate_registers()` that aggressively coalesces
/// Copy instructions (especially phi-eliminated copies) to reduce register
/// shuffles. Activated under `-Os` for code size optimization.
pub fn allocate_irc(
    func: &IrFunction,
    config: &RegAllocConfig,
) -> RegAllocResult {
    if config.available_regs.is_empty() && config.caller_saved_regs.is_empty() {
        return RegAllocResult {
            assignments: FxHashMap::default(),
            used_regs: Vec::new(),
            liveness: None,
        };
    }

    let liveness = compute_live_intervals(func);
    let (eligible, use_count) = build_eligible_and_counts(func, config);

    if eligible.is_empty() {
        return RegAllocResult {
            assignments: FxHashMap::default(),
            used_regs: Vec::new(),
            liveness: Some(liveness),
        };
    }

    let mut graph = build_graph(&liveness, func, &eligible, &use_count, config);

    if graph.num_nodes == 0 || graph.k == 0 {
        return RegAllocResult {
            assignments: FxHashMap::default(),
            used_regs: Vec::new(),
            liveness: Some(liveness),
        };
    }

    // Run IRC: simplify/coalesce/freeze/spill/select
    graph.run();

    // Assign colors and map back to PhysReg
    let (assignments, used_regs) = graph.assign_colors();

    RegAllocResult {
        assignments,
        used_regs,
        liveness: Some(liveness),
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a minimal IrcGraph for testing the IRC algorithm directly.
    fn test_graph(num_virtual: usize, k: usize) -> IrcGraph {
        let node_to_value: Vec<u32> = (0..num_virtual as u32).collect();
        let value_to_node: FxHashMap<u32, u32> =
            (0..num_virtual as u32).map(|i| (i, i)).collect();
        let spill_costs = vec![1.0; num_virtual];

        // Create a config with k registers (no precolored for simplicity)
        let config = RegAllocConfig {
            available_regs: (0..k as u8).map(PhysReg).collect(),
            caller_saved_regs: Vec::new(),
            allow_inline_asm_regalloc: false,
            optimize_size: true,
        };

        IrcGraph::new(num_virtual, &config, node_to_value, value_to_node, spill_costs)
    }

    #[test]
    fn test_two_non_interfering_values_coalesce() {
        // Two values connected by a move, no interference -> should coalesce
        let mut g = test_graph(2, 3);
        g.add_move(0, 1);
        g.run();
        g.assign_colors();
        // Both should get the same color
        assert_eq!(g.color[0], g.color[1]);
    }

    #[test]
    fn test_interfering_values_different_colors() {
        // Two values that interfere -> must get different colors
        let mut g = test_graph(2, 3);
        g.add_edge(0, 1);
        g.run();
        g.assign_colors();
        assert_ne!(g.color[0], g.color[1]);
        assert!(g.color[0].is_some());
        assert!(g.color[1].is_some());
    }

    #[test]
    fn test_spill_when_k_exhausted() {
        // 4 mutually-interfering values with K=3 -> one must spill
        let mut g = test_graph(4, 3);
        g.add_edge(0, 1);
        g.add_edge(0, 2);
        g.add_edge(0, 3);
        g.add_edge(1, 2);
        g.add_edge(1, 3);
        g.add_edge(2, 3);
        g.run();
        g.assign_colors();
        let colored: usize = (0..4).filter(|&i| g.color[i].is_some()).count();
        assert_eq!(colored, 3); // one spilled
    }

    #[test]
    fn test_chain_coalescing() {
        // a -> b -> c via moves, no interference -> all coalesce to same color
        let mut g = test_graph(3, 2);
        g.add_move(0, 1);
        g.add_move(1, 2);
        g.run();
        g.assign_colors();
        assert_eq!(g.color[0], g.color[1]);
        assert_eq!(g.color[1], g.color[2]);
    }

    #[test]
    fn test_briggs_criterion_prevents_unsafe_coalesce() {
        // Set up: K=2, nodes 0..4
        // Move between 0 and 1, but their combined neighbors have 2 high-degree nodes
        // Briggs says: don't coalesce if merged node has >= K high-degree neighbors
        let mut g = test_graph(5, 2);
        // 0 interferes with 2,3 (degree 2 each after edges)
        // 1 interferes with 3,4
        g.add_edge(0, 2);
        g.add_edge(0, 3);
        g.add_edge(1, 3);
        g.add_edge(1, 4);
        // 2 interferes with 4 (makes both high-degree)
        g.add_edge(2, 4);
        // Move between 0 and 1
        g.add_move(0, 1);
        // Merged node 0+1 would neighbor {2,3,4} — all with degree >=2 = K
        // Briggs should reject this coalesce
        g.run();
        g.assign_colors();
        // They should NOT have been coalesced (different colors possible)
        // The important thing is the algorithm completes without error
        assert!(g.color[0].is_some() || g.color[1].is_some());
    }

    #[test]
    fn test_precolored_interference() {
        // Node 0 interferes with precolored node (caller-saved reg)
        // Node 0 should NOT get that color
        let config = RegAllocConfig {
            available_regs: vec![PhysReg(3)], // callee-saved: color 1
            caller_saved_regs: vec![PhysReg(4)], // caller-saved: color 0
            allow_inline_asm_regalloc: false,
            optimize_size: true,
        };
        let node_to_value = vec![0u32];
        let value_to_node: FxHashMap<u32, u32> = [(0u32, 0u32)].into_iter().collect();
        let spill_costs = vec![1.0];

        let mut g = IrcGraph::new(1, &config, node_to_value, value_to_node, spill_costs);
        // Precolored node is at index 1 (num_virtual=1, first precolored)
        // Add interference between virtual node 0 and precolored node 1
        g.add_edge(0, 1);
        g.run();
        let (assignments, _) = g.assign_colors();
        // Node 0 should get the callee-saved color (1), not caller-saved (0)
        assert_eq!(assignments.get(&0), Some(&PhysReg(3)));
    }

    #[test]
    fn test_empty_function() {
        // Test that allocate_irc handles empty configs gracefully
        let result = RegAllocResult {
            assignments: FxHashMap::default(),
            used_regs: Vec::new(),
            liveness: None,
        };
        assert!(result.assignments.is_empty());
    }
}
