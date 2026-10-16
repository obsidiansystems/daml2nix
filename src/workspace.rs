use std::{
   collections::BTreeSet,
   path::{
      Path,
      PathBuf,
   },
};

use serde::Deserialize;

use crate::project::{
   Error,
   Pins,
   Project,
   load,
};

#[derive(Debug)]
pub struct Workspace {
   pub packages: Vec<Project>,
}

#[derive(Deserialize)]
struct RawWorkspace {
   #[serde(default)]
   packages: Vec<PathBuf>,
   #[serde(default)]
   projects: Vec<PathBuf>,
   #[serde(flatten)]
   pins:     Pins,
}

impl Workspace {
   pub fn read(path: &Path) -> Result<Self, Error> {
      let mut workspace = Self {
         packages: Vec::new(),
      };
      let mut visited = BTreeSet::new();

      workspace.collect(path, &mut visited)?;

      Ok(workspace)
   }

   pub fn producer(&self, dar: &Path) -> Option<&Project> {
      self.packages.iter().find(|package| package.dist() == dar)
   }

   fn collect(&mut self, path: &Path, visited: &mut BTreeSet<PathBuf>) -> Result<(), Error> {
      let (raw, root) = load::<RawWorkspace>(path)?;

      if !visited.insert(root.clone()) {
         return Ok(());
      }

      for package in &raw.packages {
         let project = Project::read_inherited(
            &root.join(package).join("daml.yaml"),
            Some((&raw.pins, &root)),
         )?;

         if self.packages.iter().all(|known| known.root != project.root) {
            self.packages.push(project);
         }
      }

      for project in &raw.projects {
         self.collect(&root.join(project).join("multi-package.yaml"), visited)?;
      }

      Ok(())
   }
}
