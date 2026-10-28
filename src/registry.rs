use std::{
   collections::BTreeMap,
   io,
   process::Command,
};

use thiserror::Error;
use serde::{
   Deserialize,
   de::DeserializeOwned,
};

use crate::oci::Reference;

#[derive(Debug, Error)]
pub enum Error {
   #[error("failed to run oras")]
   Spawn(#[source] io::Error),
   #[error("oras could not fetch {target}: {stderr}")]
   Fetch { target: String, stderr: String },
   #[error("oras returned an unreadable manifest for {target}")]
   Manifest {
      target: String,
      #[source]
      source: serde_json::Error,
   },
   #[error("{target} has no .dar layer")]
   MissingDar { target: String },
   #[error("{target} has main package {found}, daml.yaml expects {expected}")]
   PackageId {
      target:   String,
      found:    String,
      expected: String,
   },
}

pub struct Pinned {
   pub manifest_digest: String,
   pub dar_digest:      String,
}

#[derive(Deserialize)]
struct Descriptor {
   digest: String,
}

#[derive(Deserialize)]
struct Manifest {
   layers: Vec<Layer>,
}

#[derive(Deserialize)]
struct Layer {
   digest:      String,
   #[serde(default)]
   annotations: BTreeMap<String, String>,
}

pub fn pin(
   reference: &Reference,
   insecure: bool,
   main_package_id: Option<&str>,
) -> Result<Pinned, Error> {
   let target = match reference.tag() {
      Some(tag) => format!("{}/{}:{tag}", reference.registry(), reference.repository()),
      None => format!("{}/{}", reference.registry(), reference.repository()),
   };

   let manifest_digest = match reference.digest() {
      Some(digest) => digest.to_owned(),
      None => fetch::<Descriptor>(&target, insecure, true)?.digest,
   };

   let pinned_target = format!(
      "{}/{}@{manifest_digest}",
      reference.registry(),
      reference.repository()
   );
   let manifest = fetch::<Manifest>(&pinned_target, insecure, false)?;

   let dar = manifest
      .layers
      .into_iter()
      .find(|layer| {
         layer
            .annotations
            .get("org.opencontainers.image.title")
            .is_some_and(|title| title.ends_with(".dar"))
      })
      .ok_or_else(|| {
         Error::MissingDar {
            target: pinned_target.clone(),
         }
      })?;

   if let Some(expected) = main_package_id {
      let found = dar.annotations.get("network.canton.dpm.main-package-id");

      if found.map(String::as_str) != Some(expected) {
         return Err(Error::PackageId {
            target:   pinned_target,
            found:    found.cloned().unwrap_or_else(|| "none".to_owned()),
            expected: expected.to_owned(),
         });
      }
   }

   Ok(Pinned {
      manifest_digest,
      dar_digest: dar.digest,
   })
}

fn fetch<T: DeserializeOwned>(target: &str, insecure: bool, descriptor: bool) -> Result<T, Error> {
   let mut command = Command::new("oras");
   command.args(["manifest", "fetch"]);

   if insecure {
      command.arg("--plain-http");
   }

   if descriptor {
      command.arg("--descriptor");
   }

   let output = command.arg(target).output().map_err(Error::Spawn)?;

   if !output.status.success() {
      return Err(Error::Fetch {
         target: target.to_owned(),
         stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
      });
   }

   serde_json::from_slice(&output.stdout).map_err(|source| {
      Error::Manifest {
         target: target.to_owned(),
         source,
      }
   })
}
