use crate::domain::Store;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelProfile {
    pub name: String,
    pub default_agent: String,
    pub default_model: String,
    pub agents: [Option<String>; 4],
    pub models: [String; 4],
}

impl Store {
    fn capture_model_profile(&self, name: String) -> ModelProfile {
        ModelProfile {
            name,
            default_agent: self.research_config().default_agent.clone(),
            default_model: self.default_model().clone(),
            agents: self.research_config().agents.clone(),
            models: self.research_config().models.clone(),
        }
    }

    pub fn validate_model_profile_name(&self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() { return Err("Enter a profile name.".into()); }
        if self.model_profiles().iter().any(|p| p.name.eq_ignore_ascii_case(name)) {
            return Err("A profile with this name already exists.".into());
        }
        Ok(())
    }

    pub fn create_model_profile(&mut self, name: &str) -> Result<(), String> {
        self.validate_model_profile_name(name)?;
        let name = name.trim();
        self.push_model_profile(self.capture_model_profile(name.to_owned()));
        self.set_active_model_profile(Some(name.to_owned()));
        Ok(())
    }

    pub fn model_profile_changed(&self, profile: &ModelProfile) -> bool {
        self.capture_model_profile(profile.name.clone()) != *profile
    }

    pub fn apply_model_profile(&mut self, name: &str) {
        let Some(profile) = self.model_profiles().iter().find(|p| p.name == name).cloned() else { return; };
        self.research_config_mut().default_agent = profile.default_agent.clone();
        self.set_default_model(profile.default_model.clone());
        self.research_config_mut().agents = profile.agents.clone();
        self.research_config_mut().models = profile.models.clone();
        self.set_active_model_profile(Some(name.to_owned()));
    }

    pub fn update_model_profile(&mut self, name: &str) {
        let replacement = self.capture_model_profile(name.to_owned());
        if let Some(profile) = self.model_profile_mut(name) {
            *profile = replacement;
        }
    }

    pub fn delete_model_profile(&mut self, name: &str) {
        self.retain_model_profiles(|p| p.name != name);
        if self.active_model_profile().as_deref() == Some(name) {
            self.set_active_model_profile(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_restore_global_and_stage_overrides_without_changing_runtime_settings() {
        let mut store = Store::empty();
        store.set_default_model("provider/global".into());
        store.research_config_mut().agents[1] = Some("Codex".into());
        store.research_config_mut().models[1] = "codex-model".into();
        store.create_model_profile(" Mixed ").unwrap();
        let original = store.model_profiles()[0].clone();
        store.research_config_mut().default_agent = "Codex".into();
        store.set_default_model("another-model".into());
        store.research_config_mut().agents = Default::default();
        store.research_config_mut().models = Default::default();
        store.create_model_profile("Fast").unwrap();
        store.research_config_mut().ego_path = "custom-browser".into();
        store.research_config_mut().codex_path = "custom-codex".into();
        store.set_output_language(crate::research::OutputLanguage::SimplifiedChinese);
        store.apply_model_profile("Mixed");
        assert!(!store.model_profile_changed(&original));
        assert_eq!(store.research_model(crate::research::ResearchKind::ALL[0]), "provider/global");
        assert_eq!(store.research_model(crate::research::ResearchKind::ALL[1]), "codex-model");
        assert_eq!(store.research_config().ego_path, "custom-browser");
        assert_eq!(store.research_config().codex_path, "custom-codex");
        assert_eq!(
            store.output_language(),
            crate::research::OutputLanguage::SimplifiedChinese
        );
        store.set_default_model("provider/updated".into());
        assert!(store.model_profile_changed(&original));
        store.update_model_profile("Mixed");
        store.apply_model_profile("Fast");
        assert_eq!(store.default_model(), "another-model");
        store.apply_model_profile("Mixed");
        assert_eq!(store.default_model(), "provider/updated");
        store.delete_model_profile("Mixed");
        assert!(store.active_model_profile().is_none());
        assert_eq!(store.default_model(), "provider/updated");
        assert_eq!(store.model_profiles().len(), 1);
    }

    #[test]
    fn profiles_validate_names_and_load_legacy_workspaces() {
        let mut store = Store::empty();
        assert!(store.create_model_profile("  ").is_err());
        store.create_model_profile("Default").unwrap();
        assert!(store.create_model_profile(" default ").is_err());
        let value = serde_json::to_value(&store).unwrap();
        assert_eq!(serde_json::from_value::<Store>(value.clone()).unwrap(), store);
        let mut legacy = value;
        legacy.as_object_mut().unwrap().remove("model_profiles");
        legacy.as_object_mut().unwrap().remove("active_model_profile");
        let restored: Store = serde_json::from_value(legacy).unwrap();
        assert!(restored.model_profiles().is_empty());
        assert!(restored.active_model_profile().is_none());
    }

    #[test]
    fn profiles_survive_sqlite_restart() {
        let root = std::env::temp_dir().join(crate::opencode::new_session_id());
        std::fs::create_dir_all(&root).unwrap();
        let storage = crate::persistence::Storage::new(root.join("workspace.sqlite3"));
        let mut store = Store::empty();
        store.set_default_model("provider/model".into());
        store.create_model_profile("Daily").unwrap();
        storage.save(&store).unwrap();
        assert_eq!(storage.load().unwrap().unwrap(), store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
