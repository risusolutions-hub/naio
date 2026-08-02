//! Role hierarchy expansion and permission checks (~Django/Flask RBAC).

use std::collections::{HashMap, HashSet};

/// Role → list of inherited roles (transitive).
pub type RoleHierarchy = HashMap<String, Vec<String>>;

/// Expand roles through a hierarchy with cycle detection.
pub fn expand_roles(hierarchy: &RoleHierarchy, roles: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut stack: Vec<&str> = roles.iter().map(|s| s.as_str()).collect();
    while let Some(role) = stack.pop() {
        if !seen.insert(role.to_string()) {
            continue;
        }
        out.push(role.to_string());
        if let Some(children) = hierarchy.get(role) {
            for c in children {
                if !seen.contains(c.as_str()) {
                    stack.push(c.as_str());
                }
            }
        }
    }
    out
}

/// True if any expanded user role matches `required`.
pub fn allows(hierarchy: &RoleHierarchy, user_roles: &[String], required: &str) -> bool {
    expand_roles(hierarchy, user_roles)
        .iter()
        .any(|r| r == required)
}

/// True if every required role is present after expansion.
pub fn allows_all(hierarchy: &RoleHierarchy, user_roles: &[String], required: &[String]) -> bool {
    let expanded: HashSet<String> = expand_roles(hierarchy, user_roles).into_iter().collect();
    required.iter().all(|r| expanded.contains(r))
}

/// True if any of the required roles is present after expansion.
pub fn allows_any(hierarchy: &RoleHierarchy, user_roles: &[String], required: &[String]) -> bool {
    if required.is_empty() {
        return true;
    }
    let expanded: HashSet<String> = expand_roles(hierarchy, user_roles).into_iter().collect();
    required.iter().any(|r| expanded.contains(r))
}

/// Exact membership (no hierarchy).
pub fn has_role(roles: &[String], role: &str) -> bool {
    roles.iter().any(|r| r == role)
}

/// Exact permission membership. `"*"` in perms grants all.
pub fn has_permission(perms: &[String], perm: &str) -> bool {
    perms.iter().any(|p| p == "*" || p == perm)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hier() -> RoleHierarchy {
        let mut h = RoleHierarchy::new();
        h.insert("admin".into(), vec!["editor".into(), "viewer".into()]);
        h.insert("editor".into(), vec!["viewer".into()]);
        h
    }

    #[test]
    fn expand_transitive() {
        let e = expand_roles(&hier(), &["admin".into()]);
        assert!(e.contains(&"admin".into()));
        assert!(e.contains(&"editor".into()));
        assert!(e.contains(&"viewer".into()));
    }

    #[test]
    fn allows_inherited() {
        assert!(allows(&hier(), &["admin".into()], "viewer"));
        assert!(!allows(&hier(), &["viewer".into()], "admin"));
    }

    #[test]
    fn cycle_safe() {
        let mut h = RoleHierarchy::new();
        h.insert("a".into(), vec!["b".into()]);
        h.insert("b".into(), vec!["a".into()]);
        let e = expand_roles(&h, &["a".into()]);
        assert_eq!(e.len(), 2);
    }

    #[test]
    fn star_permission() {
        assert!(has_permission(&["*".into()], "anything"));
        assert!(!has_permission(&["read".into()], "write"));
    }
}
