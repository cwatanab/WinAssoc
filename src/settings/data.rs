//! Slint 表示用 struct ↔ Config 内部 struct の変換

use crate::config::{RouteTable, Rule};

#[derive(Debug, Clone, Default)]
pub struct RuleEntry {
    pub glob: String,
    pub host: String,
    pub url: String,
    pub modifier: String,
    pub app: String,
    pub pick: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RouteEntry {
    pub key: String,
    pub default: String,
    pub candidates: Vec<String>,
    pub rules: Vec<RuleEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct AppEntry {
    pub name: String,
    pub cmd: String,
    pub args: Vec<String>,
    pub label: String,
}

pub fn build_route_entries(routes: &std::collections::BTreeMap<String, RouteTable>) -> Vec<RouteEntry> {
    routes.iter().map(|(key, table)| RouteEntry {
        key: key.clone(),
        default: table.default.clone().unwrap_or_default(),
        candidates: table.candidates.clone().unwrap_or_default(),
        rules: table.rules.iter().map(rule_to_rule_entry).collect(),
    }).collect()
}

pub fn route_entry_to_table(entry: &RouteEntry) -> RouteTable {
    RouteTable {
        default: if entry.default.is_empty() { None } else { Some(entry.default.clone()) },
        candidates: if entry.candidates.is_empty() { None } else { Some(entry.candidates.clone()) },
        rules: entry.rules.iter().map(rule_entry_to_rule).collect(),
    }
}

pub fn rule_entry_to_rule(entry: &RuleEntry) -> Rule {
    Rule {
        glob: optional(entry.glob.clone()),
        host: optional(entry.host.clone()),
        url: optional(entry.url.clone()),
        modifier: optional(entry.modifier.clone()),
        app: optional(entry.app.clone()),
        pick: entry.pick,
    }
}

pub fn rule_to_rule_entry(rule: &Rule) -> RuleEntry {
    RuleEntry {
        glob: rule.glob.clone().unwrap_or_default(),
        host: rule.host.clone().unwrap_or_default(),
        url: rule.url.clone().unwrap_or_default(),
        modifier: rule.modifier.clone().unwrap_or_default(),
        app: rule.app.clone().unwrap_or_default(),
        pick: rule.pick,
    }
}

pub fn build_app_entries(apps: &std::collections::BTreeMap<String, crate::config::AppDef>) -> Vec<AppEntry> {
    apps.iter().map(|(name, def)| AppEntry {
        name: name.clone(),
        cmd: def.cmd.clone(),
        args: def.args.clone(),
        label: def.label.clone().unwrap_or_default(),
    }).collect()
}

pub fn app_entry_to_def(entry: &AppEntry) -> crate::config::AppDef {
    crate::config::AppDef {
        cmd: entry.cmd.clone(),
        args: entry.args.clone(),
        label: if entry.label.is_empty() { None } else { Some(entry.label.clone()) },
    }
}

fn optional(s: String) -> Option<String> { if s.is_empty() { None } else { Some(s) } }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppDef;

    #[test] fn app_entry_round_trip() {
        let entry = AppEntry { name: "test".into(), cmd: "C:\\bin\\app.exe".into(), args: vec!["--foo".into(), "{target}".into()], label: "Label".into() };
        let def = app_entry_to_def(&entry);
        assert_eq!(def.cmd, "C:\\bin\\app.exe");
        assert_eq!(def.args, vec!["--foo", "{target}"]);
        assert_eq!(def.label, Some("Label".to_string()));
    }
    #[test] fn empty_label_becomes_none() {
        let entry = AppEntry { label: "".into(), ..Default::default() };
        assert_eq!(app_entry_to_def(&entry).label, None);
    }
    #[test] fn rule_entry_to_rule_with_pick() {
        let entry = RuleEntry { pick: true, modifier: "shift".into(), ..Default::default() };
        let rule = rule_entry_to_rule(&entry);
        assert!(rule.pick);
        assert_eq!(rule.modifier, Some("shift".to_string()));
        assert_eq!(rule.app, None);
    }
    #[test] fn rule_entry_to_rule_with_app() {
        let entry = RuleEntry { app: "vscode".into(), glob: "*.md".into(), ..Default::default() };
        let rule = rule_entry_to_rule(&entry);
        assert_eq!(rule.app, Some("vscode".to_string()));
        assert_eq!(rule.glob, Some("*.md".to_string()));
        assert!(!rule.pick);
    }
    #[test] fn rule_to_entry_preserves_all_fields() {
        let rule = Rule { glob: Some("*.svg".into()), host: None, url: None, modifier: Some("shift".into()), app: Some("msedge".into()), pick: false };
        let entry = rule_to_rule_entry(&rule);
        assert_eq!(entry.glob, "*.svg");
        assert_eq!(entry.modifier, "shift");
        assert_eq!(entry.app, "msedge");
        assert!(!entry.pick);
    }
    #[test] fn build_route_entries_includes_rules_and_candidates() {
        let mut routes = std::collections::BTreeMap::new();
        routes.insert(".md".to_string(), RouteTable {
            default: Some("vscode".to_string()),
            rules: vec![Rule { pick: true, modifier: Some("shift".into()), ..Default::default() }],
            candidates: Some(vec!["vscode".to_string(), "zed".to_string()]),
        });
        let entries = build_route_entries(&routes);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, ".md");
        assert_eq!(entries[0].default, "vscode");
        assert_eq!(entries[0].candidates, vec!["vscode", "zed"]);
        assert_eq!(entries[0].rules.len(), 1);
        assert!(entries[0].rules[0].pick);
    }
    #[test] fn route_entry_to_table_preserves_candidates_default_and_rules() {
        let entry = RouteEntry {
            key: ".txt".into(), default: "hidemaru".into(),
            candidates: vec!["hidemaru".into(), "vscode".into()],
            rules: vec![RuleEntry { modifier: "shift".into(), pick: true, ..Default::default() }],
        };
        let table = route_entry_to_table(&entry);
        assert_eq!(table.default, Some("hidemaru".to_string()));
        assert_eq!(table.candidates, Some(vec!["hidemaru".to_string(), "vscode".to_string()]));
        assert_eq!(table.rules.len(), 1);
        assert!(table.rules[0].pick);
    }
    #[test] fn build_app_entries_includes_label() {
        let mut apps = std::collections::BTreeMap::new();
        apps.insert("x".to_string(), AppDef { cmd: "c.exe".into(), args: vec![], label: Some("X".into()) });
        apps.insert("y".to_string(), AppDef { cmd: "d.exe".into(), args: vec!["{target}".into()], label: None });
        let entries = build_app_entries(&apps);
        assert_eq!(entries.len(), 2);
        let x = entries.iter().find(|e| e.name == "x").unwrap();
        assert_eq!(x.label, "X");
        let y = entries.iter().find(|e| e.name == "y").unwrap();
        assert_eq!(y.label, "");
        assert_eq!(y.args, vec!["{target}"]);
    }
}