use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct Environment {
    pub pith_home: Option<PathBuf>,
    pub xdg_cache_home: Option<PathBuf>,
    pub home: Option<PathBuf>,
}

impl Environment {
    #[must_use]
    pub fn from_process() -> Self {
        Self {
            pith_home: variable("PITH_HOME"),
            xdg_cache_home: variable("XDG_CACHE_HOME"),
            home: variable("HOME"),
        }
    }
}

fn variable(name: &str) -> Option<PathBuf> {
    match std::env::var_os(name) {
        Some(value) if !value.is_empty() => Some(PathBuf::from(value)),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootsError {
    NoHome,
}

impl std::fmt::Display for RootsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoHome => formatter.write_str(
                "no store root: set PITH_HOME, XDG_CACHE_HOME or HOME, or pass --store and --state",
            ),
        }
    }
}

impl std::error::Error for RootsError {}

/// The resolved content and state roots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Roots {
    store: PathBuf,
    state: PathBuf,
}

impl Roots {
    /// Resolve both halves. `store` and `state` are the explicit overrides,
    /// each taking precedence over `$PITH_HOME` and then over the XDG default,
    /// and each overriding its half alone so a hermetic run or a test fixture
    /// can move one without the other.
    ///
    /// # Errors
    /// [`RootsError::NoHome`] when a half is unnamed and no home directory
    /// yields a default for it.
    pub fn resolve(
        environment: &Environment,
        store: Option<PathBuf>,
        state: Option<PathBuf>,
    ) -> Result<Self, RootsError> {
        if let (Some(store), Some(state)) = (store.clone(), state.clone()) {
            return Ok(Self { store, state });
        }
        let home = pith_home(environment).ok_or(RootsError::NoHome)?;
        Ok(Self {
            store: store.unwrap_or_else(|| home.join("store")),
            state: state.unwrap_or_else(|| home.join("state.db")),
        })
    }

    #[must_use]
    pub fn under(home: &Path) -> Self {
        Self {
            store: home.join("store"),
            state: home.join("state.db"),
        }
    }

    #[must_use]
    pub fn store(&self) -> &Path {
        &self.store
    }

    #[must_use]
    pub fn state(&self) -> &Path {
        &self.state
    }
}

fn pith_home(environment: &Environment) -> Option<PathBuf> {
    if let Some(home) = environment.pith_home.clone() {
        return Some(home);
    }
    let cache = environment
        .xdg_cache_home
        .clone()
        .or_else(|| environment.home.as_ref().map(|home| home.join(".cache")))?;
    Some(cache.join("pith"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn environment(pith_home: &str, xdg: &str, home: &str) -> Environment {
        let optional = |value: &str| (!value.is_empty()).then(|| PathBuf::from(value));
        Environment {
            pith_home: optional(pith_home),
            xdg_cache_home: optional(xdg),
            home: optional(home),
        }
    }

    struct Row {
        pith_home: &'static str,
        xdg: &'static str,
        home: &'static str,
        store_flag: Option<&'static str>,
        state_flag: Option<&'static str>,
        store: &'static str,
        state: &'static str,
    }

    #[test]
    fn root_precedence_holds_in_every_combination() {
        let rows = &[
            Row {
                pith_home: "/home/p",
                xdg: "/c",
                home: "/h",
                store_flag: Some("/flag/store"),
                state_flag: Some("/flag/state.db"),
                store: "/flag/store",
                state: "/flag/state.db",
            },
            Row {
                pith_home: "/home/p",
                xdg: "/c",
                home: "/h",
                store_flag: Some("/flag/store"),
                state_flag: None,
                store: "/flag/store",
                state: "/home/p/state.db",
            },
            Row {
                pith_home: "/home/p",
                xdg: "/c",
                home: "/h",
                store_flag: None,
                state_flag: Some("/flag/state.db"),
                store: "/home/p/store",
                state: "/flag/state.db",
            },
            Row {
                pith_home: "/home/p",
                xdg: "/c",
                home: "/h",
                store_flag: None,
                state_flag: None,
                store: "/home/p/store",
                state: "/home/p/state.db",
            },
            Row {
                pith_home: "",
                xdg: "/c",
                home: "/h",
                store_flag: None,
                state_flag: None,
                store: "/c/pith/store",
                state: "/c/pith/state.db",
            },
            Row {
                pith_home: "",
                xdg: "",
                home: "/h",
                store_flag: None,
                state_flag: None,
                store: "/h/.cache/pith/store",
                state: "/h/.cache/pith/state.db",
            },
        ];
        for row in rows {
            let resolved = Roots::resolve(
                &environment(row.pith_home, row.xdg, row.home),
                row.store_flag.map(PathBuf::from),
                row.state_flag.map(PathBuf::from),
            );
            assert_eq!(
                resolved,
                Ok(Roots {
                    store: PathBuf::from(row.store),
                    state: PathBuf::from(row.state),
                }),
                "precedence differs for PITH_HOME={:?} XDG={:?} HOME={:?} --store={:?} \
                 --state={:?}",
                row.pith_home,
                row.xdg,
                row.home,
                row.store_flag,
                row.state_flag
            );
        }
    }

    #[test]
    fn both_halves_named_needs_no_environment() {
        let resolved = Roots::resolve(
            &Environment::default(),
            Some(PathBuf::from("/s")),
            Some(PathBuf::from("/db")),
        );

        assert_eq!(resolved.as_ref().map(Roots::store), Ok(Path::new("/s")));
        assert_eq!(resolved.as_ref().map(Roots::state), Ok(Path::new("/db")));
    }

    #[test]
    fn an_unnamed_half_with_no_home_refuses() {
        assert_eq!(
            Roots::resolve(&Environment::default(), Some(PathBuf::from("/s")), None),
            Err(RootsError::NoHome)
        );
        assert_eq!(
            Roots::resolve(&Environment::default(), None, None),
            Err(RootsError::NoHome)
        );
    }

    #[test]
    fn an_empty_variable_is_not_a_root() {
        let resolved = Roots::resolve(&environment("", "", "/h"), None, None);

        assert_eq!(
            resolved.as_ref().map(Roots::store),
            Ok(Path::new("/h/.cache/pith/store"))
        );
    }
}
