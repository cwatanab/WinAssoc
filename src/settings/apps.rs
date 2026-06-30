//! アプリ定義の CRUD ハンドラ

use crate::settings::data::{app_entry_to_def, build_app_entries, AppEntry};
use slint::{Model, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub struct AppsState {
    pub model: Rc<VecModel<AppEntry>>,
    pub entries: Rc<RefCell<HashMap<String, crate::config::AppDef>>>,
}

impl AppsState {
    pub fn from_apps(apps: &std::collections::BTreeMap<String, crate::config::AppDef>) -> Self {
        let entries: HashMap<_, _> = apps.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let model = Rc::new(VecModel::from(build_app_entries(apps)));
        Self { model, entries: Rc::new(RefCell::new(entries)) }
    }

    pub fn add(&self, name: String) {
        self.model.push(AppEntry { name: name.clone(), ..Default::default() });
        self.entries.borrow_mut().insert(name, crate::config::AppDef::default());
    }

    pub fn remove(&self, idx: usize) -> Option<String> {
        let removed = self.model.row_data(idx)?;
        let name = removed.name.clone();
        self.model.remove(idx);
        self.entries.borrow_mut().remove(&name);
        Some(name)
    }

    pub fn update(&self, idx: usize, entry: AppEntry) {
        self.model.set_row_data(idx, entry.clone());
        let def = app_entry_to_def(&entry);
        self.entries.borrow_mut().insert(entry.name.clone(), def);
    }

    pub fn to_config(&self) -> std::collections::BTreeMap<String, crate::config::AppDef> {
        self.entries.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppDef;

    #[test] fn add_pushes_to_model_and_entries() {
        let state = AppsState::from_apps(&std::collections::BTreeMap::new());
        state.add("newapp".to_string());
        assert_eq!(state.model.row_count(), 1);
        assert!(state.entries.borrow().contains_key("newapp"));
    }
    #[test] fn remove_returns_name_and_removes() {
        let mut apps = std::collections::BTreeMap::new();
        apps.insert("a".to_string(), AppDef { cmd: "x".into(), args: vec![], label: None, ..Default::default() });
        let state = AppsState::from_apps(&apps);
        assert_eq!(state.remove(0), Some("a".to_string()));
        assert_eq!(state.model.row_count(), 0);
        assert!(state.entries.borrow().is_empty());
    }
    #[test] fn update_replaces_entry() {
        let mut apps = std::collections::BTreeMap::new();
        apps.insert("x".to_string(), AppDef { cmd: "old".into(), args: vec![], label: None, ..Default::default() });
        let state = AppsState::from_apps(&apps);
        state.update(0, AppEntry { name: "x".into(), cmd: "new".into(), ..Default::default() });
        assert_eq!(state.entries.borrow().get("x").unwrap().cmd, "new");
    }
}
