use std::{
   collections::BTreeMap,
   io,
   path::{
      Path,
      PathBuf,
   },
   process::{
      Command,
      ExitStatus,
   },
};

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
   #[error("failed to run nix")]
   Spawn(#[source] io::Error),
   #[error("nix {command} failed: {stderr}")]
   Nix { command: String, stderr: String },
   #[error("nix returned unreadable output")]
   Output(#[source] serde_json::Error),
}

#[derive(Debug, Deserialize)]
pub struct Evaluated {
   pub source:     String,
   pub derivation: String,
   pub dar:        PathBuf,
   pub directory:  PathBuf,
}

#[derive(Deserialize)]
struct Metadata {
   url: String,
}

pub fn locked_url(flake: &str) -> Result<String, Error> {
   let metadata =
      serde_json::from_slice::<Metadata>(&run(&["flake", "metadata", "--json", flake])?)
         .map_err(Error::Output)?;

   Ok(metadata.url)
}

pub fn expression(flake_url: &str, nix_file: &Path) -> String {
   format!(
      "({}) {{ flake = {}; generated = {}; }}",
      include_str!("evaluate.nix"),
      serde_json::to_string(flake_url).expect("string serializes"),
      serde_json::to_string(&nix_file.to_string_lossy()).expect("string serializes"),
   )
}

pub fn evaluate(flake_url: &str, nix_file: &Path) -> Result<BTreeMap<String, Evaluated>, Error> {
   let summary = format!("({}).summary", expression(flake_url, nix_file));

   serde_json::from_slice(&run(&["eval", "--impure", "--json", "--expr", &summary])?)
      .map_err(Error::Output)
}

pub fn build(derivations: &[&str], out_link: Option<&Path>) -> Result<(), Error> {
   let outputs = derivations
      .iter()
      .map(|derivation| format!("{derivation}^out"))
      .collect::<Vec<_>>();

   let out_link = out_link.map(|path| path.to_string_lossy().into_owned());
   let mut args = vec!["build"];

   match &out_link {
      Some(path) => args.extend(["--out-link", path]),
      None => args.push("--no-link"),
   }

   args.extend(outputs.iter().map(String::as_str));
   run(&args).map(drop)
}

pub fn rebuild(derivation: &str) -> Result<(), Error> {
   run(&[
      "build",
      "--no-link",
      "--rebuild",
      &format!("{derivation}^out"),
   ])
   .map(drop)
}

pub fn hash_path(path: &str) -> Result<String, Error> {
   let hash = run(&["hash", "path", path])?;
   Ok(String::from_utf8_lossy(&hash).trim().to_owned())
}

pub fn hash_file(path: &Path) -> Result<String, Error> {
   let hash = run(&[
      "hash",
      "file",
      "--type",
      "sha256",
      "--base16",
      &path.to_string_lossy(),
   ])?;
   Ok(format!("sha256:{}", String::from_utf8_lossy(&hash).trim()))
}

pub fn develop(installable: &str) -> Result<ExitStatus, Error> {
   nix()
      .args(["develop", "--impure", "--expr", installable])
      .status()
      .map_err(Error::Spawn)
}

fn nix() -> Command {
   let mut command = Command::new("nix");
   command.args(["--extra-experimental-features", "nix-command flakes"]);
   command
}

fn run(args: &[&str]) -> Result<Vec<u8>, Error> {
   let output = nix().args(args).output().map_err(Error::Spawn)?;

   if !output.status.success() {
      return Err(Error::Nix {
         command: args.first().copied().unwrap_or_default().to_owned(),
         stderr:  String::from_utf8_lossy(&output.stderr).trim().to_owned(),
      });
   }

   Ok(output.stdout)
}
