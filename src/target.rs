//! Display targets this toolkit knows how to produce/patch `.tft` files for.
//!
//! There is exactly one variant on purpose. Every offset and record layout in
//! [`crate::tft`] was derived from a real `h5.tft` compiled for the
//! NX8048P050-011R-Y, and nothing here has been cross-checked against any
//! other Nextion model or resolution. A different display (even a different
//! resolution on the same "generic" enhanced-series chip) could use a
//! different header layout, record size, or text-pool slot size -- we simply
//! don't know, because we've never seen one. Treat any other target as
//! unsupported until someone reverse-engineers it the same way and adds a
//! variant here, backed by a real reference `.HMI`/`.tft` pair (see
//! `docs/targets.md`).

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Nx8048p050011rY,
}

impl Target {
    pub const fn width(self) -> u16 {
        match self {
            Target::Nx8048p050011rY => 800,
        }
    }

    pub const fn height(self) -> u16 {
        match self {
            Target::Nx8048p050011rY => 480,
        }
    }

    pub const fn model_name(self) -> &'static str {
        match self {
            Target::Nx8048p050011rY => "NX8048P050-011R-Y",
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.model_name())
    }
}

#[derive(Debug, thiserror::Error)]
#[error(
    "unsupported target {0:?}: this toolkit only supports NX8048P050-011R-Y \
     (see docs/targets.md for what it would take to add another)"
)]
pub struct UnknownTargetError(pub String);

impl FromStr for Target {
    type Err = UnknownTargetError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "NX8048P050-011R-Y" | "NX8048P050011RY" => Ok(Target::Nx8048p050011rY),
            other => Err(UnknownTargetError(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_name() {
        assert_eq!(
            "NX8048P050-011R-Y".parse::<Target>().unwrap(),
            Target::Nx8048p050011rY
        );
    }

    #[test]
    fn parses_case_insensitively() {
        assert_eq!(
            "nx8048p050-011r-y".parse::<Target>().unwrap(),
            Target::Nx8048p050011rY
        );
    }

    #[test]
    fn rejects_unknown_target() {
        let err = "NX4832K035".parse::<Target>().unwrap_err();
        assert!(err.to_string().contains("unsupported target"));
    }

    #[test]
    fn dimensions_match_reference_file() {
        assert_eq!(Target::Nx8048p050011rY.width(), 800);
        assert_eq!(Target::Nx8048p050011rY.height(), 480);
    }
}
