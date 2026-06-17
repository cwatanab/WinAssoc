//! プロトコル (URL スキーム) ルーティング テーブルの CRUD

use crate::settings::data::{build_route_entries, route_entry_to_table, rule_to_rule_entry, RouteEntry, RuleEntry};
use slint::{Model, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub struct ProtocolState {
    pub route_model: Rc<VecModel<RouteEntry>>,
    pub candidates_models: RefCell<HashMap<String, Rc<VecModel<String>>>>,
    pub rules_models: RefCell<HashMap<String, Rc<VecModel<RuleEntry>>>>,
}

impl ProtocolState {
    pub fn from_protocol(protocol: &std::collections::BTreeMap<String, crate::config::RouteTable>) -> Self {
        let entries = build_route_entries(protocol);
        let route_model = Rc::new(VecModel::from(entries));
        let mut candidates_models = HashMap::new();
        let mut rules_models = HashMap::new();
        for (key, table) in protocol {
            candidates_models.insert(key.clone(), Rc::new(VecModel::from(table.candidates.clone().unwrap_or_default())));
            rules_models.insert(key.clone(), Rc::new(VecModel::from(
                table.rules.iter().map(rule_to_rule_entry).collect::<Vec<_>>(),
            )));
        }
        Self { route_model, candidates_models: RefCell::new(candidates_models), rules_models: RefCell::new(rules_models) }
    }

    pub fn add_route(&self, key: String) {
        self.route_model.push(RouteEntry { key: key.clone(), ..Default::default() });
        self.candidates_models.borrow_mut().insert(key.clone(), Rc::new(VecModel::from(vec![])));
        self.rules_models.borrow_mut().insert(key, Rc::new(VecModel::from(vec![])));
    }

    pub fn remove_route(&self, idx: usize) -> Option<String> {
        let removed = self.route_model.row_data(idx)?;
        let key = removed.key.clone();
        self.route_model.remove(idx);
        self.candidates_models.borrow_mut().remove(&key);
        self.rules_models.borrow_mut().remove(&key);
        Some(key)
    }

    pub fn add_rule(&self, key: &str) {
        {
            if let Some(model) = self.rules_models.borrow_mut().get_mut(key) {
                model.push(RuleEntry::default());
            }
        }
        self.sync_route_entry(key);
    }

    pub fn remove_rule(&self, key: &str, idx: usize) {
        {
            if let Some(model) = self.rules_models.borrow_mut().get_mut(key) {
                if idx < model.row_count() { model.remove(idx); }
            }
        }
        self.sync_route_entry(key);
    }

    pub fn move_rule(&self, key: &str, idx: usize, delta: i32) {
        let new_idx = (idx as i32 + delta) as usize;
        {
            if let Some(model) = self.rules_models.borrow_mut().get_mut(key) {
                if new_idx < model.row_count() {
                    let row = model.row_data(idx).unwrap();
                    model.remove(idx);
                    model.insert(new_idx, row);
                }
            }
        }
        self.sync_route_entry(key);
    }

    pub fn add_candidate(&self, key: &str, candidate: String) {
        {
            if let Some(model) = self.candidates_models.borrow_mut().get_mut(key) {
                if !model.iter().any(|c| c == candidate) {
                    model.push(candidate);
                }
            }
        }
        self.sync_route_entry(key);
    }

    pub fn remove_candidate(&self, key: &str, candidate: String) {
        {
            if let Some(model) = self.candidates_models.borrow_mut().get_mut(key) {
                for i in 0..model.row_count() {
                    if model.row_data(i).unwrap() == candidate {
                        model.remove(i);
                        break;
                    }
                }
            }
        }
        self.sync_route_entry(key);
    }

    pub fn update_rule(&self, key: &str, idx: usize, entry: RuleEntry) {
        {
            if let Some(model) = self.rules_models.borrow_mut().get_mut(key) {
                if idx < model.row_count() {
                    model.set_row_data(idx, entry);
                }
            }
        }
        self.sync_route_entry(key);
    }

    pub fn update_default(&self, key: &str, default: String) {
        self.update_route_field(key, |e| e.default = default.clone());
    }

    fn sync_route_entry(&self, key: &str) {
        let candidates = self.candidates_models.borrow().get(key).map(|m| {
            (0..m.row_count()).map(|i| m.row_data(i).unwrap()).collect::<Vec<_>>()
        }).unwrap_or_default();
        let rules = self.rules_models.borrow().get(key).map(|m| {
            (0..m.row_count()).map(|i| m.row_data(i).unwrap()).collect::<Vec<_>>()
        }).unwrap_or_default();
        let updated = RouteEntry { key: key.to_string(), default: String::new(), candidates, rules, ..Default::default() };
        for i in 0..self.route_model.row_count() {
            if let Some(e) = self.route_model.row_data(i) {
                if e.key == key {
                    let mut u = updated.clone();
                    u.default = e.default.clone();
                    u.default_error = e.default_error.clone();
                    u.candidates_error = e.candidates_error.clone();
                    self.route_model.set_row_data(i, u);
                    return;
                }
            }
        }
    }

    fn update_route_field<F: FnOnce(&mut RouteEntry)>(&self, key: &str, f: F) {
        for i in 0..self.route_model.row_count() {
            if let Some(mut e) = self.route_model.row_data(i) {
                if e.key == key { f(&mut e); self.route_model.set_row_data(i, e); return; }
            }
        }
    }

    pub fn to_config(&self) -> std::collections::BTreeMap<String, crate::config::RouteTable> {
        let mut out = std::collections::BTreeMap::new();
        for i in 0..self.route_model.row_count() {
            let entry = self.route_model.row_data(i).unwrap();
            out.insert(entry.key.clone(), route_entry_to_table(&entry));
        }
        out
    }
}
