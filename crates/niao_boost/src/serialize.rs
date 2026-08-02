//! JSON model serialization (stdlib-only, no njson dependency).

use crate::binning::BinnedMatrix;
use crate::booster::Booster;
use crate::error::{BoostError, BoostResult};
use crate::objective::TaskKind;
use crate::params::BoosterParams;
use crate::tree::{Tree, TreeNode};

pub fn save_model(booster: &Booster, path: &str) -> BoostResult<()> {
    let json = model_to_json(booster)?;
    std::fs::write(path, json).map_err(|e| BoostError::Io(e.to_string()))
}

pub fn load_model(path: &str) -> BoostResult<Booster> {
    let text = std::fs::read_to_string(path).map_err(|e| BoostError::Io(e.to_string()))?;
    model_from_json(&text)
}

pub fn model_to_json(booster: &Booster) -> BoostResult<String> {
    if !booster.fitted {
        return Err(BoostError::NotFitted);
    }
    let mut out = String::new();
    out.push_str("{\"version\":1,");
    out.push_str(&format!("\"task\":\"{:?}\",", booster.task));
    out.push_str(&format!("\"num_class\":{},", booster.num_class));
    out.push_str(&format!("\"best_iteration\":{},", booster.best_iteration));
    out.push_str(&format!(
        "\"learning_rate\":{},",
        booster.params.learning_rate
    ));
    out.push_str(&format!("\"lambda_l2\":{},", booster.params.lambda_l2));
    out.push_str("\"trees\":[");
    for (i, tree) in booster.trees.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&tree_to_json(&tree.root));
    }
    out.push_str("],");
    out.push_str("\"thresholds\":[");
    for (fi, th) in booster.binned_template.thresholds.iter().enumerate() {
        if fi > 0 {
            out.push(',');
        }
        out.push('[');
        for (bi, b) in th.iter().enumerate() {
            if bi > 0 {
                out.push(',');
            }
            out.push_str(&format!("{b:.17}"));
        }
        out.push(']');
    }
    out.push_str("]}");
    Ok(out)
}

fn tree_to_json(node: &TreeNode) -> String {
    match node {
        TreeNode::Leaf { value, cover } => {
            format!("{{\"t\":\"L\",\"v\":{value:.17},\"c\":{cover}}}")
        }
        TreeNode::Split {
            feature,
            bin,
            default_left,
            gain,
            left,
            right,
        } => format!(
            "{{\"t\":\"S\",\"f\":{feature},\"b\":{bin},\"d\":{default_left},\"g\":{gain:.17},\"l\":{},\"r\":{}}}",
            tree_to_json(left),
            tree_to_json(right)
        ),
    }
}

pub fn model_from_json(text: &str) -> BoostResult<Booster> {
    let task = if text.contains("\"task\":\"Regression\"") {
        TaskKind::Regression
    } else if text.contains("\"task\":\"Binary\"") {
        TaskKind::Binary
    } else {
        TaskKind::Multiclass
    };
    let num_class = extract_usize(text, "\"num_class\":").unwrap_or(1);
    let best_iteration = extract_usize(text, "\"best_iteration\":").unwrap_or(0);
    let learning_rate = extract_f64(text, "\"learning_rate\":").unwrap_or(0.1);
    let lambda_l2 = extract_f64(text, "\"lambda_l2\":").unwrap_or(1.0);

    let mut params = BoosterParams::default();
    params.learning_rate = learning_rate;
    params.lambda_l2 = lambda_l2;

    let trees_start = text
        .find("\"trees\":[")
        .ok_or_else(|| BoostError::Io("missing trees".into()))?
        + 9;
    let trees_end = text
        .rfind("],\"thresholds\"")
        .ok_or_else(|| BoostError::Io("bad trees".into()))?;
    let trees_json = &text[trees_start..trees_end];

    let mut trees = Vec::new();
    let mut pos = 0;
    while pos < trees_json.len() {
        while pos < trees_json.len()
            && (trees_json.as_bytes()[pos] == b',' || trees_json.as_bytes()[pos] == b' ')
        {
            pos += 1;
        }
        if pos >= trees_json.len() {
            break;
        }
        if trees_json.as_bytes()[pos] != b'{' {
            pos += 1;
            continue;
        }
        let (node, next) = parse_tree_node(trees_json, pos)?;
        trees.push(Tree {
            root: node,
            num_leaves: 1,
        });
        pos = next;
    }

    let thresholds = parse_thresholds(text)?;
    let n_features = thresholds.len();

    Ok(Booster {
        trees,
        params,
        task,
        num_class,
        base_score: 0.0,
        feature_importance_gain: vec![0.0; n_features],
        feature_importance_split: vec![0; n_features],
        feature_importance_cover: vec![0; n_features],
        best_iteration,
        eval_log: Vec::new(),
        fitted: true,
        binned_template: BinnedMatrix {
            n_rows: 0,
            n_features,
            max_bins: 256,
            bins: Vec::new(),
            thresholds,
            missing: Vec::new(),
        },
    })
}

fn parse_tree_node(s: &str, start: usize) -> BoostResult<(TreeNode, usize)> {
    let rest = &s[start..];
    if rest.starts_with("{\"t\":\"L\"") {
        let value = extract_f64(rest, "\"v\":").unwrap_or(0.0);
        let cover = extract_usize(rest, "\"c\":").unwrap_or(0) as u32;
        let end = find_matching_brace(rest, 0)? + 1;
        return Ok((TreeNode::Leaf { value, cover }, start + end));
    }
    if rest.starts_with("{\"t\":\"S\"") {
        let feature = extract_usize(rest, "\"f\":").unwrap_or(0) as u16;
        let bin = extract_usize(rest, "\"b\":").unwrap_or(0) as u8;
        let default_left = rest.contains("\"d\":true");
        let gain = extract_f64(rest, "\"g\":").unwrap_or(0.0);
        let lpos = rest
            .find("\"l\":")
            .ok_or_else(|| BoostError::Io("missing l".into()))?
            + 4;
        let (left, mid) = parse_tree_node(s, start + lpos)?;
        let rpos = s[mid..]
            .find("\"r\":")
            .ok_or_else(|| BoostError::Io("missing r".into()))?
            + mid
            + 4;
        let (right, end) = parse_tree_node(s, rpos)?;
        return Ok((
            TreeNode::Split {
                feature,
                bin,
                default_left,
                gain,
                left: Box::new(left),
                right: Box::new(right),
            },
            end,
        ));
    }
    Err(BoostError::Io("bad tree node".into()))
}

fn find_matching_brace(s: &str, start: usize) -> BoostResult<usize> {
    let bytes = s.as_bytes();
    if bytes.get(start) != Some(&b'{') {
        return Err(BoostError::Io("expected brace".into()));
    }
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i - start);
                }
            }
            _ => {}
        }
    }
    Err(BoostError::Io("unclosed brace".into()))
}

fn parse_thresholds(text: &str) -> BoostResult<Vec<Vec<f64>>> {
    let start = text
        .find("\"thresholds\":[")
        .ok_or_else(|| BoostError::Io("missing thresholds".into()))?
        + 14;
    let end = text
        .rfind(']')
        .ok_or_else(|| BoostError::Io("bad thresholds".into()))?;
    let inner = &text[start..end];
    let mut out = Vec::new();
    let mut i = 0;
    while i < inner.len() {
        if inner.as_bytes()[i] == b'[' {
            i += 1;
            let mut vals = Vec::new();
            while i < inner.len() && inner.as_bytes()[i] != b']' {
                let num_start = i;
                while i < inner.len()
                    && (inner.as_bytes()[i].is_ascii_digit()
                        || inner.as_bytes()[i] == b'.'
                        || inner.as_bytes()[i] == b'-'
                        || inner.as_bytes()[i] == b'e'
                        || inner.as_bytes()[i] == b'E'
                        || inner.as_bytes()[i] == b'+')
                {
                    i += 1;
                }
                if num_start < i {
                    vals.push(
                        inner[num_start..i]
                            .parse::<f64>()
                            .map_err(|e| BoostError::Io(e.to_string()))?,
                    );
                }
                while i < inner.len()
                    && (inner.as_bytes()[i] == b',' || inner.as_bytes()[i] == b' ')
                {
                    i += 1;
                }
            }
            out.push(vals);
            i += 1;
        } else {
            i += 1;
        }
    }
    Ok(out)
}

fn extract_f64(s: &str, key: &str) -> Option<f64> {
    let i = s.find(key)? + key.len();
    let rest = &s[i..];
    let end = rest
        .find(|c: char| c == ',' || c == '}' || c == ']')
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn extract_usize(s: &str, key: &str) -> Option<usize> {
    extract_f64(s, key).map(|v| v as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::booster::Booster;
    use crate::objective::TaskKind;
    use crate::params::BoosterParams;
    use crate::tree::{Tree, TreeNode};

    #[test]
    fn roundtrip_json() {
        let mut booster = Booster::new(BoosterParams::default(), TaskKind::Regression, 1).unwrap();
        booster.fitted = true;
        booster.trees.push(Tree {
            root: TreeNode::Split {
                feature: 0,
                bin: 1,
                default_left: true,
                gain: 0.5,
                left: Box::new(TreeNode::Leaf {
                    value: 0.1,
                    cover: 3,
                }),
                right: Box::new(TreeNode::Leaf {
                    value: -0.2,
                    cover: 2,
                }),
            },
            num_leaves: 2,
        });
        booster.binned_template.thresholds = vec![vec![0.5, 1.5]];
        let json = model_to_json(&booster).unwrap();
        let loaded = model_from_json(&json).unwrap();
        assert_eq!(loaded.trees.len(), 1);
        assert!(loaded.fitted);
    }
}
