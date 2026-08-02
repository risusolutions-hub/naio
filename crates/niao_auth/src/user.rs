//! User context objects (~Flask-Login UserMixin / Django AnonymousUser).

use niao_json_core::{Object, Value};

/// Build an authenticated user JSON object.
pub fn user(id: &str, roles: &[String], permissions: &[String], active: bool) -> Value {
    let mut obj = Object::new();
    obj.insert("id".into(), Value::string(id));
    obj.insert(
        "roles".into(),
        Value::array(roles.iter().map(|r| Value::string(r.as_str())).collect()),
    );
    obj.insert(
        "permissions".into(),
        Value::array(
            permissions
                .iter()
                .map(|p| Value::string(p.as_str()))
                .collect(),
        ),
    );
    obj.insert("is_authenticated".into(), Value::bool(true));
    obj.insert("is_anonymous".into(), Value::bool(false));
    obj.insert("is_active".into(), Value::bool(active));
    Value::object(obj)
}

/// Anonymous user (not logged in).
pub fn anonymous() -> Value {
    let mut obj = Object::new();
    obj.insert("id".into(), Value::Null);
    obj.insert("roles".into(), Value::array(vec![]));
    obj.insert("permissions".into(), Value::array(vec![]));
    obj.insert("is_authenticated".into(), Value::bool(false));
    obj.insert("is_anonymous".into(), Value::bool(true));
    obj.insert("is_active".into(), Value::bool(false));
    Value::object(obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_flags() {
        let u = user("u1", &["admin".into()], &[], true);
        assert_eq!(u.get("id").and_then(|v| v.as_str()), Some("u1"));
        assert_eq!(
            u.get("is_authenticated").and_then(|v| v.as_bool()),
            Some(true)
        );
        let a = anonymous();
        assert_eq!(a.get("is_anonymous").and_then(|v| v.as_bool()), Some(true));
    }
}
