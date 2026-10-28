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
   nix,
   project::{
      Project,
      normalize,
   },
   workspace::Workspace,
};

const USAGE: &str =
   "usage: daml2nix generate <daml.yaml | multi-package.yaml> <output.nix>";

fn main() -> ExitCode {
   let args = env::args().skip(1).collect::<Vec<_>>();

   let [command, input, output] = args.as_slice() else {
      eprintln!("{USAGE}");
      return ExitCode::FAILURE;
   };

   if command != "generate" {
      eprintln!("{USAGE}");
      return ExitCode::FAILURE;
   }

   match generate(input.into(), output.into()) {
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

fn generate(input: PathBuf, output: PathBuf) -> Result<(), Box<dyn Error>> {
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

   fs::write(&output, nix::render(&workspace, &root, output_dir)?)?;

   Ok(())
}
