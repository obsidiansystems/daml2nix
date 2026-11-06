use std::{
   collections::BTreeMap,
   fs,
   io,
   path::{
      Path,
      PathBuf,
   },
};

use serde::{
   Deserialize,
   Serialize,
};
use thiserror::Error;

use crate::store;

pub const FILE_NAME: &str = "daml-nix.lock";

#[derive(Debug, Error)]
pub enum Error {
   #[error("failed to read {path}")]
   Read {
      path:   PathBuf,
      #[source]
      source: io::Error,
   },
   #[error("{path} is not a valid lock")]
   Parse {
      path:   PathBuf,
      #[source]
      source: serde_json::Error,
   },
   #[error("failed to record build outputs")]
   Store(#[source] store::Error),
   #[error("nix evaluated no entry for package {0:?}")]
   MissingPackage(String),
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Lock {
   pub version:      u32,
   pub builder:      String,
   pub packages:     BTreeMap<String, Package>,
   pub dependencies: BTreeMap<String, Dependency>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Package {
   pub version:    String,
   pub source:     String,
   pub derivation: String,
   pub dar:        String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Dependency {
   pub manifest: String,
   pub dar:      String,
   #[serde(default, skip_serializing_if = "std::ops::Not::not")]
   pub insecure: bool,
   pub audit:    Option<String>,
}

impl Lock {
   pub fn read(path: &Path) -> Result<Self, Error> {
      let json = fs::read(path).map_err(|source| {
         Error::Read {
            path: path.to_owned(),
            source,
         }
      })?;

      serde_json::from_slice(&json).map_err(|source| {
         Error::Parse {
            path: path.to_owned(),
            source,
         }
      })
   }

   pub fn to_json(&self) -> String {
      let mut json =
         serde_json::to_string_pretty(self).expect("lock serializes, every key is a string");
      json.push('\n');
      json
   }

   pub fn fill(
      &mut self,
      nix_file: &Path,
      flake: &str,
      versions: &[(String, String)],
   ) -> Result<(), Error> {
      let url = store::locked_url(flake).map_err(Error::Store)?;
      let evaluated = store::evaluate(&url, nix_file).map_err(Error::Store)?;

      let derivations = evaluated
         .values()
         .map(|package| package.derivation.as_str())
         .collect::<Vec<_>>();
      store::build(&derivations, None).map_err(Error::Store)?;

      self.builder = url;

      for (name, version) in versions {
         let package = evaluated
            .get(name)
            .ok_or_else(|| Error::MissingPackage(name.clone()))?;

         self.packages.insert(name.clone(), Package {
            version:    version.clone(),
            source:     store::hash_path(&package.source).map_err(Error::Store)?,
            derivation: package.derivation.clone(),
            dar:        store::hash_file(&package.dar).map_err(Error::Store)?,
         });
      }

      Ok(())
   }
}
