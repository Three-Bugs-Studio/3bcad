use nalgebra::{DMatrix, DVector};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolveReport {
    pub converged: bool,
    /// L2 norm of the remaining residuals.
    pub residual: f64,
    pub iterations: usize,
}

/// Levenberg-Marquardt least squares: moves `x` to minimise `|f(x)|`.
///
/// Under-defined sketches are fine: the damping term keeps the normal
/// equations positive definite and picks the step that moves geometry least.
/// Inconsistent (over-defined) sketches stop with `converged: false`.
pub fn levenberg_marquardt(x: &mut [f64], f: impl Fn(&[f64]) -> Vec<f64>, max_iter: usize, tol: f64) -> SolveReport {
    let norm = |r: &[f64]| r.iter().map(|v| v * v).sum::<f64>().sqrt();
    let mut r = f(x);
    let mut cost = norm(&r);
    let mut lambda = 1e-3;
    let n = x.len();

    for iter in 0..max_iter {
        if cost < tol {
            return SolveReport {
                converged: true,
                residual: cost,
                iterations: iter,
            };
        }
        // Central-difference Jacobian. shortcut: numeric, O(n) residual evals; switch to analytic derivatives for sketches with 1000s of params.
        let mut jac = DMatrix::zeros(r.len(), n);
        for j in 0..n {
            let x0 = x[j];
            let h = 1e-7 * x0.abs().max(1.0);
            x[j] = x0 + h;
            let rp = f(x);
            x[j] = x0 - h;
            let rm = f(x);
            x[j] = x0;
            for i in 0..r.len() {
                jac[(i, j)] = (rp[i] - rm[i]) / (2.0 * h);
            }
        }
        let jt = jac.transpose();
        let jtj = &jt * &jac;
        let neg_grad = -(&jt * DVector::from_column_slice(&r));

        loop {
            let mut a = jtj.clone();
            for i in 0..n {
                a[(i, i)] += lambda * (1.0 + jtj[(i, i)]);
            }
            if let Some(step) = a.cholesky().map(|c| c.solve(&neg_grad)) {
                let trial: Vec<f64> = x.iter().zip(step.iter()).map(|(a, b)| a + b).collect();
                let rt = f(&trial);
                let trial_cost = norm(&rt);
                if trial_cost < cost {
                    x.copy_from_slice(&trial);
                    (r, cost) = (rt, trial_cost);
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
