use std::{
   collections::BTreeMap,
   env,
   fs,
   io,
   path::{
      Component,
      Path,
      PathBuf,
      absolute,
   },
};

use serde::{
   Deserialize,
   de::DeserializeOwned,
};
use thiserror::Error;

use crate::oci::{
   InvalidReference,
   Reference,
};

#[derive(Debug, Error)]
pub enum Error {
   #[error("failed to read {path}")]
   Read {
      path:   PathBuf,
      #[source]
      source: io::Error,
   },
   #[error("failed to parse {path}")]
   Parse {
      path:   PathBuf,
      #[source]
      source: Box<serde_saphyr::Error>,
   },
   #[error("{path} references undefined environment variables: {}", .names.join(", "))]
   UndefinedVariables { path: PathBuf, names: Vec<String> },
   #[error("dependency {0:?} uses a source this builder does not support")]
   UnsupportedDependency(String),
   #[error("dependency {0:?} must start with oci://")]
   MissingOciScheme(String),
   #[error("dependency {dependency:?} uses undefined artifact location {alias:?}")]
   UndefinedLocation {
      dependency: String,
      alias:      String,
   },
   #[error("dependency {0:?} must have the form @<artifact-location>/<path>")]
   MalformedAlias(String),
   #[error(transparent)]
   InvalidReference(#[from] InvalidReference),
   #[error("component {0:?} must be <name>:<version>, an oci:// reference, or {{name, path}}")]
   InvalidComponent(String),
   #[error("components and override-components cannot both be set")]
   ConflictingComponents,
   #[error("damlc is pinned by a floating image tag {0:?}, which cannot be built hermetically")]
   FloatingDamlc(String),
   #[error("sdk-version and components cannot both be set")]
   SdkWithComponents,
   #[error("no damlc pin, set sdk-version or a damlc component")]
   MissingDamlc,
}

#[derive(Debug)]
pub struct Project {
   pub root:              PathBuf,
   pub name:              String,
   pub version:           String,
   pub source:            PathBuf,
   pub build_options:     Vec<String>,
   pub dependencies:      Vec<Dependency>,
   pub data_dependencies: Vec<Dependency>,
   pub damlc:             Damlc,
}

#[derive(Debug)]
pub struct Dependency {
   pub location:        Location,
   pub main_package_id: Option<String>,
}

#[derive(Debug)]
pub enum Location {
   Builtin(String),
   Dar(PathBuf),
   Oci {
      reference: Reference,
      insecure:  bool,
   },
}

#[derive(Debug)]
pub enum Damlc {
   Sdk(String),
   Version(String),
   Oci(Reference),
   Local(PathBuf),
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct Pins {
   sdk_version:         Option<String>,
   components:          Option<Vec<RawComponent>>,
   override_components: Option<BTreeMap<String, RawOverride>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct RawProject {
   name:               String,
   version:            String,
   source:             PathBuf,
   #[serde(default)]
   build_options:      Vec<String>,
   #[serde(default)]
   dependencies:       Vec<RawDependency>,
   #[serde(default)]
   data_dependencies:  Vec<RawDependency>,
   #[serde(default)]
   artifact_locations: BTreeMap<String, RawLocation>,
   #[serde(flatten)]
   pins:               Pins,
}

#[derive(Deserialize)]
struct Interpolation {
   #[serde(rename = "environment-variable-interpolation")]
   enabled: Option<bool>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawDependency {
   Plain(String),
   WithPackageId {
      value:           String,
      #[serde(rename = "main-package-id")]
      main_package_id: String,
   },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawComponent {
   Plain(String),
   Local { name: String, path: PathBuf },
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct RawOverride {
   version:    Option<String>,
   image_tag:  Option<String>,
   local_path: Option<PathBuf>,
   uri:        Option<String>,
}

#[derive(Deserialize)]
struct RawLocation {
   url:      String,
   #[serde(default)]
   insecure: bool,
}

impl Project {
   pub fn read(path: &Path) -> Result<Self, Error> {
      Self::read_inherited(path, None)
   }

   pub(crate) fn read_inherited(
      path: &Path,
      workspace: Option<(&Pins, &Path)>,
   ) -> Result<Self, Error> {
      let (raw, root) = load::<RawProject>(path)?;
      let damlc = raw.pins.damlc(&root, workspace)?;

      let resolve = |deps: Vec<RawDependency>| {
         deps
            .into_iter()
            .map(|dep| dep.resolve(&root, &raw.artifact_locations))
            .collect::<Result<Vec<_>, _>>()
      };

      Ok(Self {
         dependencies: resolve(raw.dependencies)?,
         data_dependencies: resolve(raw.data_dependencies)?,
         name: raw.name,
         version: raw.version,
         source: normalize(&root.join(raw.source)),
         build_options: raw.build_options,
         damlc,
         root,
      })
   }

   pub fn dist(&self) -> PathBuf {
      self
         .root
         .join(".daml/dist")
         .join(format!("{}-{}.dar", self.name, self.version))
   }
}

impl Pins {
   fn pinned(&self, root: &Path) -> Result<Option<Damlc>, Error> {
      match (&self.components, &self.override_components) {
         (Some(_), Some(_)) => Err(Error::ConflictingComponents),
         (Some(components), None) => {
            components
               .iter()
               .map(|component| component.damlc(root))
               .find_map(Result::transpose)
               .transpose()
         },
         (None, Some(overrides)) => {
            overrides
               .get("damlc")
               .map(|over| over.damlc(root))
               .transpose()
         },
         (None, None) => Ok(None),
      }
   }

   fn has_components(&self) -> bool {
      self
         .components
         .as_ref()
         .is_some_and(|components| !components.is_empty())
         || self
            .override_components
            .as_ref()
            .is_some_and(|overrides| !overrides.is_empty())
   }

   fn damlc(&self, root: &Path, workspace: Option<(&Self, &Path)>) -> Result<Damlc, Error> {
      let sdk_version = self
         .sdk_version
         .as_ref()
         .or_else(|| workspace.and_then(|(pins, _)| pins.sdk_version.as_ref()));
      let has_components =
         self.has_components() || workspace.is_some_and(|(pins, _)| pins.has_components());

      if has_components && sdk_version.is_some() {
         return Err(Error::SdkWithComponents);
      }

      if let Some(damlc) = self.pinned(root)? {
         return Ok(damlc);
      }

      if let Some((pins, workspace_root)) = workspace
         && let Some(damlc) = pins.pinned(workspace_root)?
      {
         return Ok(damlc);
      }

      sdk_version
         .map(|version| Damlc::Sdk(version.clone()))
         .ok_or(Error::MissingDamlc)
   }
}

pub(crate) fn load<T: DeserializeOwned>(path: &Path) -> Result<(T, PathBuf), Error> {
   let read_error = |source| {
      Error::Read {
         path: path.to_owned(),
         source,
      }
   };

   let contents = fs::read_to_string(path).map_err(read_error)?;
   let root = normalize(&absolute(path).map_err(read_error)?.with_file_name(""));

   let interpolate = serde_saphyr::from_str::<Interpolation>(&contents)
      .map_or(true, |interpolation| interpolation.enabled.unwrap_or(true));

   let expanded = if interpolate {
      expand_env(&contents).map_err(|names| {
         Error::UndefinedVariables {
            path: path.to_owned(),
            names,
         }
      })?
   } else {
      contents
   };

   let raw = serde_saphyr::from_str::<T>(&expanded).map_err(|source| {
      Error::Parse {
         path:   path.to_owned(),
         source: Box::new(source),
      }
   })?;

   Ok((raw, root))
}

pub fn normalize(path: &Path) -> PathBuf {
   let mut normal = PathBuf::new();

   for component in path.components() {
      match component {
         Component::CurDir => {},
         Component::ParentDir => {
            normal.pop();
         },
         other => normal.push(other),
      }
   }

   normal
}

impl RawComponent {
   fn damlc(&self, root: &Path) -> Result<Option<Damlc>, Error> {
      let entry = match self {
         Self::Local { name, path } => {
            return Ok((name == "damlc").then(|| Damlc::Local(normalize(&root.join(path)))));
         },
         Self::Plain(entry) => entry,
      };

      if entry.starts_with("oci://") {
         let reference = entry.parse::<Reference>()?;
         return Ok((reference.name() == "damlc").then_some(Damlc::Oci(reference)));
      }

      match entry.split_once(':') {
         Some((name, version))
            if !entry.contains(['/', '@']) && !name.is_empty() && !version.is_empty() =>
         {
            Ok((name == "damlc").then(|| Damlc::Version(version.to_owned())))
         },
         _ => Err(Error::InvalidComponent(entry.clone())),
      }
   }
}

impl RawOverride {
   fn damlc(&self, root: &Path) -> Result<Damlc, Error> {
      if let Some(path) = &self.local_path {
         return Ok(Damlc::Local(normalize(&root.join(path))));
      }

      if let Some(uri) = &self.uri {
         return Ok(Damlc::Oci(uri.parse()?));
      }

      if let Some(version) = &self.version {
         return Ok(Damlc::Version(version.clone()));
      }

      Err(Error::FloatingDamlc(
         self.image_tag.clone().unwrap_or_default(),
      ))
   }
}

impl RawDependency {
   fn resolve(
      self,
      root: &Path,
      locations: &BTreeMap<String, RawLocation>,
   ) -> Result<Dependency, Error> {
      let (value, main_package_id) = match self {
         Self::Plain(value) => (value, None),
         Self::WithPackageId {
            value,
            main_package_id,
         } => (value, Some(main_package_id)),
      };

      let location = if value.starts_with("oci://") {
         Location::Oci {
            reference: value.parse()?,
            insecure:  false,
         }
      } else if value.starts_with("git:")
         || value.starts_with("http://")
         || value.starts_with("https://")
      {
         return Err(Error::UnsupportedDependency(value));
      } else if let Some(aliased) = value.strip_prefix('@') {
         let (alias, suffix) = aliased
            .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'))
            .map_or((aliased, ""), |end| aliased.split_at(end));
         let alias = format!("@{alias}");

         let location = locations.get(&alias).ok_or_else(|| {
            Error::UndefinedLocation {
               dependency: value.clone(),
               alias:      alias.clone(),
            }
         })?;

         if !suffix.starts_with('/') {
            return Err(Error::MalformedAlias(value));
         }

         if location.url.starts_with("git:") {
            return Err(Error::UnsupportedDependency(value));
         }

         Location::Oci {
            reference: format!("{}{suffix}", location.url).parse()?,
            insecure:  location.insecure,
         }
      } else if value.to_ascii_lowercase().ends_with(".dar") {
         Location::Dar(normalize(&root.join(value)))
      } else if value.contains(':') {
         return Err(Error::MissingOciScheme(value));
      } else {
         Location::Builtin(value)
      };

      Ok(Dependency {
         location,
         main_package_id,
      })
   }
}

fn expand_env(contents: &str) -> Result<String, Vec<String>> {
   let mut expanded = String::with_capacity(contents.len());
   let mut undefined = Vec::new();
   let mut rest = contents;

   while let Some(start) = rest.find('$') {
      expanded.push_str(&rest[..start]);
      let after = &rest[start + 1..];

      if expanded.ends_with('\\') {
         expanded.pop();
         expanded.push('$');
         rest = after;
         continue;
      }

      let (name, consumed) = if let Some(braced) = after.strip_prefix('{') {
         match braced.find('}') {
            Some(end) => (&braced[..end], end + 2),
            None => ("", 0),
         }
      } else {
         let end = after
            .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .unwrap_or(after.len());
         (&after[..end], end)
      };

      if name.is_empty() {
         expanded.push('$');
         rest = after;
         continue;
      }

      match env::var(name) {
         Ok(value) => expanded.push_str(&value),
         Err(_) => undefined.push(name.to_owned()),
      }

      rest = &after[consumed..];
   }

   expanded.push_str(rest);

   if undefined.is_empty() {
      Ok(expanded)
   } else {
      Err(undefined)
   }
}
