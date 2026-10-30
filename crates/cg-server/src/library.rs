//! Show projects, stored one JSON file per project in `<data>/projects/`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cg_core::Project;
use serde::Serialize;

pub struct Library {
    dir: PathBuf,
    projects: BTreeMap<String, Arc<Project>>,
    /// When each project was last saved (ms since 1970): its file's date.
    modified: BTreeMap<String, u64>,
}

/// Milliseconds since 1970 of a file's last change, or of now.
fn millis(t: std::time::SystemTime) -> u64 {
    t.duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// What lists and pickers need, without the templates.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub width: u32,
    pub height: u32,
    pub frame_rate: cg_core::FrameRate,
    pub templates: usize,
    pub rundown: usize,
    /// Template shown on the project's card (the first one).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    /// Last saved (ms since 1970), for the "recent" order of the projects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<u64>,
}

impl From<&Project> for ProjectSummary {
    fn from(p: &Project) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            description: p.description.clone(),
            width: p.width,
            height: p.height,
            frame_rate: p.frame_rate,
            templates: p.design_templates().count(),
            rundown: p.rundown.len(),
            cover: p.design_templates().next().map(|t| t.id.clone()),
            modified_at: None,
        }
    }
}

/// How many versions a project keeps.
const MAX_VERSIONS: usize = 50;

/// One entry of a project's history.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub id: String,
    /// Unix ms.
    pub at: u64,
    pub by: String,
    pub note: String,
    pub templates: usize,
}

#[derive(Serialize, serde::Deserialize)]
struct VersionFile {
    info: VersionInfo,
    project: Project,
}

#[derive(serde::Deserialize)]
struct VersionHead {
    info: VersionInfo,
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl Library {
    /// Loads every project. A pre-0.2 `project.json` becomes the first
    /// project; an empty library stays empty.
    pub fn load(data_dir: &Path) -> std::io::Result<Self> {
        let dir = data_dir.join("projects");
        std::fs::create_dir_all(&dir)?;
        let mut lib = Self {
            dir,
            projects: BTreeMap::new(),
            modified: BTreeMap::new(),
        };
        for entry in std::fs::read_dir(&lib.dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            match std::fs::read(&path).map(|b| serde_json::from_slice::<Project>(&b)) {
                Ok(Ok(mut p)) if valid_id(&p.id) => {
                    // Pre-zone projects: one zone per playout layer used.
                    if p.ensure_zones() {
                        tracing::info!(id = p.id, "project migrated to zones");
                        lib.save(p)?;
                    } else {
                        if let Ok(t) = std::fs::metadata(&path).and_then(|m| m.modified()) {
                            lib.modified.insert(p.id.clone(), millis(t));
                        }
                        lib.projects.insert(p.id.clone(), Arc::new(p));
                    }
                }
                Ok(Ok(_)) => tracing::error!(?path, "project with an invalid id ignored"),
                Ok(Err(e)) => tracing::error!(?path, "invalid project: {e}"),
                Err(e) => tracing::error!(?path, "cannot read project: {e}"),
            }
        }
        let legacy = data_dir.join("project.json");
        if legacy.exists() {
            match std::fs::read(&legacy).map(|b| serde_json::from_slice::<Project>(&b)) {
                Ok(Ok(mut p)) => {
                    p.ensure_zones();
                    if !valid_id(&p.id) {
                        p.id = cg_core::new_id();
                    }
                    if !lib.projects.contains_key(&p.id) {
                        tracing::info!(
                            id = p.id,
                            "migrating project.json into the project library"
                        );
                        lib.save(p)?;
                    }
                    let _ = std::fs::rename(&legacy, data_dir.join("project.json.migrated"));
                }
                _ => tracing::error!("cannot migrate project.json (left in place)"),
            }
        }
        Ok(lib)
    }

    pub fn get(&self, id: &str) -> Option<Arc<Project>> {
        self.projects.get(id).cloned()
    }

    pub fn list(&self) -> impl Iterator<Item = &Arc<Project>> {
        self.projects.values()
    }

    /// What lists show of a project, with when it was last saved.
    pub fn summary(&self, p: &Project) -> ProjectSummary {
        ProjectSummary {
            modified_at: self.modified.get(&p.id).copied(),
            ..ProjectSummary::from(p)
        }
    }

    pub fn first_id(&self) -> Option<String> {
        self.projects.keys().next().cloned()
    }

    pub fn save(&mut self, project: Project) -> std::io::Result<Arc<Project>> {
        Self::write_file(&self.dir, &project)?;
        let p = Arc::new(project);
        self.insert(p.clone());
        Ok(p)
    }

    /// Where the project files are.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Writes a project's file in `dir` (compact JSON; done without
    /// holding the library, see `AppState::save_project`).
    pub fn write_file(dir: &Path, project: &Project) -> std::io::Result<()> {
        if !valid_id(&project.id) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid project id",
            ));
        }
        let path = dir.join(format!("{}.json", project.id));
        let tmp = path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec(project).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(&tmp, &path)
    }

    /// Puts a project written with `write_file` in the library; the
    /// version it replaces.
    pub fn insert(&mut self, p: Arc<Project>) -> Option<Arc<Project>> {
        self.modified
            .insert(p.id.clone(), millis(std::time::SystemTime::now()));
        self.projects.insert(p.id.clone(), p)
    }

    fn versions_dir(&self, id: &str) -> PathBuf {
        self.dir.join("versions").join(id)
    }

    /// Keeps a copy of `project` in its history (the newest `MAX_VERSIONS`).
    pub fn save_version(
        &self,
        project: &Project,
        by: &str,
        note: &str,
    ) -> std::io::Result<VersionInfo> {
        let dir = self.versions_dir(&project.id);
        std::fs::create_dir_all(&dir)?;
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        let info = VersionInfo {
            id: format!("{at}-{}", cg_core::new_id()),
            at,
            by: by.to_string(),
            note: note.to_string(),
            templates: project.design_templates().count(),
        };
        let file = VersionFile {
            info: info.clone(),
            project: project.clone(),
        };
        std::fs::write(
            dir.join(format!("{}.json", info.id)),
            serde_json::to_vec(&file).map_err(std::io::Error::other)?,
        )?;
        let mut all = self.versions(&project.id);
        while all.len() > MAX_VERSIONS {
            let old = all.pop().unwrap();
            let _ = std::fs::remove_file(dir.join(format!("{}.json", old.id)));
        }
        Ok(info)
    }

    /// The history of a project, newest first.
    pub fn versions(&self, id: &str) -> Vec<VersionInfo> {
        let Ok(rd) = std::fs::read_dir(self.versions_dir(id)) else {
            return Vec::new();
        };
        let mut out: Vec<VersionInfo> = rd
            .filter_map(|e| e.ok())
            .filter_map(|e| std::fs::read(e.path()).ok())
            .filter_map(|b| serde_json::from_slice::<VersionHead>(&b).ok())
            .map(|h| h.info)
            .collect();
        out.sort_by_key(|v| std::cmp::Reverse(v.at));
        out
    }

    /// The project as it was in version `vid`.
    pub fn version(&self, id: &str, vid: &str) -> Option<Project> {
        if !valid_id(vid) {
            return None;
        }
        let b = std::fs::read(self.versions_dir(id).join(format!("{vid}.json"))).ok()?;
        serde_json::from_slice::<VersionFile>(&b)
            .ok()
            .map(|f| f.project)
    }

    pub fn delete(&mut self, id: &str) -> std::io::Result<bool> {
        if self.projects.remove(id).is_none() {
            return Ok(false);
        }
        self.modified.remove(id);
        let path = self.dir.join(format!("{id}.json"));
        match std::fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_project_json_and_roundtrips() {
        let dir = std::env::temp_dir().join(format!("cg-lib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut p = cg_core::sample::demo_project();
        p.id = "my-show".into();
        std::fs::write(dir.join("project.json"), serde_json::to_vec(&p).unwrap()).unwrap();

        let mut lib = Library::load(&dir).unwrap();
        assert!(lib.get("my-show").is_some());
        assert!(!dir.join("project.json").exists());
        let mut copy = (*lib.get("my-show").unwrap()).clone();
        copy.id = "copy".into();
        lib.save(copy).unwrap();
        assert_eq!(Library::load(&dir).unwrap().list().count(), 2);
        assert!(lib.delete("copy").unwrap());
        assert_eq!(Library::load(&dir).unwrap().list().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn summaries_carry_the_last_save() {
        let dir = std::env::temp_dir().join(format!("cg-lib-mtime-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut lib = Library::load(&dir).unwrap();
        let mut p = cg_core::sample::demo_project();
        p.id = "dated".into();
        lib.save(p).unwrap();
        let at = lib.summary(&lib.get("dated").unwrap()).modified_at.unwrap();
        assert!(at > 1_600_000_000_000);
        // After a restart: the file's date.
        let again = Library::load(&dir).unwrap();
        let back = again
            .summary(&again.get("dated").unwrap())
            .modified_at
            .unwrap();
        assert!(back.abs_diff(at) < 5_000);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
