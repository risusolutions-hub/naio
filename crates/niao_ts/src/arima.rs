//! AR / ARMA / ARIMA / SARIMA models with Yule–Walker and MLE.

use crate::diagnostics::{diff, seasonal_diff};
use crate::error::{TsError, TsResult};
use crate::util::{aic, aicc, bic, levinson, mean, sigmoid_bounded, var};
use niao_optim::{minimize, MinimizeMethod, MinimizeOptions};
use niao_stats::special::norm_ppf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArimaOrder {
    pub p: usize,
    pub d: usize,
    pub q: usize,
    pub seasonal_p: usize,
    pub seasonal_d: usize,
    pub seasonal_q: usize,
    pub seasonal_period: usize,
}

impl ArimaOrder {
    pub fn arima(p: usize, d: usize, q: usize) -> Self {
        Self {
            p,
            d,
            q,
            seasonal_p: 0,
            seasonal_d: 0,
            seasonal_q: 0,
            seasonal_period: 0,
        }
    }

    pub fn sarima(p: usize, d: usize, q: usize, pp: usize, dd: usize, qq: usize, s: usize) -> Self {
        Self {
            p,
            d,
            q,
            seasonal_p: pp,
            seasonal_d: dd,
            seasonal_q: qq,
            seasonal_period: s,
        }
    }

    pub fn n_params(&self) -> usize {
        let mut k = self.p + self.q;
        if self.p + self.q > 0 {
            k += 1; // constant
        }
        k += self.seasonal_p + self.seasonal_q;
        k
    }
}

#[derive(Debug, Clone)]
pub struct ForecastResult {
    pub mean: Vec<f64>,
    pub lower: Vec<f64>,
    pub upper: Vec<f64>,
    pub se: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct ArimaFit {
    pub order: ArimaOrder,
    pub ar: Vec<f64>,
    pub ma: Vec<f64>,
    pub seasonal_ar: Vec<f64>,
    pub seasonal_ma: Vec<f64>,
    pub constant: f64,
    pub sigma2: f64,
    pub log_likelihood: f64,
    pub aic: f64,
    pub bic: f64,
    pub aicc: f64,
    pub fitted: Vec<f64>,
    pub residuals: Vec<f64>,
    pub endog: Vec<f64>,
    pub original: Vec<f64>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ArimaModel {
    order: ArimaOrder,
    fit: Option<ArimaFit>,
}

impl ArimaModel {
    pub fn new(order: ArimaOrder) -> Self {
        Self { order, fit: None }
    }

    pub fn ar(p: usize) -> Self {
        Self::new(ArimaOrder::arima(p, 0, 0))
    }

    pub fn arima(p: usize, d: usize, q: usize) -> Self {
        Self::new(ArimaOrder::arima(p, d, q))
    }

    pub fn sarima(p: usize, d: usize, q: usize, pp: usize, dd: usize, qq: usize, s: usize) -> Self {
        Self::new(ArimaOrder::sarima(p, d, q, pp, dd, qq, s))
    }

    pub fn is_fitted(&self) -> bool {
        self.fit.is_some()
    }

    pub fn fit(&mut self, endog: &[f64]) -> TsResult<&ArimaFit> {
        let original = endog.to_vec();
        let y = prepare_series(endog, &self.order)?;
        let mut fit = if self.order.q == 0 && self.order.seasonal_q == 0 && self.order.p > 0 {
            fit_ar_yule_walker(&y, &self.order)?
        } else if self.order.p == 0 && self.order.q == 0 && self.order.seasonal_p == 0 && self.order.seasonal_q == 0
        {
            fit_constant_only(&y, &self.order)?
        } else {
            fit_arma_mle(&y, &self.order)?
        };
        fit.original = original;
        self.fit = Some(fit);
        Ok(self.fit.as_ref().unwrap())
    }

    pub fn fit_result(&self) -> TsResult<&ArimaFit> {
        self.fit
            .as_ref()
            .ok_or_else(|| TsError::NotFitted("call fit() first".into()))
    }

    pub fn aic(&self) -> TsResult<f64> {
        Ok(self.fit_result()?.aic)
    }

    pub fn fitted_values(&self) -> TsResult<&[f64]> {
        self.fit
            .as_ref()
            .map(|f| f.fitted.as_slice())
            .ok_or_else(|| TsError::NotFitted("call fit() first".into()))
    }

    pub fn residuals(&self) -> TsResult<&[f64]> {
        self.fit
            .as_ref()
            .map(|f| f.residuals.as_slice())
            .ok_or_else(|| TsError::NotFitted("call fit() first".into()))
    }

    pub fn predict(&self, start: usize, end: usize) -> TsResult<Vec<f64>> {
        let fit = self
            .fit
            .as_ref()
            .ok_or_else(|| TsError::NotFitted("call fit() first".into()))?;
        if end < start {
            return Err(TsError::Domain("predict: end < start".into()));
        }
        let h = end - start + 1;
        let fc = forecast_core(fit, h)?;
        Ok(integrate_forecast(&fit.original, &self.order, &fc.mean))
    }

    pub fn forecast(&self, h: usize, alpha: f64) -> TsResult<ForecastResult> {
        let fit = self
            .fit
            .as_ref()
            .ok_or_else(|| TsError::NotFitted("call fit() first".into()))?;
        let mut fc = forecast_core(fit, h)?;
        fc.mean = integrate_forecast(&fit.original, &self.order, &fc.mean);
        let z = norm_ppf(1.0 - alpha / 2.0).map_err(|e| TsError::Error(e.to_string()))?;
        fc.lower = fc
            .mean
            .iter()
            .zip(&fc.se)
            .map(|(&m, &s)| m - z * s)
            .collect();
        fc.upper = fc
            .mean
            .iter()
            .zip(&fc.se)
            .map(|(&m, &s)| m + z * s)
            .collect();
        Ok(fc)
    }

    pub fn summary(&self) -> TsResult<String> {
        let fit = self
            .fit
            .as_ref()
            .ok_or_else(|| TsError::NotFitted("call fit() first".into()))?;
        let mut s = format!(
            "ARIMA({},{},{})({}, {}, {})x{}\nlog-likelihood={:.4} AIC={:.4} BIC={:.4}\n",
            fit.order.p,
            fit.order.d,
            fit.order.q,
            fit.order.seasonal_p,
            fit.order.seasonal_d,
            fit.order.seasonal_q,
            fit.order.seasonal_period,
            fit.log_likelihood,
            fit.aic,
            fit.bic,
        );
        if !fit.ar.is_empty() {
            s.push_str(&format!("AR: {:?}\n", fit.ar));
        }
        if !fit.ma.is_empty() {
            s.push_str(&format!("MA: {:?}\n", fit.ma));
        }
        s.push_str(&format!("sigma2={:.6}\n", fit.sigma2));
        Ok(s)
    }
}

fn prepare_series(endog: &[f64], order: &ArimaOrder) -> TsResult<Vec<f64>> {
    if endog.len() < order.p + order.q + order.d + 3 {
        return Err(TsError::Domain("series too short for order".into()));
    }
    let mut y = endog.to_vec();
    for _ in 0..order.d {
        y = diff(&y, 1)?;
    }
    if order.seasonal_period > 0 {
        for _ in 0..order.seasonal_d {
            y = seasonal_diff(&y, order.seasonal_period, 1)?;
        }
    }
    Ok(y)
}

fn fit_constant_only(y: &[f64], order: &ArimaOrder) -> TsResult<ArimaFit> {
    let c = mean(y)?;
    let residuals: Vec<f64> = y.iter().map(|v| v - c).collect();
    let sigma2 = var(&residuals, 1)?;
    let n = y.len();
    let ll = gaussian_loglik(n, sigma2, &residuals);
    let k = 1;
    Ok(ArimaFit {
        order: *order,
        ar: Vec::new(),
        ma: Vec::new(),
        seasonal_ar: Vec::new(),
        seasonal_ma: Vec::new(),
        constant: c,
        sigma2,
        log_likelihood: ll,
        aic: aic(ll, k, n),
        bic: bic(ll, k, n),
        aicc: aicc(ll, k, n),
        fitted: vec![c; n],
        residuals,
        endog: y.to_vec(),
        original: Vec::new(),
        warnings: Vec::new(),
    })
}

fn sample_autocov(x: &[f64], maxlag: usize) -> TsResult<Vec<f64>> {
    let n = x.len();
    let mu = mean(x)?;
    let mut gamma = vec![0.0; maxlag + 1];
    for k in 0..=maxlag {
        let mut s = 0.0;
        for t in k..n {
            s += (x[t] - mu) * (x[t - k] - mu);
        }
        gamma[k] = s / n as f64;
    }
    Ok(gamma)
}

fn fit_ar_yule_walker(y: &[f64], order: &ArimaOrder) -> TsResult<ArimaFit> {
    let p = order.p;
    let r_full = sample_autocov(y, p)?;
    let (phi, sigma2) = levinson(&r_full, p)?;
    let n = y.len();
    let mut fitted = vec![0.0; n];
    let mut residuals = vec![0.0; n];
    let c = mean(y)?;
    for t in 0..n {
        let mut pred = c;
        for (i, &ph) in phi.iter().enumerate() {
            if t > i {
                pred += ph * y[t - i - 1];
            }
        }
        fitted[t] = pred;
        residuals[t] = y[t] - pred;
    }
    let sigma2 = var(&residuals[p..], 1).unwrap_or(sigma2);
    let ll = gaussian_loglik(n - p, sigma2, &residuals[p..]);
    let k = p + 1;
    let mut warnings = Vec::new();
    if phi.iter().any(|&ph| ph.abs() > 0.99) {
        warnings.push("near unit root in AR coefficients".into());
    }
    Ok(ArimaFit {
        order: *order,
        ar: phi,
        ma: Vec::new(),
        seasonal_ar: Vec::new(),
        seasonal_ma: Vec::new(),
        constant: c,
        sigma2,
        log_likelihood: ll,
        aic: aic(ll, k, n),
        bic: bic(ll, k, n),
        aicc: aicc(ll, k, n),
        fitted,
        residuals,
        endog: y.to_vec(),
        original: Vec::new(),
        warnings,
    })
}

struct ArmaParams {
    ar: Vec<f64>,
    ma: Vec<f64>,
    constant: f64,
}

fn fit_arma_mle(y: &[f64], order: &ArimaOrder) -> TsResult<ArimaFit> {
    let p = order.p;
    let q = order.q;
    let n_params = p + q + 1;
    let n = y.len();
    if n <= p + q + 2 {
        return Err(TsError::Domain("series too short for ARMA MLE".into()));
    }

    // Hannan-Rissanen init for MA if q > 0
    let init = if q > 0 {
        hr_init(y, p, q)?
    } else {
        let r_full = sample_autocov(y, p)?;
        let (phi, _) = levinson(&r_full, p).unwrap_or((vec![0.0; p], r_full[0]));
        ArmaParams {
            ar: phi,
            ma: vec![0.0; q],
            constant: mean(y)?,
        }
    };

    let mut u_init = vec![0.0; n_params];
    u_init[0] = init.constant;
    for (i, &ph) in init.ar.iter().enumerate() {
        u_init[1 + i] = atanh_bounded(ph);
    }
    for (i, &th) in init.ma.iter().enumerate() {
        u_init[1 + p + i] = atanh_bounded(th);
    }

    let y_owned = y.to_vec();

    struct Ctx {
        y: Vec<f64>,
        p: usize,
        q: usize,
    }

    let ctx = Ctx {
        y: y_owned,
        p,
        q,
    };

    let mut obj = |u: &[f64], grad: &mut [f64]| -> f64 {
        let c = u[0];
        let ar: Vec<f64> = (0..ctx.p).map(|i| sigmoid_bounded(u[1 + i])).collect();
        let ma: Vec<f64> = (0..ctx.q)
            .map(|i| sigmoid_bounded(u[1 + ctx.p + i]))
            .collect();
        let (res, _) = arma_filter(&ctx.y, c, &ar, &ma);
        let m = res.len().saturating_sub(ctx.p.max(ctx.q));
        let slice = &res[ctx.p.max(ctx.q)..];
        let sigma2 = slice.iter().map(|r| r * r).sum::<f64>() / slice.len().max(1) as f64;
        let sigma2 = sigma2.max(1e-12);
        let ll = gaussian_loglik(slice.len(), sigma2, slice);
        let h = 1e-8;
        for i in 0..u.len() {
            let mut u_plus = u.to_vec();
            u_plus[i] += h;
            let ar_p: Vec<f64> = (0..ctx.p).map(|j| sigmoid_bounded(u_plus[1 + j])).collect();
            let ma_p: Vec<f64> = (0..ctx.q)
                .map(|j| sigmoid_bounded(u_plus[1 + ctx.p + j]))
                .collect();
            let (res_p, _) = arma_filter(&ctx.y, u_plus[0], &ar_p, &ma_p);
            let sl_p = &res_p[ctx.p.max(ctx.q)..];
            let s2_p = sl_p.iter().map(|r| r * r).sum::<f64>() / sl_p.len().max(1) as f64;
            let ll_p = gaussian_loglik(sl_p.len(), s2_p.max(1e-12), sl_p);
            grad[i] = (ll_p - ll) / h;
        }
        -ll
    };

    let res = minimize(
        &mut obj,
        &u_init,
        MinimizeMethod::LBfgs,
        None::<fn(&[f64], &mut [f64]) -> ()>,
        MinimizeOptions {
            max_iter: 500,
            gtol: 1e-6,
            ..Default::default()
        },
    );

    if !res.success {
        return Err(TsError::NonConvergence(format!(
            "ARMA MLE: {}",
            res.message
        )));
    }

    let u = res.x;
    let c = u[0];
    let ar: Vec<f64> = (0..p).map(|i| sigmoid_bounded(u[1 + i])).collect();
    let ma: Vec<f64> = (0..q).map(|i| sigmoid_bounded(u[1 + p + i])).collect();
    let (residuals, fitted) = arma_filter(y, c, &ar, &ma);
    let m = p.max(q);
    let slice = &residuals[m..];
    let sigma2 = slice.iter().map(|r| r * r).sum::<f64>() / slice.len().max(1) as f64;
    let ll = gaussian_loglik(slice.len(), sigma2.max(1e-12), slice);
    let k = n_params;
    let mut warnings = Vec::new();
    if ar.iter().chain(ma.iter()).any(|&c| c.abs() > 0.99) {
        warnings.push("near unit root / non-invertible".into());
    }

    Ok(ArimaFit {
        order: *order,
        ar,
        ma,
        seasonal_ar: Vec::new(),
        seasonal_ma: Vec::new(),
        constant: c,
        sigma2: sigma2.max(1e-12),
        log_likelihood: ll,
        aic: aic(ll, k, n),
        bic: bic(ll, k, n),
        aicc: aicc(ll, k, n),
        fitted,
        residuals,
        endog: y.to_vec(),
        original: Vec::new(),
        warnings,
    })
}

fn atanh_bounded(x: f64) -> f64 {
    x.clamp(-0.999, 0.999).atanh()
}

fn hr_init(y: &[f64], p: usize, q: usize) -> TsResult<ArmaParams> {
    let maxlag = p + q;
    let lm = crate::diagnostics::lagmat(y, maxlag, true)?;
    let n = lm.len();
    if n < p + q + 5 {
        return Ok(ArmaParams {
            ar: vec![0.0; p],
            ma: vec![0.0; q],
            constant: mean(y)?,
        });
    }
    let mut x = Vec::with_capacity(n);
    for row in &lm {
        let mut r = row.clone();
        x.push(r);
    }
    let yy: Vec<f64> = y[maxlag..].to_vec();
    let fit = niao_stats::regression::ols(&x, &yy).map_err(|e| TsError::Error(e.to_string()))?;
    let c = fit.coefficients[0];
    let ar: Vec<f64> = fit.coefficients[1..=p].to_vec();
    let ma: Vec<f64> = if q > 0 {
        fit.coefficients[p + 1..p + 1 + q]
            .iter()
            .map(|&v| (-v).clamp(-0.9, 0.9))
            .collect()
    } else {
        vec![]
    };
    Ok(ArmaParams { ar, ma, constant: c })
}

#[inline]
fn arma_filter(y: &[f64], c: f64, ar: &[f64], ma: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = y.len();
    let p = ar.len();
    let q = ma.len();
    let m = p.max(q);
    let mut e = vec![0.0; n];
    let mut fitted = vec![0.0; n];
    for t in 0..n {
        let mut pred = c;
        for (i, &ph) in ar.iter().enumerate() {
            if t > i {
                pred += ph * y[t - i - 1];
            }
        }
        for (j, &th) in ma.iter().enumerate() {
            if t > j {
                pred += th * e[t - j - 1];
            }
        }
        fitted[t] = pred;
        e[t] = y[t] - pred;
    }
    let _ = m;
    (e, fitted)
}

fn gaussian_loglik(n: usize, sigma2: f64, residuals: &[f64]) -> f64 {
    if n == 0 || sigma2 <= 0.0 {
        return f64::NEG_INFINITY;
    }
    let rss: f64 = residuals.iter().map(|r| r * r).sum();
    -0.5 * n as f64 * (2.0 * std::f64::consts::LN_2 + std::f64::consts::PI.ln() + sigma2.ln())
        - 0.5 * rss / sigma2
}

fn forecast_core(fit: &ArimaFit, h: usize) -> TsResult<ForecastResult> {
    let y = &fit.endog;
    let n = y.len();
    let p = fit.ar.len();
    let q = fit.ma.len();
    let mut extended_y = y.clone();
    let mut extended_e = fit.residuals.clone();
    let mut means = Vec::with_capacity(h);
    let mut ses = Vec::with_capacity(h);
    let mut psi = vec![1.0; h + 1];
    for j in 1..=h {
        let mut s = 0.0;
        for (i, &ph) in fit.ar.iter().enumerate() {
            if j > i + 1 {
                s += ph * psi[j - i - 1];
            } else if j == i + 1 {
                s += ph;
            }
        }
        psi[j] = s;
    }
    for step in 0..h {
        let t = n + step;
        let mut pred = fit.constant;
        for (i, &ph) in fit.ar.iter().enumerate() {
            if t > i {
                pred += ph * extended_y[t - i - 1];
            }
        }
        for (j, &th) in fit.ma.iter().enumerate() {
            if step <= j {
                // future shocks = 0
            } else if t > j {
                pred += th * extended_e[t - j - 1];
            }
        }
        means.push(pred);
        extended_y.push(pred);
        extended_e.push(0.0);
        let se = (fit.sigma2 * psi[step + 1].powi(2)).sqrt();
        ses.push(se);
    }
    Ok(ForecastResult {
        mean: means,
        lower: Vec::new(),
        upper: Vec::new(),
        se: ses,
    })
}

fn integrate_forecast(original: &[f64], order: &ArimaOrder, diff_fc: &[f64]) -> Vec<f64> {
    if order.d == 0 && order.seasonal_d == 0 {
        return diff_fc.to_vec();
    }
    let mut out = Vec::with_capacity(diff_fc.len());
    let mut level = *original.last().unwrap_or(&0.0);
    for &delta in diff_fc {
        level += delta;
        out.push(level);
    }
    out
}

/// Fast AR(p) via Yule–Walker (standalone).
pub fn ar_yule_walker(y: &[f64], p: usize) -> TsResult<(Vec<f64>, f64)> {
    let order = ArimaOrder::arima(p, 0, 0);
    let fit = fit_ar_yule_walker(y, &order)?;
    Ok((fit.ar, fit.sigma2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::E4073_NTS_NOT_FITTED;
    use crate::util::close;

    fn ar1_data(n: usize, phi: f64) -> Vec<f64> {
        let mut y = vec![0.0; n];
        y[0] = 1.0;
        let mut e = 0.1;
        for t in 1..n {
            e = (e * 16807.0) % 2147483647.0 / 2147483647.0 - 0.5;
            y[t] = phi * y[t - 1] + 0.1 * e;
        }
        y
    }

    #[test]
    fn ar1_yule_walker() {
        let y = ar1_data(500, 0.7);
        let (phi, _) = ar_yule_walker(&y, 1).unwrap();
        assert!(close(phi[0], 0.7, 0.05), "phi={}", phi[0]);
    }

    #[test]
    fn arima_fit_forecast() {
        let y = ar1_data(200, 0.6);
        let mut m = ArimaModel::arima(1, 0, 0);
        m.fit(&y).unwrap();
        let fc = m.forecast(5, 0.05).unwrap();
        assert_eq!(fc.mean.len(), 5);
        assert_eq!(fc.lower.len(), 5);
        assert!(fc.upper[0] > fc.mean[0]);
    }

    #[test]
    fn not_fitted_error() {
        let m = ArimaModel::arima(1, 0, 0);
        assert_eq!(m.forecast(1, 0.05).unwrap_err().code(), E4073_NTS_NOT_FITTED);
    }

    #[test]
    fn arma11_fit() {
        let mut y = vec![0.0; 300];
        y[0] = 0.0;
        let mut e_prev = 0.0;
        for t in 1..300 {
            let e = (t as f64 * 0.1).sin() * 0.1;
            y[t] = 0.5 * y[t - 1] + e + 0.3 * e_prev;
            e_prev = e;
        }
        let mut m = ArimaModel::arima(1, 0, 1);
        let fit = m.fit(&y);
        assert!(fit.is_ok(), "{:?}", fit.err());
    }
}
