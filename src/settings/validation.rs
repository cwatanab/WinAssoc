//! UI からの保存時バリデーション

use crate::config::Config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    DuplicateAppName(String),
    InvalidAppName(String),
    EmptyCmd(String),
    DuplicateExt(String),
    InvalidExtKey(String),
    DuplicateProtocol(String),
    InvalidProtocolScheme(String),
    InvalidModifier(String),
    UnknownDefaultApp { section: String, key: String, app: String },
    UnknownCandidate { section: String, key: String, app: String },
    UnknownRuleApp { section: String, key: String, rule_idx: usize, app: String },
    RuleHasNoAction { section: String, key: String, rule_idx: usize },
    EmptyRule { section: String, key: String, rule_idx: usize },
    EmptyCandidates(String),
}

pub fn validate(config: &Config) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let known_apps: std::collections::HashSet<&str> = config.apps.keys().map(|s| s.as_str()).collect();

    for name in config.apps.keys() {
        if !is_valid_app_name(name) {
            errors.push(ValidationError::InvalidAppName(name.clone()));
        }
        if let Some(def) = config.apps.get(name) {
            if def.cmd.trim().is_empty() {
                errors.push(ValidationError::EmptyCmd(name.clone()));
            }
        }
    }

    for (key, table) in &config.ext {
        if !is_valid_ext_key(key) {
            errors.push(ValidationError::InvalidExtKey(key.clone()));
        }
        validate_route_table(&mut errors, "ext", key, table, &known_apps);
    }

    for (key, table) in &config.protocol {
        if !is_valid_protocol_scheme(key) {
            errors.push(ValidationError::InvalidProtocolScheme(key.clone()));
        }
        validate_route_table(&mut errors, "protocol", key, table, &known_apps);
    }

    errors
}

fn validate_route_table(
    errors: &mut Vec<ValidationError>,
    section: &str,
    key: &str,
    table: &crate::config::RouteTable,
    known_apps: &std::collections::HashSet<&str>,
) {
    if let Some(default) = &table.default {
        if !known_apps.contains(default.as_str()) {
            errors.push(ValidationError::UnknownDefaultApp { section: section.into(), key: key.into(), app: default.clone() });
        }
    }
    if let Some(candidates) = &table.candidates {
        if candidates.is_empty() {
            errors.push(ValidationError::EmptyCandidates(key.into()));
        }
        for c in candidates {
            if !known_apps.contains(c.as_str()) {
                errors.push(ValidationError::UnknownCandidate { section: section.into(), key: key.into(), app: c.clone() });
            }
        }
    }
    for (idx, rule) in table.rules.iter().enumerate() {
        let has_condition = rule.glob.is_some() || rule.host.is_some() || rule.url.is_some() || rule.modifier.is_some();
        let has_action = rule.pick || rule.app.is_some();

        if !has_action {
            errors.push(ValidationError::RuleHasNoAction { section: section.into(), key: key.into(), rule_idx: idx });
        }
        if !has_condition && !has_action {
            errors.push(ValidationError::EmptyRule { section: section.into(), key: key.into(), rule_idx: idx });
        }
        if let Some(m) = &rule.modifier {
            if !matches!(m.to_ascii_lowercase().as_str(), "shift" | "ctrl" | "alt") {
                errors.push(ValidationError::InvalidModifier(m.clone()));
            }
        }
        if let Some(app) = &rule.app {
            if !known_apps.contains(app.as_str()) {
                errors.push(ValidationError::UnknownRuleApp { section: section.into(), key: key.into(), rule_idx: idx, app: app.clone() });
            }
        }
    }
}

fn is_valid_app_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}
fn is_valid_ext_key(s: &str) -> bool {
    let name = if s.starts_with('.') { &s[1..] } else { s };
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric())
}
fn is_valid_protocol_scheme(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppDef, RouteTable, Rule};
    use std::collections::BTreeMap;

    fn empty() -> Config { Config::default_config() }

    #[test] fn default_config_is_valid() { assert_eq!(validate(&empty()).len(), 0); }
    #[test] fn invalid_app_name_is_error() {
        let mut c = empty();
        c.apps.insert("with space".to_string(), AppDef::default());
        assert!(validate(&c).iter().any(|e| matches!(e, ValidationError::InvalidAppName(_))));
    }
    #[test] fn empty_cmd_is_error() {
        let mut c = empty();
        c.apps.insert("ok".to_string(), AppDef::default());
        assert!(validate(&c).iter().any(|e| matches!(e, ValidationError::EmptyCmd(_))));
    }
    #[test] fn unknown_default_app_is_error() {
        let mut c = empty();
        c.apps.insert("real".to_string(), AppDef { cmd: "x".into(), args: vec![], label: None });
        let mut ext = BTreeMap::new();
        ext.insert(".md".to_string(), RouteTable { default: Some("ghost".into()), rules: vec![], candidates: None });
        c.ext = ext;
        assert!(validate(&c).iter().any(|e| matches!(e, ValidationError::UnknownDefaultApp { .. })));
    }
    #[test] fn unknown_candidate_is_error() {
        let mut c = empty();
        c.apps.insert("real".to_string(), AppDef { cmd: "x".into(), args: vec![], label: None });
        let mut ext = BTreeMap::new();
        ext.insert(".md".to_string(), RouteTable { default: None, rules: vec![], candidates: Some(vec!["ghost".into()]) });
        c.ext = ext;
        assert!(validate(&c).iter().any(|e| matches!(e, ValidationError::UnknownCandidate { .. })));
    }
    #[test] fn rule_with_pick_true_is_ok() {
        let mut c = empty();
        c.apps.insert("real".to_string(), AppDef { cmd: "x".into(), args: vec![], label: None });
        let mut ext = BTreeMap::new();
        ext.insert(".md".to_string(), RouteTable { default: None, rules: vec![Rule { pick: true, ..Default::default() }], candidates: None });
        c.ext = ext;
        assert_eq!(validate(&c).len(), 0);
    }
    #[test] fn rule_without_pick_and_app_is_error() {
        let mut c = empty();
        let mut ext = BTreeMap::new();
        ext.insert(".md".to_string(), RouteTable { default: None, rules: vec![Rule::default()], candidates: None });
        c.ext = ext;
        assert!(validate(&c).iter().any(|e| matches!(e, ValidationError::RuleHasNoAction { .. })));
    }
    #[test] fn rule_with_unknown_app_is_error() {
        let mut c = empty();
        c.apps.insert("real".to_string(), AppDef { cmd: "x".into(), args: vec![], label: None });
        let mut ext = BTreeMap::new();
        ext.insert(".md".to_string(), RouteTable { default: None, rules: vec![Rule { app: Some("ghost".into()), ..Default::default() }], candidates: None });
        c.ext = ext;
        assert!(validate(&c).iter().any(|e| matches!(e, ValidationError::UnknownRuleApp { .. })));
    }
    #[test] fn invalid_modifier_is_error() {
        let mut c = empty();
        let mut ext = BTreeMap::new();
        ext.insert(".md".to_string(), RouteTable { default: None, rules: vec![Rule { modifier: Some("hyper".into()), pick: true, ..Default::default() }], candidates: None });
        c.ext = ext;
        assert!(validate(&c).iter().any(|e| matches!(e, ValidationError::InvalidModifier(_))));
    }
    #[test] fn invalid_ext_key_fails() {
        let mut c = empty();
        let mut ext = BTreeMap::new();
        ext.insert("a/b".to_string(), RouteTable { default: None, rules: vec![], candidates: None });
        c.ext = ext;
        assert!(validate(&c).iter().any(|e| matches!(e, ValidationError::InvalidExtKey(_))));
    }
    #[test] fn valid_ext_key_passes() {
        let mut c = empty();
        let mut ext = BTreeMap::new();
        ext.insert("md".to_string(), RouteTable { default: None, rules: vec![], candidates: None });
        ext.insert(".md".to_string(), RouteTable { default: None, rules: vec![], candidates: None });
        c.ext = ext;
        assert_eq!(validate(&c).len(), 0);
    }
    #[test] fn protocol_scheme_must_be_alphanumeric() {
        let mut c = empty();
        let mut proto = BTreeMap::new();
        proto.insert("ht tp".to_string(), RouteTable { default: None, rules: vec![], candidates: None });
        c.protocol = proto;
        assert!(validate(&c).iter().any(|e| matches!(e, ValidationError::InvalidProtocolScheme(_))));
    }
}
