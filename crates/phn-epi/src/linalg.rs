//! Small dense linear algebra for model fitting (p ≤ a few dozen).

/// A row-major square or rectangular matrix.
pub type Matrix = Vec<Vec<f64>>;

/// Cholesky factor L (lower triangular) of a symmetric positive-definite
/// matrix, or `None` when the matrix is not positive definite (for example
/// when predictors are collinear).
pub fn cholesky(a: &Matrix) -> Option<Matrix> {
    let n = a.len();
    let mut l = vec![vec![0.0; n]; n];
    let scale = (0..n)
        .map(|i| a[i][i].abs())
        .fold(0.0f64, f64::max)
        .max(1e-300);
    for i in 0..n {
        for j in 0..=i {
            let mut s = a[i][j];
            for k in 0..j {
                s -= l[i][k] * l[j][k];
            }
            if i == j {
                if s <= 1e-12 * scale {
                    return None;
                }
                l[i][j] = s.sqrt();
            } else {
                l[i][j] = s / l[j][j];
            }
        }
    }
    Some(l)
}

/// Solve A x = b given the Cholesky factor of A.
pub fn cholesky_solve(l: &Matrix, b: &[f64]) -> Vec<f64> {
    let n = l.len();
    let mut y = vec![0.0; n];
    for i in 0..n {
        let mut s = b[i];
        for k in 0..i {
            s -= l[i][k] * y[k];
        }
        y[i] = s / l[i][i];
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = y[i];
        for k in i + 1..n {
            s -= l[k][i] * x[k];
        }
        x[i] = s / l[i][i];
    }
    x
}

/// Inverse of A from its Cholesky factor.
pub fn cholesky_inverse(l: &Matrix) -> Matrix {
    let n = l.len();
    let mut inv = vec![vec![0.0; n]; n];
    for j in 0..n {
        let mut e = vec![0.0; n];
        e[j] = 1.0;
        let col = cholesky_solve(l, &e);
        for i in 0..n {
            inv[i][j] = col[i];
        }
    }
    inv
}

/// Xᵀ W X and Xᵀ W z for rows `x` (n × p), weights `w`, and response `z`.
pub fn weighted_normal_equations(x: &[Vec<f64>], w: &[f64], z: &[f64]) -> (Matrix, Vec<f64>) {
    let p = x.first().map_or(0, Vec::len);
    let mut xtwx = vec![vec![0.0; p]; p];
    let mut xtwz = vec![0.0; p];
    for (i, row) in x.iter().enumerate() {
        let wi = w[i];
        if wi == 0.0 {
            continue;
        }
        for a in 0..p {
            let wa = wi * row[a];
            xtwz[a] += wa * z[i];
            for b in 0..=a {
                xtwx[a][b] += wa * row[b];
            }
        }
    }
    for a in 0..p {
        for b in 0..a {
            xtwx[b][a] = xtwx[a][b];
        }
    }
    (xtwx, xtwz)
}

/// cᵀ M c for a vector `c` and symmetric matrix `m`.
pub fn quad_form(c: &[f64], m: &Matrix) -> f64 {
    let mut s = 0.0;
    for i in 0..c.len() {
        for j in 0..c.len() {
            s += c[i] * m[i][j] * c[j];
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solves_and_inverts() {
        let a = vec![
            vec![4.0, 2.0, 0.6],
            vec![2.0, 2.0, 0.5],
            vec![0.6, 0.5, 3.0],
        ];
        let l = cholesky(&a).unwrap();
        let x = cholesky_solve(&l, &[1.0, 2.0, 3.0]);
        for i in 0..3 {
            let ax: f64 = (0..3).map(|j| a[i][j] * x[j]).sum();
            assert!((ax - [1.0, 2.0, 3.0][i]).abs() < 1e-12);
        }
        let inv = cholesky_inverse(&l);
        for i in 0..3 {
            for j in 0..3 {
                let v: f64 = (0..3).map(|k| a[i][k] * inv[k][j]).sum();
                assert!((v - if i == j { 1.0 } else { 0.0 }).abs() < 1e-12);
            }
        }
        assert!(cholesky(&vec![vec![1.0, 1.0], vec![1.0, 1.0]]).is_none());
        assert!(
            (quad_form(&[1.0, 1.0], &vec![vec![1.0, 0.5], vec![0.5, 2.0]]) - 4.0).abs() < 1e-12
        );
    }
}
