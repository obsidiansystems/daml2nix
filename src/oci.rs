use std::{
   fmt,
   str::FromStr,
};

use thiserror::Error;

#[derive(Debug, Error)]
#[error("invalid OCI reference {0:?}, expected oci://<registry>/<repository>[:<tag>][@<digest>]")]
pub struct InvalidReference(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
   registry:   String,
   repository: String,
   tag:        Option<String>,
   digest:     Option<String>,
}

impl Reference {
   pub fn registry(&self) -> &str {
      &self.registry
   }

   pub fn repository(&self) -> &str {
      &self.repository
   }

   pub fn tag(&self) -> Option<&str> {
      self.tag.as_deref()
   }

   pub fn digest(&self) -> Option<&str> {
      self.digest.as_deref()
   }

   pub fn name(&self) -> &str {
      self
         .repository
         .rsplit('/')
         .next()
         .unwrap_or(&self.repository)
   }
}

impl FromStr for Reference {
   type Err = InvalidReference;

   fn from_str(input: &str) -> Result<Self, Self::Err> {
      let invalid = || InvalidReference(input.to_owned());
      let rest = input.strip_prefix("oci://").ok_or_else(invalid)?;

      let (rest, digest) = match rest.split_once('@') {
         Some((rest, digest)) if digest.starts_with("sha256:") => (rest, Some(digest.to_owned())),
         Some(_) => return Err(invalid()),
         None => (rest, None),
      };

      let (registry, path) = rest.split_once('/').ok_or_else(invalid)?;

      let (repository, tag) = match path.rsplit_once(':') {
         Some((repository, tag)) => (repository, Some(tag.to_owned())),
         None => (path, None),
      };

      let valid = |part: &str| !part.is_empty() && !part.contains(char::is_whitespace);

      if !valid(registry) || !repository.split('/').all(valid) || tag.as_deref() == Some("") {
         return Err(invalid());
      }

      Ok(Self {
         registry: registry.to_owned(),
         repository: repository.to_owned(),
         tag,
         digest,
      })
   }
}

impl fmt::Display for Reference {
   fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
      write!(formatter, "oci://{}/{}", self.registry, self.repository)?;

      if let Some(tag) = &self.tag {
         write!(formatter, ":{tag}")?;
      }

      if let Some(digest) = &self.digest {
         write!(formatter, "@{digest}")?;
      }

      Ok(())
   }
}
