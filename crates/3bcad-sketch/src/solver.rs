//! Sparse Levenberg-Marquardt least squares with exact Jacobians.
//!
//! The problem is split into independent components (sets of parameters linked by terms), so a
//! sketch made of many separate shapes is many small solves. Small components use a dense
//! Cholesky step; big ones use Jacobi-preconditioned conjugate gradients on the normal equations,
//! which only needs sparse Jacobian products.

use nalgebra::{DMatrix, DVector, SymmetricEigen};

use crate::scalar::{Dual, MAX_LOCAL};

/// Components with more parameters than this use the iterative step.
const DENSE_MAX: usize = 300;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolveReport {
    pub converged: bool,
    /// L2 norm of the remaining residuals.
    pub residual: f64,
    pub iterations: usize,
}

/// A least-squares problem made of small terms, each reading a few parameters.
pub trait System {
    fn terms(&self) -> usize;
    /// Global parameter indices term `t` reads (at most [`MAX_LOCAL`]).
    fn term_params(&self, t: usize) -> &[usize];
    /// Residuals of term `t` from its local parameter values.
    fn eval(&self, t: usize, local: &[f64], out: &mut Vec<f64>);
    /// Same, as dual numbers seeded per local parameter.
    fn eval_dual(&self, t: usize, local: &[Dual], out: &mut Vec<Dual>);
}

/// Parameters linked by terms, solved independently of the rest.
pub struct Component {
    pub terms: Vec<usize>,
    pub params: Vec<usize>,
}

pub fn components(sys: &dyn System, n: usize) -> Vec<Component> {
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for t in 0..sys.terms() {
        let ps = sys.term_params(t);
        for w in ps.windows(2) {
            let (a, b) = (find(&mut parent, w[0]), find(&mut parent, w[1]));
            parent[a] = b;
        }
    }
    let mut index = vec![usize::MAX; n];
    let mut out: Vec<Component> = Vec::new();
    for t in 0..sys.terms() {
        let Some(&p0) = sys.term_params(t).first() else {
            continue;
        };
        let root = find(&mut parent, p0);
        if index[root] == usize::MAX {
            index[root] = out.len();
            out.push(Component {
                terms: Vec::new(),
                params: Vec::new(),
            });
        }
        out[index[root]].terms.push(t);
    }
    for p in 0..n {
        let root = find(&mut parent, p);
        if index[root] != usize::MAX {
            out[index[root]].params.push(p);
        }
    }
    out
}

/// Sparse rows of the Jacobian of one component, columns local to `comp.params`.
pub(crate) struct Jacobian {
    pub rows: Vec<Vec<(usize, f64)>>,
    pub residuals: Vec<f64>,
    /// The term each row came from.
    pub row_terms: Vec<usize>,
    pub cols: usize,
}

pub(crate) fn residuals(sys: &dyn System, comp: &Component, x: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut local = Vec::with_capacity(MAX_LOCAL);
    for &t in &comp.terms {
        local.clear();
        local.extend(sys.term_params(t).iter().map(|&p| x[p]));
        sys.eval(t, &local, &mut out);
    }
    out
}

pub(crate) fn jacobian(sys: &dyn System, comp: &Component, x: &[f64], col_of: &[usize]) -> Jacobian {
    let mut rows = Vec::new();
    let mut residuals = Vec::new();
    let mut row_terms = Vec::new();
    let mut out = Vec::new();
    for &t in &comp.terms {
        let ps = sys.term_params(t);
        let local: Vec<Dual> = ps.iter().enumerate().map(|(i, &p)| Dual::var(x[p], i)).collect();
        out.clear();
        sys.eval_dual(t, &local, &mut out);
        for r in &out {
            residuals.push(r.v);
            row_terms.push(t);
            let mut row: Vec<(usize, f64)> = Vec::with_capacity(ps.len());
            for (i, &p) in ps.iter().enumerate() {
                if r.d[i] != 0.0 {
                    // A term may read the same parameter twice; merge those partials.
                    match row.iter_mut().find(|(c, _)| *c == col_of[p]) {
                        Some((_, v)) => *v += r.d[i],
                        None => row.push((col_of[p], r.d[i])),
                    }
                }
            }
            rows.push(row);
        }
    }
    Jacobian {
        rows,
        residuals,
        row_terms,
        cols: comp.params.len(),
    }
}

fn norm(r: &[f64]) -> f64 {
    r.iter().map(|v| v * v).sum::<f64>().sqrt()
}

/// Solves every component; `x` holds all parameters and is updated in place.
pub fn solve(sys: &dyn System, x: &mut [f64], max_iter: usize, tol: f64) -> SolveReport {
    let mut report = SolveReport {
        converged: true,
        residual: 0.0,
        iterations: 0,
    };
    let mut sq = 0.0;
    let mut col_of = vec![usize::MAX; x.len()];
    for comp in components(sys, x.len()) {
        for (i, &p) in comp.params.iter().enumerate() {
            col_of[p] = i;
        }
        let r = solve_component(sys, &comp, x, &col_of, max_iter, tol);
        report.converged &= r.converged;
        report.iterations = report.iterations.max(r.iterations);
        sq += r.residual * r.residual;
    }
    report.residual = sq.sqrt();
    report
}

fn solve_component(
    sys: &dyn System,
    comp: &Component,
    x: &mut [f64],
    col_of: &[usize],
    max_iter: usize,
    tol: f64,
) -> SolveReport {
    let mut cost = norm(&residuals(sys, comp, x));
    let mut lambda = 1e-3;
    for iter in 0..max_iter {
        if cost < tol {
            return SolveReport {
                converged: true,
                residual: cost,
                iterations: iter,
            };
        }
        let jac = jacobian(sys, comp, x, col_of);
        let n = jac.cols;
        let mut grad = vec![0.0; n];
        let mut diag = vec![0.0; n];
        for (row, r) in jac.rows.iter().zip(&jac.residuals) {
            for &(c, v) in row {
                grad[c] += v * r;
                diag[c] += v * v;
            }
        }
        let jtj = (n <= DENSE_MAX).then(|| {
            let mut m = DMatrix::<f64>::zeros(n, n);
            for row in &jac.rows {
                for &(c1, v1) in row {
                    for &(c2, v2) in row {
                        m[(c1, c2)] += v1 * v2;
                    }
                }
            }
            m
        });
        loop {
            let damp: Vec<f64> = diag.iter().map(|d| lambda * (1.0 + d)).collect();
            let step = match &jtj {
                Some(m) => {
                    let mut a = m.clone();
                    for (i, d) in damp.iter().enumerate() {
                        a[(i, i)] += d;
                    }
                    a.cholesky()
                        .map(|c| c.solve(&-DVector::from_column_slice(&grad)).as_slice().to_vec())
                }
                None => Some(conjugate_gradient(&jac, &diag, &damp, &grad)),
            };
            if let Some(step) = step {
                let mut trial = x.to_vec();
                for (i, &p) in comp.params.iter().enumerate() {
                    trial[p] += step[i];
                }
                let trial_cost = norm(&residuals(sys, comp, &trial));
                if trial_cost < cost {
                    for &p in &comp.params {
                        x[p] = trial[p];
                    }
                    cost = trial_cost;
                    lambda = (lambda * 0.1).max(1e-12);
                    break;
                }
            }
            lambda *= 10.0;
            if lambda > 1e12 {
                return SolveReport {
                    converged: cost < tol,
                    residual: cost,
                    iterations: iter + 1,
                };
            }
        }
    }
    SolveReport {
        converged: cost < tol,
        residual: cost,
        iterations: max_iter,
    }
}

/// Solves `(JᵀJ + diag(damp)) step = -grad` with Jacobi-preconditioned CG, using only J products.
fn conjugate_gradient(jac: &Jacobian, diag: &[f64], damp: &[f64], grad: &[f64]) -> Vec<f64> {
    let n = jac.cols;
    let apply = |v: &[f64]| -> Vec<f64> {
        let mut out: Vec<f64> = v.iter().zip(damp).map(|(v, d)| v * d).collect();
        for row in &jac.rows {
            let jv: f64 = row.iter().map(|&(c, w)| w * v[c]).sum();
            for &(c, w) in row {
                out[c] += w * jv;
            }
        }
        out
    };
    let precond: Vec<f64> = diag.iter().zip(damp).map(|(d, l)| 1.0 / (d + l).max(1e-300)).collect();
    let mut step = vec![0.0; n];
    let mut r: Vec<f64> = grad.iter().map(|g| -g).collect();
    let mut z: Vec<f64> = r.iter().zip(&precond).map(|(r, m)| r * m).collect();
    let mut p = z.clone();
    let mut rz: f64 = r.iter().zip(&z).map(|(a, b)| a * b).sum();
    let target = norm(&r) * 1e-12;
    for _ in 0..(2 * n).max(50) {
        if norm(&r) <= target {
            break;
        }
        let ap = apply(&p);
        let alpha = rz / p.iter().zip(&ap).map(|(a, b)| a * b).sum::<f64>();
        for i in 0..n {
            step[i] += alpha * p[i];
            r[i] -= alpha * ap[i];
            z[i] = r[i] * precond[i];
        }
        let rz_new: f64 = r.iter().zip(&z).map(|(a, b)| a * b).sum();
        let beta = rz_new / rz;
        rz = rz_new;
        for i in 0..n {
            p[i] = z[i] + beta * p[i];
        }
    }
    step
}

/// Degrees of freedom of one component at the current solution.
pub(crate) struct Rank {
    /// Parameters (local columns) that can still move without violating any term.
    pub free: Vec<bool>,
    pub rank: usize,
    pub rows: usize,
}

/// Rank and null space from the eigen-decomposition of `JᵀJ` (full basis, unlike a thin SVD).
pub(crate) fn rank(jac: &Jacobian) -> Rank {
    let n = jac.cols;
    let mut m = DMatrix::<f64>::zeros(n, n);
    for row in &jac.rows {
        for &(c1, v1) in row {
            for &(c2, v2) in row {
                m[(c1, c2)] += v1 * v2;
            }
        }
    }
    let eig = SymmetricEigen::new(m);
    let max = eig.eigenvalues.iter().fold(0.0_f64, |a, &b| a.max(b.abs()));
    let eps = 1e-10 * max.max(1.0);
    let mut free = vec![false; n];
    let mut rank = 0;
    for (k, &ev) in eig.eigenvalues.iter().enumerate() {
        if ev.abs() > eps {
            rank += 1;
        } else {
            for (i, f) in free.iter_mut().enumerate() {
                *f |= eig.eigenvectors[(i, k)].abs() > 1e-6;
            }
        }
    }
    Rank {
        free,
        rank,
        rows: jac.rows.len(),
    }
}
