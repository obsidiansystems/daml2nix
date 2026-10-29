use std::{
   collections::BTreeMap,
   io,
   path::Path,
   process::Command,
};

use serde::{
   Deserialize,
   Serialize,
};
use thiserror::Error;

pub const FILE_NAME: &str = "daml-nix.lock";

#[derive(Debug, Error)]
pub enum Error {
   #[error("failed to run nix")]
   Spawn(#[source] io::Error),
   #[error("nix {command} failed: {stderr}")]
   Nix { command: String, stderr: String },
   #[error("nix returned unreadable output")]
   Output(#[source] serde_json::Error),
   #[error("nix evaluated no entry for package {0:?}")]
   MissingPackage(String),
}

#[derive(Debug, Default, Serialize)]
pub struct Lock {
   pub version:      u32,
   pub builder:      String,
   pub packages:     BTreeMap<String, Package>,
   pub dependencies: BTreeMap<String, Dependency>,
}

#[derive(Debug, Serialize)]
pub struct Package {
   pub version:    String,
   pub source:     String,
   pub derivation: String,
   pub dar:        String,
}

#[derive(Debug, Serialize)]
pub struct Dependency {
   pub manifest: String,
   pub dar:      String,
   #[serde(skip_serializing_if = "std::ops::Not::not")]
   pub insecure: bool,
   pub audit:    Option<String>,
}

#[derive(Deserialize)]
struct Metadata {
   url: String,
}

#[derive(Deserialize)]
struct Evaluated {
   source:     String,
   derivation: String,
   dar:        String,
}

impl Lock {
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
      let metadata =
         serde_json::from_slice::<Metadata>(&run(&["flake", "metadata", "--json", flake])?)
            .map_err(Error::Output)?;

      let expression = format!(
         "({}) {{ flake = {}; generated = {}; }}",
         include_str!("evaluate.nix"),
         serde_json::to_string(&metadata.url).expect("string serializes"),
         serde_json::to_string(&nix_file.to_string_lossy()).expect("string serializes"),
      );

      let evaluated = serde_json::from_slice::<BTreeMap<String, Evaluated>>(&run(&[
         "eval",
         "--impure",
         "--json",
         "--expr",
         &expression,
      ])?)
      .map_err(Error::Output)?;

      let outputs = evaluated
         .values()
         .map(|package| format!("{}^out", package.derivation))
         .collect::<Vec<_>>();
      let mut build = vec!["build", "--no-link"];
      build.extend(outputs.iter().map(String::as_str));
      run(&build)?;

      self.builder = metadata.url;

      for (name, version) in versions {
         let package = evaluated
            .get(name)
            .ok_or_else(|| Error::MissingPackage(name.clone()))?;
         let source = run(&["hash", "path", &package.source])?;
         let dar = run(&["hash", "file", "--type", "sha256", "--base16", &package.dar])?;

         self.packages.insert(name.clone(), Package {
            version:    version.clone(),
            source:     String::from_utf8_lossy(&source).trim().to_owned(),
            derivation: package.derivation.clone(),
            dar:        format!("sha256:{}", String::from_utf8_lossy(&dar).trim()),
         });
      }

      Ok(())
   }
}

fn run(args: &[&str]) -> Result<Vec<u8>, Error> {
   let output = Command::new("nix")
      .args(["--extra-experimental-features", "nix-command flakes"])
      .args(args)
      .output()
      .map_err(Error::Spawn)?;

   if !output.status.success() {
      return Err(Error::Nix {
         command: args.first().copied().unwrap_or_default().to_owned(),
         stderr:  String::from_utf8_lossy(&output.stderr).trim().to_owned(),
      });
   }

   Ok(output.stdout)
}
