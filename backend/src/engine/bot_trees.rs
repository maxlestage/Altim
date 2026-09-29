//! Small gradient-boosted trees for the « Bot Altim » v2 (candidate « arbres »): logistic loss, histogram splits on
//! quantile bins computed from the training rows only, depth ≤ 3, fixed shrinkage and tree count, a minimum leaf
//! size and an L2 penalty on the leaf values. Deterministic: no sampling, ties broken by the first feature and bin
//! in order. Pure, no dependency.
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

    /// Fits on `xs` / `ys` (same length, at least one row). Bins and every statistic come from these rows only.
    pub fn fit(xs: &[&[f32]], ys: &[bool], cols: &[usize]) -> Forest {
        let n = xs.len();
        let m = cols.len();
        let base = ys.iter().filter(|y| **y).count() as f64 / n.max(1) as f64;
        let b = base.clamp(1e-6, 1.0 - 1e-6);
        let bias = (b / (1.0 - b)).ln();
        // Bin thresholds per column, and the rows' bins (row-major).
        let col_edges: Vec<Vec<f32>> = cols
            .iter()
            .map(|&j| {
                let mut v: Vec<f32> = xs.iter().map(|x| x[j]).collect();
                edges(&mut v)
            })
            .collect();
        let mut binned = vec![0u8; n * m];
        for (i, x) in xs.iter().enumerate() {
            for (k, &j) in cols.iter().enumerate() {
                binned[i * m + k] = bin_of(&col_edges[k], x[j]);
            }
        }
        let y: Vec<f64> = ys.iter().map(|v| if *v { 1.0 } else { 0.0 }).collect();
        let mut f = vec![bias; n];
        let mut g = vec![0.0; n];
        let mut h = vec![0.0; n];
        let mut trees = Vec::with_capacity(TREES);
        let mut hist = vec![(0.0f64, 0.0f64, 0u32); m * BINS];
        for _ in 0..TREES {
            for i in 0..n {
                let p = sigmoid(f[i]);
                g[i] = p - y[i];
                h[i] = (p * (1.0 - p)).max(1e-12);
            }
            let mut tree: Tree = Vec::new();
            // (node index, rows, depth)
            let all: Vec<u32> = (0..n as u32).collect();
            let (gs, hs) = all.iter().fold((0.0, 0.0), |a, &i| (a.0 + g[i as usize], a.1 + h[i as usize]));
            tree.push(Node { feature: 0, threshold: 0.0, left: 0, right: 0, value: -SHRINKAGE * gs / (hs + LAMBDA) });
            let mut stack = vec![(0usize, all, 0usize, gs, hs)];
            let mut leaves: Vec<(usize, Vec<u32>)> = Vec::new();
            while let Some((node, idx, depth, gs, hs)) = stack.pop() {
                if depth >= DEPTH || idx.len() < 2 * MIN_LEAF {
                    leaves.push((node, idx));
                    continue;
                }
                hist.iter_mut().for_each(|e| *e = (0.0, 0.0, 0));
                for &i in &idx {
                    let i = i as usize;
                    let row = &binned[i * m..(i + 1) * m];
                    for (k, &bn) in row.iter().enumerate() {
                        let e = &mut hist[k * BINS + bn as usize];
                        e.0 += g[i];
                        e.1 += h[i];
                        e.2 += 1;
                    }
                }
                let parent = gs * gs / (hs + LAMBDA);
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
                        if cl < MIN_LEAF || cr < MIN_LEAF {
                            continue;
                        }
                        let (gr, hr) = (gs - gl, hs - hl);
                        let gain = gl * gl / (hl + LAMBDA) + gr * gr / (hr + LAMBDA) - parent;
                        if gain > 1e-12 && best.as_ref().is_none_or(|b| gain > b.gain) {
                            best = Some(Split { col: k, bin: bn, gain });
                        }
                    }
                }
                let Some(s) = best else {
                    leaves.push((node, idx));
                    continue;
                };
                let (left, right): (Vec<u32>, Vec<u32>) = idx.into_iter().partition(|&i| binned[i as usize * m + s.col] as usize <= s.bin);
                let sum = |v: &[u32]| v.iter().fold((0.0, 0.0), |a, &i| (a.0 + g[i as usize], a.1 + h[i as usize]));
                let ((gl, hl), (gr, hr)) = (sum(&left), sum(&right));
                let (li, ri) = (tree.len(), tree.len() + 1);
                tree.push(Node { feature: 0, threshold: 0.0, left: 0, right: 0, value: -SHRINKAGE * gl / (hl + LAMBDA) });
                tree.push(Node { feature: 0, threshold: 0.0, left: 0, right: 0, value: -SHRINKAGE * gr / (hr + LAMBDA) });
                let t = &mut tree[node];
                t.feature = cols[s.col] as u16;
                t.threshold = col_edges[s.col][s.bin] as f64;
                t.left = li as u16;
                t.right = ri as u16;
                // Right first on the stack so that the left subtree is numbered first (stable layout).
                stack.push((ri, right, depth + 1, gr, hr));
                stack.push((li, left, depth + 1, gl, hl));
            }
            for (node, idx) in leaves {
                let v = tree[node].value;
                for i in idx {
                    f[i as usize] += v;
                }
            }
            trees.push(tree);
        }
        Forest { cols: cols.to_vec(), bias, trees, base_rate: base, rows: n }
    }
}
