use std::borrow::Cow;
use std::path::PathBuf;
use crate::parsing::ast::{Expr, Identifier, Type};
use crate::parsing::span::{Span, Spanned};
use crate::r#static::info::{IdInfo, TypeEnv, WarningKind, XsError};

/// Needleman - Wunsch distance
pub fn dist(s1: &str, s2: &str) -> f32 {
    const MIS_COST: i32 = 8;
    const GAP_OPEN: i32 = 8;
    const GAP_EXTEND: i32 = 1;

    let a = s1.as_bytes();
    let b = s2.as_bytes();

    let n = a.len();
    let m = b.len();

    const INF: i32 = i32::MAX / 4;

    let mut mat = vec![vec![INF; m + 1]; n + 1];
    let mut gap_a = vec![vec![INF; m + 1]; n + 1];
    let mut gap_b = vec![vec![INF; m + 1]; n + 1];

    mat[0][0] = 0;

    #[allow(clippy::needless_range_loop)]
    for i in 1..=n {
        gap_b[i][0] = GAP_OPEN + (i as i32 - 1) * GAP_EXTEND;
    }

    #[allow(clippy::needless_range_loop)]
    for j in 1..=m {
        gap_a[0][j] = GAP_OPEN + (j as i32 - 1) * GAP_EXTEND;
    }

    for i in 1..=n {
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] {
                0
            } else {
                MIS_COST
            };

            mat[i][j] = mat[i - 1][j - 1]
                .min(gap_a[i - 1][j - 1])
                .min(gap_b[i - 1][j - 1])
                + cost;

            gap_a[i][j] = (mat[i][j - 1] + GAP_OPEN)
                .min(gap_a[i][j - 1] + GAP_EXTEND)
                .min(gap_b[i][j - 1] + GAP_OPEN);

            gap_b[i][j] = (mat[i - 1][j] + GAP_OPEN)
                .min(gap_b[i - 1][j] + GAP_EXTEND)
                .min(gap_a[i - 1][j] + GAP_OPEN);
        }
    }

    let score = mat[n][m]
        .min(gap_a[n][m])
        .min(gap_b[n][m]);

    // Maximum possible cost is substituting every aligned character
    // plus one gap run for any length difference.
    let common = n.min(m) as i32;
    let diff = n.abs_diff(m) as i32;

    let max_score = common * MIS_COST
        + if diff == 0 {
        0
    } else {
        GAP_OPEN + (diff - 1) * GAP_EXTEND
    };

    if max_score == 0 {
        0.0
    } else {
        score as f32 / max_score as f32
    }
}

pub fn get_param_name(id: &Identifier) -> Option<String> {
    fn has_numeric_suffix(s: &str, prefix: &str) -> bool {
        s.strip_prefix(prefix)
            .is_some_and(|rest| rest.bytes().all(|b| b.is_ascii_digit()))
    }
    let mut name = id.0.as_str();
    if matches!(name, "key" | "value" | "label")
        || has_numeric_suffix(name, "arg")
        || has_numeric_suffix(name, "param")
        || has_numeric_suffix(name, "str") {
        return None;
    }

    // common name conventions for cConstant, kConstant, bBool, mMember, sStatic, gGlobal
    if (name.starts_with("c") || name.starts_with("k") || name.starts_with("b")
        || name.starts_with("m") || name.starts_with("s") || name.starts_with("g"))
        && name.chars().nth(1).is_some_and(|c| c.is_ascii_uppercase()) {
        name = &name[1..];
    }
    name = name.strip_suffix("id").unwrap_or(name);
    name = name.strip_suffix("Id").unwrap_or(name);
    name = name.strip_suffix("ID").unwrap_or(name);
    name = name.strip_suffix("ids").unwrap_or(name);
    name = name.strip_suffix("Ids").unwrap_or(name);
    name = name.strip_suffix("IDs").unwrap_or(name);
    name = name.strip_suffix("_").unwrap_or(name);
    name = name.strip_prefix("_").unwrap_or(name);
    if name.is_empty() {
        None
    }
    else {
        Some(name.into())
    }
}

pub fn get_arg_name((expr, _span): &Spanned<Expr>, type_env: &TypeEnv, type_override: Option<Type>) -> Option<(String, Type)> {
    match expr {
        Expr::Identifier(id) => {
            let type_ = match type_override {
                Some(type_) => Cow::Owned(type_),
                None => {
                    let IdInfo { type_, .. } = type_env.get_ref(id)?;
                    Cow::Borrowed(type_)
                }
            };

            get_param_name(id).map(|name| (name, type_.into_owned()))
        }
        Expr::FnCall { name: id, .. } => {
            let Some(IdInfo { type_: Type::Fn { type_sign, .. }, .. }) = type_env.get_ref(&id.0) else {
                return None;
            };

            let mut name = id.0.0.as_str();

            name = name.strip_prefix("xs").unwrap_or(name);
            name = name.strip_prefix("Vector").unwrap_or(name);
            name = name.strip_prefix("Array").unwrap_or(name);
            name = name.strip_prefix("str").unwrap_or(name);
            name = name.strip_prefix("get").unwrap_or(name);
            name = name.strip_prefix("set").unwrap_or(name);
            name = name.strip_prefix("is").unwrap_or(name);
            name = name.strip_prefix("does").unwrap_or(name);
            name = name.strip_prefix("Get").unwrap_or(name);
            name = name.strip_prefix("Set").unwrap_or(name);
            name = name.strip_prefix("Is").unwrap_or(name);
            name = name.strip_prefix("Does").unwrap_or(name);
            name = name.strip_suffix("Id").unwrap_or(name);
            name = name.strip_suffix("ID").unwrap_or(name);
            name = name.strip_suffix("id").unwrap_or(name);

            let rtype = type_sign.last().expect("Return type").1.clone();
            if name.is_empty() || rtype == Type::Void {
                None
            } else {
                Some((name.into(), rtype))
            }
        }
        Expr::Paren(inner) => get_arg_name(inner, type_env, None),
        _ => None,
    }
}

fn contains_keywords(name: &Identifier) -> bool {
    let s = name.0.as_str();
    s.contains("backward")
    || s.contains("complement")
    || s.contains("endian")
    || s.contains("invert")
    || s.contains("inverse")
    || s.contains("landscape")
    || s.contains("opposite")
    || s.contains("portrait")
    || s.contains("reciprocal")
    || s.contains("reverse")
    || s.contains("rotate")
    || s.contains("rotation")
    || s.contains("swap")
    || s.contains("transpose")
    || s.contains("undo")
}

fn bipartite_matching(costs: &[Vec<Option<f32>>]) -> Vec<usize> {
    let n = costs.len();

    let mut best_cost = f32::INFINITY;
    let mut best_matching = vec![0; n];

    let mut current_matching = vec![0; n];
    let mut used = vec![false; n];

    fn dfs(
        row: usize,
        costs: &[Vec<Option<f32>>],
        used: &mut [bool],
        current_matching: &mut [usize],
        current_cost: f32,
        best_cost: &mut f32,
        best_matching: &mut Vec<usize>,
    ) {
        if row == costs.len() {
            if current_cost < *best_cost {
                *best_cost = current_cost;
                best_matching.copy_from_slice(current_matching);
            }
            return;
        }

        if current_cost >= *best_cost {
            return;
        }

        for col in 0..costs.len() {
            if used[col] {
                continue;
            }

            let Some(cost) = costs[row][col] else {
                continue;
            };

            used[col] = true;
            current_matching[row] = col;

            dfs(
                row + 1,
                costs,
                used,
                current_matching,
                current_cost + cost,
                best_cost,
                best_matching,
            );

            used[col] = false;
        }
    }

    dfs(
        0,
        costs,
        &mut used,
        &mut current_matching,
        0.0,
        &mut best_cost,
        &mut best_matching,
    );

    best_matching
}

/// Implements the algorithm presented in this paper by Rice et al. 2017,
/// [Detecting Argument Selection Defects](https://static.googleusercontent.com/media/research.google.com/ru//pubs/archive/46317.pdf)
pub fn warn_arg_selection_defects(path: &PathBuf, name: &Identifier, name_span: &Span, type_sign: &[(Identifier, Type)], args: &[Spanned<Expr>], type_env: &mut TypeEnv) {
    let Some(fn_env) = &type_env.current_fnv_env else {
        return;
    };
    let is_recursive = fn_env.name == *name;
    if is_recursive || contains_keywords(&fn_env.name) {
        return;
    }
    let (params, args) = type_sign.iter().zip(args)
        .filter_map(|(param, arg)| {
            let param_name = get_param_name(&param.0);
            let arg_name = get_arg_name(arg, type_env, None);
            let (Some(param_name), Some(arg_name)) = (param_name, arg_name) else {
                return None;
            };
            Some(((param_name, param.1.clone()), arg_name))
        }).unzip::<_, _, Vec<_>, Vec<_>>();

    let mut costs = vec![vec![None; params.len()]; params.len()];

    for (i, (name, type_)) in params.iter().enumerate() {
        let distance = dist(name, &args[i].0);
        costs[i][i] = Some(distance);
        for (j, (arg_name, arg_type)) in args.iter().enumerate() {
            if i == j || !type_.accepts(arg_type) {
                continue;
            }
            let other_distance = dist(name, arg_name);
            if other_distance < distance {
                costs[i][j] = Some(other_distance);
            }
        }
    }

    let matches = bipartite_matching(&costs);

    let mut mismatches = Vec::new();
    let mut sum = 0_f32;
    for (i, j) in matches.into_iter().enumerate() {
        if i == j {
            continue;
        }
        mismatches.push(format!("{}({})", &params[i].0, &args[i].0));
        sum += costs[i][i].expect("refl") - costs[i][j].expect("getting a pairing without it having a cost = matching algo bug");
    }
    if mismatches.is_empty() || sum/(mismatches.len() as f32) < 0.3 {
        return;
    }

    type_env.add_err(path, XsError::warning(
        name_span,
        &format!(
            "Arguments passed in possibly wrong order. Review following parameters and their corresponding arguments: {}",
            mismatches.join(", ")
        ),
        vec![],
        WarningKind::SwappedParams,
    ));
}