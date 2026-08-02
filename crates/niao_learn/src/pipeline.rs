//! Pipeline composition.

use crate::error::{LearnError, LearnResult};
use crate::linear::LinearRegression;
use crate::logistic::LogisticRegression;
use crate::neighbors::KNeighborsClassifier;
use crate::preprocessing::{
    Binarizer, LabelEncoder, MinMaxScaler, Normalizer, OneHotEncoder, OrdinalEncoder,
    PolynomialFeatures, RobustScaler, SimpleImputer, StandardScaler,
};
use crate::traits::{Estimator, Predictor, Scorer, Transformer};
use crate::tree::DecisionTreeClassifier;
use niao_num::NdArray;

/// A named pipeline step that is either a transformer or a final estimator.
#[derive(Clone, Debug)]
pub enum Step {
    StandardScaler(StandardScaler),
    MinMaxScaler(MinMaxScaler),
    RobustScaler(RobustScaler),
    Normalizer(Normalizer),
    Binarizer(Binarizer),
    SimpleImputer(SimpleImputer),
    OneHotEncoder(OneHotEncoder),
    OrdinalEncoder(OrdinalEncoder),
    LabelEncoder(LabelEncoder),
    PolynomialFeatures(PolynomialFeatures),
    LogisticRegression(LogisticRegression),
    LinearRegression(LinearRegression),
    DecisionTreeClassifier(DecisionTreeClassifier),
    KNeighborsClassifier(KNeighborsClassifier),
}

impl Step {
    fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<()> {
        match self {
            Step::StandardScaler(s) => s.fit(x, y),
            Step::MinMaxScaler(s) => s.fit(x, y),
            Step::RobustScaler(s) => s.fit(x, y),
            Step::Normalizer(s) => s.fit(x, y),
            Step::Binarizer(s) => s.fit(x, y),
            Step::SimpleImputer(s) => s.fit(x, y),
            Step::OneHotEncoder(s) => s.fit(x, y),
            Step::OrdinalEncoder(s) => s.fit(x, y),
            Step::LabelEncoder(s) => s.fit(x, y),
            Step::PolynomialFeatures(s) => s.fit(x, y),
            Step::LogisticRegression(s) => s.fit(x, y),
            Step::LinearRegression(s) => s.fit(x, y),
            Step::DecisionTreeClassifier(s) => s.fit(x, y),
            Step::KNeighborsClassifier(s) => s.fit(x, y),
        }
    }

    fn transform(&self, x: &NdArray) -> LearnResult<NdArray> {
        match self {
            Step::StandardScaler(s) => s.transform(x),
            Step::MinMaxScaler(s) => s.transform(x),
            Step::RobustScaler(s) => s.transform(x),
            Step::Normalizer(s) => s.transform(x),
            Step::Binarizer(s) => s.transform(x),
            Step::SimpleImputer(s) => s.transform(x),
            Step::OneHotEncoder(s) => s.transform(x),
            Step::OrdinalEncoder(s) => s.transform(x),
            Step::LabelEncoder(s) => s.transform(x),
            Step::PolynomialFeatures(s) => s.transform(x),
            _ => Err(LearnError::Error("final estimator has no transform".into())),
        }
    }

    fn is_transformer(&self) -> bool {
        !matches!(
            self,
            Step::LogisticRegression(_)
                | Step::LinearRegression(_)
                | Step::DecisionTreeClassifier(_)
                | Step::KNeighborsClassifier(_)
        )
    }

    fn predict(&self, x: &NdArray) -> LearnResult<NdArray> {
        match self {
            Step::LogisticRegression(s) => s.predict(x),
            Step::LinearRegression(s) => s.predict(x),
            Step::DecisionTreeClassifier(s) => s.predict(x),
            Step::KNeighborsClassifier(s) => s.predict(x),
            _ => Err(LearnError::Error("step has no predict".into())),
        }
    }

    fn score(&self, x: &NdArray, y: &NdArray) -> LearnResult<f64> {
        match self {
            Step::LogisticRegression(s) => s.score(x, y),
            Step::LinearRegression(s) => s.score(x, y),
            Step::DecisionTreeClassifier(s) => s.score(x, y),
            Step::KNeighborsClassifier(s) => s.score(x, y),
            _ => Err(LearnError::Error("step has no score".into())),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Pipeline {
    pub steps: Vec<(String, Step)>,
    fitted: bool,
}

impl Pipeline {
    pub fn new(steps: Vec<(String, Step)>) -> Self {
        Self {
            steps,
            fitted: false,
        }
    }

    fn transform_prefix(&self, x: &NdArray, upto: usize) -> LearnResult<NdArray> {
        let mut cur = x.clone();
        for i in 0..upto {
            if self.steps[i].1.is_transformer() {
                cur = self.steps[i].1.transform(&cur)?;
            }
        }
        Ok(cur)
    }
}

impl Estimator for Pipeline {
    fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<()> {
        if self.steps.is_empty() {
            return Err(LearnError::Error("empty pipeline".into()));
        }
        let mut cur = x.clone();
        let last = self.steps.len() - 1;
        for i in 0..last {
            if !self.steps[i].1.is_transformer() {
                return Err(LearnError::Error(
                    "only the final pipeline step may be an estimator".into(),
                ));
            }
            self.steps[i].1.fit(&cur, y)?;
            cur = self.steps[i].1.transform(&cur)?;
        }
        self.steps[last].1.fit(&cur, y)?;
        self.fitted = true;
        Ok(())
    }
}

impl Predictor for Pipeline {
    fn predict(&self, x: &NdArray) -> LearnResult<NdArray> {
        if !self.fitted {
            return Err(LearnError::NotFitted("Pipeline not fitted".into()));
        }
        let last = self.steps.len() - 1;
        let cur = self.transform_prefix(x, last)?;
        self.steps[last].1.predict(&cur)
    }
}

impl Scorer for Pipeline {
    fn score(&self, x: &NdArray, y: &NdArray) -> LearnResult<f64> {
        if !self.fitted {
            return Err(LearnError::NotFitted("Pipeline not fitted".into()));
        }
        let last = self.steps.len() - 1;
        let cur = self.transform_prefix(x, last)?;
        self.steps[last].1.score(&cur, y)
    }
}

impl Transformer for Pipeline {
    fn transform(&self, x: &NdArray) -> LearnResult<NdArray> {
        if !self.fitted {
            return Err(LearnError::NotFitted("Pipeline not fitted".into()));
        }
        // transform through all transformer steps (exclude final estimator if present)
        let mut end = self.steps.len();
        if !self.steps[end - 1].1.is_transformer() {
            end -= 1;
        }
        self.transform_prefix(x, end)
    }
}
