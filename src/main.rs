use std::{
   env,
   error::Error,
   fs,
   path::{
      PathBuf,
      absolute,
   },
   process::ExitCode,
};

use daml2nix::{
   lock::{
      self,
      Lock,
   },
   nix,
   project::{
      Project,
      normalize,
   },
   workspace::Workspace,
};

const USAGE: &str =
   "usage: daml2nix generate [--flake <ref>] <daml.yaml | multi-package.yaml> <output.nix>";

const DEFAULT_FLAKE: &str = "github:obsidiansystems/daml2nix";

fn main() -> ExitCode {
   let args = env::args().skip(1).collect::<Vec<_>>();

   let (flake, rest) = match args.as_slice() {
      [command, flag, flake, rest @ ..] if command == "generate" && flag == "--flake" => {
         (flake.as_str(), rest)
      },
      [command, rest @ ..] if command == "generate" => (DEFAULT_FLAKE, rest),
      _ => {
         eprintln!("{USAGE}");
         return ExitCode::FAILURE;
      },
   };

   let [input, output] = rest else {
      eprintln!("{USAGE}");
      return ExitCode::FAILURE;
   };

   match generate(input.into(), output.into(), flake) {
      Ok(()) => ExitCode::SUCCESS,
      Err(error) => {
         eprint!("error: {error}");

         let mut source = error.source();

         while let Some(cause) = source {
            eprint!(": {cause}");
            source = cause.source();
         }

         eprintln!();
         ExitCode::FAILURE
      },
   }
}

fn generate(input: PathBuf, output: PathBuf, flake: &str) -> Result<(), Box<dyn Error>> {
   let workspace = if input
      .file_name()
      .is_some_and(|name| name == "multi-package.yaml")
   {
      Workspace::read(&input)?
   } else {
      Workspace {
         packages: vec![Project::read(&input)?],
      }
   };

   let root = normalize(&absolute(&input)?.with_file_name(""));
   let output = normalize(&absolute(&output)?);
   let output_dir = output
      .parent()
      .ok_or("output path has no parent directory")?;
   let lock_path = output_dir.join(lock::FILE_NAME);

   let mut lock = Lock {
      version: 1,
      ..Lock::default()
   };

   fs::write(
      &output,
      nix::render(&workspace, &root, output_dir, &mut lock)?,
   )?;
   fs::write(&lock_path, lock.to_json())?;

   let versions = workspace
      .packages
      .iter()
      .map(|package| (package.name.clone(), package.version.clone()))
      .collect::<Vec<_>>();

   lock.fill(&output, flake, &versions)?;
   fs::write(&lock_path, lock.to_json())?;

   Ok(())
}
