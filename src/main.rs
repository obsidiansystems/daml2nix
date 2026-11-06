use std::{
   collections::BTreeMap,
   env,
   error::Error,
   fs::{
      self,
      Permissions,
   },
   io,
   os::unix::fs::PermissionsExt,
   path::{
      Path,
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
   store::{
      self,
      Evaluated,
   },
   workspace::Workspace,
};
use thiserror::Error;

const USAGE: &str = "\
usage: daml2nix generate [--flake <ref>] <daml.yaml | multi-package.yaml> <output.nix>
       daml2nix build [--flake <ref>] [<output.nix>]
       daml2nix verify [--flake <ref>] [<output.nix>]
       daml2nix install [--flake <ref>] [<output.nix>]
       daml2nix shell [--flake <ref>] [--package <name>] [<output.nix>]
       daml2nix clean [<output.nix>]";

const DEFAULT_FLAKE: &str = "github:obsidiansystems/daml2nix";
const DEFAULT_OUTPUT: &str = "default.nix";
const STATE_DIR: &str = ".dpm-nix";

#[derive(Debug, Error)]
enum Failure {
   #[error("no package named {0:?} in the generated Nix")]
   UnknownPackage(String),
   #[error("the generated Nix has several packages, pick one with --package ({0})")]
   AmbiguousPackage(String),
   #[error("{0} package(s) did not match the lock")]
   Mismatch(usize),
}

struct Arguments {
   command: String,
   flake:   Option<String>,
   package: Option<String>,
   paths:   Vec<PathBuf>,
}

struct Generated {
   nix_file:  PathBuf,
   flake_url: Option<String>,
   lock:      Lock,
   evaluated: BTreeMap<String, Evaluated>,
}

fn main() -> ExitCode {
   let Some(arguments) = parse(env::args().skip(1)) else {
      eprintln!("{USAGE}");
      return ExitCode::FAILURE;
   };

   let flake = arguments.flake.as_deref();
   let output = arguments
      .paths
      .first()
      .map_or(Path::new(DEFAULT_OUTPUT), PathBuf::as_path);

   let result = match (arguments.command.as_str(), arguments.paths.as_slice()) {
      ("generate", [input, output]) => {
         generate(input, output, flake.unwrap_or(DEFAULT_FLAKE)).map(|()| ExitCode::SUCCESS)
      },
      ("build", [] | [_]) => {
         Generated::load(output, flake)
            .and_then(|generated| generated.build())
            .map(|()| ExitCode::SUCCESS)
      },
      ("verify", [] | [_]) => {
         Generated::load(output, flake)
            .and_then(|generated| generated.verify())
            .map(|()| ExitCode::SUCCESS)
      },
      ("install", [] | [_]) => {
         Generated::load(output, flake)
            .and_then(|generated| generated.install())
            .map(|()| ExitCode::SUCCESS)
      },
      ("shell", [] | [_]) => {
         Generated::load(output, flake)
            .and_then(|generated| generated.shell(arguments.package.as_deref()))
      },
      ("clean", [] | [_]) if flake.is_none() => clean(output).map(|()| ExitCode::SUCCESS),
      _ => {
         eprintln!("{USAGE}");
         return ExitCode::FAILURE;
      },
   };

   match result {
      Ok(code) => code,
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

fn parse(mut args: impl Iterator<Item = String>) -> Option<Arguments> {
   let command = args.next()?;
   let mut flake = None;
   let mut package = None;
   let mut paths = Vec::new();

   while let Some(arg) = args.next() {
      match arg.as_str() {
         "--flake" => flake = Some(args.next()?),
         "--package" if command == "shell" => package = Some(args.next()?),
         _ if arg.starts_with("--") => return None,
         _ => paths.push(arg.into()),
      }
   }

   Some(Arguments {
      command,
      flake,
      package,
      paths,
   })
}

fn generate(input: &Path, output: &Path, flake: &str) -> Result<(), Box<dyn Error>> {
   let workspace = if input
      .file_name()
      .is_some_and(|name| name == "multi-package.yaml")
   {
      Workspace::read(input)?
   } else {
      Workspace {
         packages: vec![Project::read(input)?],
      }
   };

   let root = normalize(&absolute(input)?.with_file_name(""));
   let output = normalize(&absolute(output)?);
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

fn clean(nix_file: &Path) -> Result<(), Box<dyn Error>> {
   let state = nix_file.with_file_name(STATE_DIR);

   match fs::remove_dir_all(&state) {
      Ok(()) => println!("removed {}", state.display()),
      Err(error) if error.kind() == io::ErrorKind::NotFound => {},
      Err(error) => return Err(error.into()),
   }

   Ok(())
}

impl Generated {
   fn load(nix_file: &Path, flake: Option<&str>) -> Result<Self, Box<dyn Error>> {
      let nix_file = normalize(&absolute(nix_file)?);
      let lock = Lock::read(&nix_file.with_file_name(lock::FILE_NAME))?;
      let flake_url = flake.map(store::locked_url).transpose()?;
      let evaluated = store::evaluate(flake_url.as_deref().unwrap_or(&lock.builder), &nix_file)?;

      Ok(Self {
         nix_file,
         flake_url,
         lock,
         evaluated,
      })
   }

   fn flake_url(&self) -> &str {
      self.flake_url.as_deref().unwrap_or(&self.lock.builder)
   }

   fn build(&self) -> Result<(), Box<dyn Error>> {
      let links = self.nix_file.with_file_name(STATE_DIR);
      fs::create_dir_all(&links)?;

      for (name, package) in &self.evaluated {
         store::build(&[&package.derivation], Some(&links.join(name)))?;
         println!("{name}: {}", package.dar.display());
      }

      Ok(())
   }

   fn verify(&self) -> Result<(), Box<dyn Error>> {
      let mut mismatches = 0;

      for name in self
         .evaluated
         .keys()
         .filter(|name| !self.lock.packages.contains_key(*name))
      {
         println!("{name}: not in the lock");
         mismatches += 1;
      }

      for (name, expected) in &self.lock.packages {
         let Some(actual) = self.evaluated.get(name) else {
            println!("{name}: not in the generated Nix");
            mismatches += 1;
            continue;
         };

         let mut problems = Vec::new();

         if actual.derivation != expected.derivation {
            problems.push(format!(
               "derivation {}, lock has {}",
               actual.derivation, expected.derivation
            ));
         }

         let source = store::hash_path(&actual.source)?;

         if source != expected.source {
            problems.push(format!("source {source}, lock has {}", expected.source));
         }

         store::build(&[&actual.derivation], None)?;

         if let Err(error) = store::rebuild(&actual.derivation) {
            problems.push(format!("rebuild failed ({error})"));
         }

         let dar = store::hash_file(&actual.dar)?;

         if dar != expected.dar {
            problems.push(format!("dar {dar}, lock has {}", expected.dar));
         }

         if problems.is_empty() {
            println!("{name}: ok");
         } else {
            println!("{name}: {}", problems.join("; "));
            mismatches += 1;
         }
      }

      if mismatches > 0 {
         return Err(Failure::Mismatch(mismatches).into());
      }

      Ok(())
   }

   fn install(&self) -> Result<(), Box<dyn Error>> {
      self.build()?;

      for (name, package) in &self.evaluated {
         let dist = package.directory.join(".daml/dist");
         let target = dist.join(
            package
               .dar
               .file_name()
               .expect("dar paths end in a file name"),
         );
         fs::create_dir_all(&dist)?;

         match fs::remove_file(&target) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error.into()),
            _ => {},
         }

         fs::copy(&package.dar, &target)?;
         fs::set_permissions(&target, Permissions::from_mode(0o644))?;
         println!("{name}: installed {}", target.display());
      }

      Ok(())
   }

   fn shell(&self, package: Option<&str>) -> Result<ExitCode, Box<dyn Error>> {
      let names = self
         .evaluated
         .keys()
         .map(String::as_str)
         .collect::<Vec<_>>();

      let name = match (package, names.as_slice()) {
         (Some(name), _) if self.evaluated.contains_key(name) => name,
         (Some(name), _) => return Err(Failure::UnknownPackage(name.to_owned()).into()),
         (None, [only]) => only,
         (None, _) => return Err(Failure::AmbiguousPackage(names.join(", ")).into()),
      };

      let installable = format!(
         "({}).packages.{}.shell",
         store::expression(self.flake_url(), &self.nix_file),
         serde_json::to_string(name)?,
      );

      let status = store::develop(&installable)?;
      Ok(status
         .code()
         .and_then(|code| u8::try_from(code).ok())
         .map_or(ExitCode::FAILURE, ExitCode::from))
   }
}
