//! Frame-template store (CHANGELOG §2 build note): templates are user data,
//! persisted in `templates.json` alongside the app settings, independent of
//! any batch. Built-ins are code-defined, cannot be edited or deleted, and are
//! always present in the returned list.

use crate::types::{builtin_templates, FrameTemplate};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct TemplateStore {
    path: PathBuf,
    user: Mutex<Vec<FrameTemplate>>,
}

impl TemplateStore {
    pub fn new(data_dir: &Path) -> Self {
        let path = data_dir.join("templates.json");
        let user: Vec<FrameTemplate> = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        TemplateStore {
            path,
            user: Mutex::new(user),
        }
    }

    fn persist(&self, user: &[FrameTemplate]) {
        if let Ok(b) = serde_json::to_vec_pretty(user) {
            let tmp = self.path.with_extension("tmp");
            if std::fs::write(&tmp, b).is_ok() {
                let _ = std::fs::rename(&tmp, &self.path);
            }
        }
    }

    /// Built-ins first, then user templates.
    pub fn list(&self) -> Vec<FrameTemplate> {
        let mut all = builtin_templates();
        all.extend(self.user.lock().unwrap().iter().cloned());
        all
    }

    pub fn get(&self, id: &str) -> FrameTemplate {
        self.list()
            .into_iter()
            .find(|t| t.id == id)
            .unwrap_or_default() // unknown/deleted id → Classic
    }

    /// Insert or update a user template. Built-in ids are refused.
    pub fn save(&self, mut tpl: FrameTemplate) -> Result<FrameTemplate, String> {
        if builtin_templates().iter().any(|b| b.id == tpl.id) {
            return Err("built-in templates can only be duplicated".into());
        }
        tpl.builtin = false;
        if tpl.id.trim().is_empty() {
            tpl.id = format!(
                "custom-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or_default()
            );
        }
        if tpl.name.trim().is_empty() {
            tpl.name = "Untitled template".into();
        }
        let mut user = self.user.lock().unwrap();
        if let Some(existing) = user.iter_mut().find(|t| t.id == tpl.id) {
            *existing = tpl.clone();
        } else {
            user.push(tpl.clone());
        }
        self.persist(&user);
        Ok(tpl)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        if builtin_templates().iter().any(|b| b.id == id) {
            return Err("built-in templates cannot be deleted".into());
        }
        let mut user = self.user.lock().unwrap();
        user.retain(|t| t.id != id);
        self.persist(&user);
        Ok(())
    }
}
