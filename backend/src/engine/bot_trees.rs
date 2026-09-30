//! Small gradient-boosted trees for the « Bot Altim » (v2's candidate « arbres », v3's longer ones): logistic loss,
//! histogram splits on quantile bins computed from the training rows only, a depth limit, a shrinkage, a minimum leaf
//! size and an L2 penalty on the leaf values; v2: a fixed tree count (`V2`), v3: up to 1 000 rounds stopped early on
//! a validation set (`LONG`, `PATIENCE`). Deterministic: no sampling, ties broken by the first feature and bin in
//! order. Pure, no dependency.
use serde::{Deserialize, Serialize};

pub const TREES: usize = 100;
pub const DEPTH: usize = 3;
pub const SHRINKAGE: f64 = 0.1;
/// Quantile bins per feature (at most; fewer when a feature takes fewer values).
pub const BINS: usize = 32;
/// Rows on each side of a split, at least.
pub const MIN_LEAF: usize = 200;
/// L2 penalty on a leaf's Newton step: − G ÷ (H + λ).
pub const LAMBDA: f64 = 1.0;

/// Hyper-parameters of a fit (`trees`: the rounds, at most with early stopping).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    pub trees: usize,
    pub depth: usize,
    pub shrinkage: f64,
    pub min_leaf: usize,
    pub lambda: f64,
    /// A child's histogram as its parent's minus its sibling's (the smaller one is summed): same splits up to the
    /// rounding, about twice faster. Off for v2 (its sums row by row, unchanged).
    pub subtract: bool,
}

/// v2's candidate « arbres ».
pub const V2: Params = Params { trees: TREES, depth: DEPTH, shrinkage: SHRINKAGE, min_leaf: MIN_LEAF, lambda: LAMBDA, subtract: true };
/// v3's longer trees (« arbres longs »): rounds chosen by early stopping.
pub const LONG: Params = Params { trees: 1000, depth: 4, shrinkage: 0.05, min_leaf: 200, lambda: 1.0, subtract: true };
/// Early stopping: stop this many rounds after the best validation log-loss, keep the trees up to the best.
pub const PATIENCE: usize = 50;

/// What early stopping found: the rounds kept, the best validation log-loss, the rounds computed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stop {
    pub rounds: usize,
    pub loss: f64,
    pub computed: usize,
}

/// A node: `[feature, threshold, left, right, value]` in JSON (compact). Leaves have `left == right == 0`; `value` is
/// the node's shrunk Newton step (log-odds), kept on inner nodes too for the per-feature contributions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(from = "(u16, f64, u16, u16, f64)", into = "(u16, f64, u16, u16, f64)")]
pub struct Node {
    pub feature: u16,
    pub threshold: f64,
    pub left: u16,
    pub right: u16,
    pub value: f64,
}

impl From<(u16, f64, u16, u16, f64)> for Node {
    fn from(t: (u16, f64, u16, u16, f64)) -> Self {
        Node { feature: t.0, threshold: t.1, left: t.2, right: t.3, value: t.4 }
    }
}
impl From<Node> for (u16, f64, u16, u16, f64) {
    fn from(n: Node) -> Self {
        (n.feature, n.threshold, n.left, n.right, n.value)
    }
}

impl Node {
    pub fn is_leaf(&self) -> bool {
        self.left == 0 && self.right == 0
    }
}

/// One tree: node 0 is the root.
pub type Tree = Vec<Node>;

/// The boosted model on the feature columns `cols` of a row (`x[cols[k]]`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Forest {
    pub cols: Vec<usize>,
    /// Starting log-odds: the training base rate's.
    pub bias: f64,
    pub trees: Vec<Tree>,
    pub base_rate: f64,
    pub rows: usize,
}

fn sigmoid(z: f64) -> f64 {
    if z >= 0.0 { 1.0 / (1.0 + (-z).exp()) } else { z.exp() / (1.0 + z.exp()) }
}

/// Quantile thresholds of one column (sorted, distinct): at most `BINS − 1`.
pub fn edges(values: &mut [f32]) -> Vec<f32> {
    values.sort_by(|a, b| a.total_cmp(b));
    let n = values.len();
    let mut out: Vec<f32> = Vec::new();
    if n == 0 {
        return out;
    }
    for k in 1..BINS {
        let v = values[(k * n / BINS).min(n - 1)];
        // A threshold below the maximum only (a split must leave rows on the right).
        if v < values[n - 1] && out.last().is_none_or(|l| v > *l) {
            out.push(v);
        }
    }
    out
}

/// Bin of a value: the index of the first threshold ≥ it (so `bin ≤ b` ⟺ `value ≤ edges[b]`).
fn bin_of(e: &[f32], v: f32) -> u8 {
    e.partition_point(|t| *t < v) as u8
}

struct Split {
    col: usize,
    bin: usize,
    gain: f64,
}

impl Forest {
    fn leaf_of<'a>(tree: &'a [Node], x: &[f32]) -> &'a Node {
        let mut n = &tree[0];
        while !n.is_leaf() {
            n = if x[n.feature as usize] <= n.threshold as f32 { &tree[n.left as usize] } else { &tree[n.right as usize] };
        }
        n
    }

    /// Log-odds of the positive label at `x` (a whole feature row).
    pub fn margin(&self, x: &[f32]) -> f64 {
        self.bias + self.trees.iter().map(|t| Self::leaf_of(t, x).value).sum::<f64>()
    }

    pub fn prob(&self, x: &[f32]) -> f64 {
        sigmoid(self.margin(x))
    }

    /// Per-feature contributions to the log-odds (index = feature of the row), following each tree's path: the change
    /// of the node value at each split is credited to its feature. They add up to the margin minus the bias and the
    /// roots' values.
    pub fn contributions(&self, x: &[f32], n_features: usize) -> Vec<f64> {
        let mut out = vec![0.0; n_features];
        for t in &self.trees {
            let mut n = &t[0];
            while !n.is_leaf() {
                let c = if x[n.feature as usize] <= n.threshold as f32 { &t[n.left as usize] } else { &t[n.right as usize] };
                if let Some(o) = out.get_mut(n.feature as usize) {
                    *o += c.value - n.value;
                }
                n = c;
            }
        }
        out
    }

    /// Fits v2's trees on `xs` / `ys` (same length, at least one row). Bins and every statistic come from these rows
    /// only.
    pub fn fit(xs: &[&[f32]], ys: &[bool], cols: &[usize]) -> Forest {
        Self::fit_params(xs, ys, cols, &V2, None).0
    }

    /// Fits with `p`; with `valid` (rows and labels), stops `PATIENCE` rounds after the best mean log-loss on them
    /// and keeps the trees up to the best round (the validation rows never shape a split or a leaf).
    pub fn fit_params(xs: &[&[f32]], ys: &[bool], cols: &[usize], p: &Params, valid: Option<(&[&[f32]], &[bool])>) -> (Forest, Option<Stop>) {
        let n = xs.len();
        let m = cols.len();
        let base = ys.iter().filter(|y| **y).count() as f64 / n.max(1) as f64;
        let b = base.clamp(1e-6, 1.0 - 1e-6);
        let bias = (b / (1.0 - b)).ln();
        // Bin thresholds per column, and the rows' bins (column-major: one feature's bins together).
        let col_edges: Vec<Vec<f32>> = cols
            .iter()
            .map(|&j| {
                let mut v: Vec<f32> = xs.iter().map(|x| x[j]).collect();
                edges(&mut v)
            })
            .collect();
        let mut binned = vec![0u8; n * m];
        for (k, &j) in cols.iter().enumerate() {
            for (i, x) in xs.iter().enumerate() {
                binned[k * n + i] = bin_of(&col_edges[k], x[j]);
            }
        }
        let y: Vec<f64> = ys.iter().map(|v| if *v { 1.0 } else { 0.0 }).collect();
        let mut f = vec![bias; n];
        // (gradient, hessian) of each row.
        let mut gh = vec![(0.0f64, 0.0f64); n];
        let mut trees = Vec::with_capacity(p.trees);
        // Histogram of a node's rows: one feature at a time (each bin sums its rows in their order, as row by row).
        let build = |idx: &[u32], gh: &[(f64, f64)]| -> Vec<(f64, f64, u32)> {
            let mut hist = vec![(0.0f64, 0.0f64, 0u32); m * BINS];
            for k in 0..m {
                let col = &binned[k * n..(k + 1) * n];
                let hk = &mut hist[k * BINS..(k + 1) * BINS];
                for &i in idx {
                    let i = i as usize;
                    let (gi, hi) = gh[i];
                    let e = &mut hk[col[i] as usize];
                    e.0 += gi;
                    e.1 += hi;
                    e.2 += 1;
                }
            }
            hist
        };
        // Validation margins, updated tree by tree; (best loss, rounds at the best).
        let mut vf: Vec<f64> = valid.map(|v| vec![bias; v.0.len()]).unwrap_or_default();
        let mut best: Option<(f64, usize)> = None;
        for round in 0..p.trees {
            for i in 0..n {
                let p = sigmoid(f[i]);
                gh[i] = (p - y[i], (p * (1.0 - p)).max(1e-12));
            }
            let mut tree: Tree = Vec::new();
            // (node index, rows, depth)
            let all: Vec<u32> = (0..n as u32).collect();
            let (gs, hs) = all.iter().fold((0.0, 0.0), |a, &i| (a.0 + gh[i as usize].0, a.1 + gh[i as usize].1));
            tree.push(Node { feature: 0, threshold: 0.0, left: 0, right: 0, value: -p.shrinkage * gs / (hs + p.lambda) });
            // (node index, rows, depth, sums, histogram when already known)
            let mut stack: Vec<(usize, Vec<u32>, usize, f64, f64, Option<Vec<(f64, f64, u32)>>)> = vec![(0usize, all, 0usize, gs, hs, None)];
            let mut leaves: Vec<(usize, Vec<u32>)> = Vec::new();
            let splits = |len: usize, depth: usize| depth < p.depth && len >= 2 * p.min_leaf;
            while let Some((node, idx, depth, gs, hs, known)) = stack.pop() {
                if !splits(idx.len(), depth) {
                    leaves.push((node, idx));
                    continue;
                }
                let hist = known.unwrap_or_else(|| build(&idx, &gh));
                let parent = gs * gs / (hs + p.lambda);
                let mut best: Option<Split> = None;
                for k in 0..m {
                    let nb = col_edges[k].len();
                    let (mut gl, mut hl, mut cl) = (0.0, 0.0, 0usize);
                    for bn in 0..nb {
                        let e = hist[k * BINS + bn];
                        gl += e.0;
                        hl += e.1;
                        cl += e.2 as usize;
                        let cr = idx.len() - cl;
                        if cl < p.min_leaf || cr < p.min_leaf {
                            continue;
                        }
                        let (gr, hr) = (gs - gl, hs - hl);
                        let gain = gl * gl / (hl + p.lambda) + gr * gr / (hr + p.lambda) - parent;
                        if gain > 1e-12 && best.as_ref().is_none_or(|b| gain > b.gain) {
                            best = Some(Split { col: k, bin: bn, gain });
                        }
                    }
                }
                let Some(s) = best else {
                    leaves.push((node, idx));
                    continue;
                };
                let (left, right): (Vec<u32>, Vec<u32>) = idx.into_iter().partition(|&i| binned[s.col * n + i as usize] as usize <= s.bin);
                let sum = |v: &[u32]| v.iter().fold((0.0, 0.0), |a, &i| (a.0 + gh[i as usize].0, a.1 + gh[i as usize].1));
                let ((gl, hl), (gr, hr)) = (sum(&left), sum(&right));
                let (li, ri) = (tree.len(), tree.len() + 1);
                tree.push(Node { feature: 0, threshold: 0.0, left: 0, right: 0, value: -p.shrinkage * gl / (hl + p.lambda) });
                tree.push(Node { feature: 0, threshold: 0.0, left: 0, right: 0, value: -p.shrinkage * gr / (hr + p.lambda) });
                let t = &mut tree[node];
                t.feature = cols[s.col] as u16;
                t.threshold = col_edges[s.col][s.bin] as f64;
                t.left = li as u16;
                t.right = ri as u16;
                // The children's histograms: the smaller one summed, the other by difference (`subtract` only).
                let (mut lh, mut rh) = (None, None);
                if p.subtract && (splits(left.len(), depth + 1) || splits(right.len(), depth + 1)) {
                    let small_left = left.len() <= right.len();
                    let small = build(if small_left { &left } else { &right }, &gh);
                    let big: Vec<(f64, f64, u32)> = hist.iter().zip(&small).map(|(a, b)| (a.0 - b.0, a.1 - b.1, a.2 - b.2)).collect();
                    (lh, rh) = if small_left { (Some(small), Some(big)) } else { (Some(big), Some(small)) };
                }
                // Right first on the stack so that the left subtree is numbered first (stable layout).
                stack.push((ri, right, depth + 1, gr, hr, rh));
                stack.push((li, left, depth + 1, gl, hl, lh));
            }
            for (node, idx) in leaves {
                let v = tree[node].value;
                for i in idx {
                    f[i as usize] += v;
                }
            }
            if let Some((vx, vy)) = valid {
                let mut loss = 0.0;
                for (k, x) in vx.iter().enumerate() {
                    vf[k] += Self::leaf_of(&tree, x).value;
                    let q = sigmoid(vf[k]).clamp(1e-12, 1.0 - 1e-12);
                    loss -= if vy[k] { q.ln() } else { (1.0 - q).ln() };
                }
                let loss = loss / vx.len().max(1) as f64;
                if best.is_none_or(|(b, _)| loss < b) {
                    best = Some((loss, round + 1));
                }
            }
            trees.push(tree);
            if best.is_some_and(|(_, r)| round + 1 >= r + PATIENCE) {
                break;
            }
        }
        let computed = trees.len();
        let stop = best.map(|(loss, rounds)| {
            trees.truncate(rounds);
            Stop { rounds, loss, computed }
        });
        (Forest { cols: cols.to_vec(), bias, trees, base_rate: base, rows: n }, stop)
    }
}
