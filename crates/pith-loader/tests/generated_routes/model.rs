use proptest::prelude::*;

#[derive(Clone, Debug)]
pub struct Branch {
    pub depth: u8,
    pub spelling: String,
    pub endpoint: String,
    pub order: u32,
}

#[derive(Clone, Debug)]
pub struct Content {
    pub version: u16,
    pub text_type: bool,
}

impl Content {
    pub fn source(&self) -> String {
        let inner = if self.text_type { "Text" } else { "Bool" };
        format!("nominal Payload = {inner}\n")
    }
}

#[derive(Clone, Debug)]
pub struct Case {
    pub branches: Vec<Branch>,
    pub content: Content,
    pub alternate: Content,
    pub canonical: String,
    pub alias: String,
}

pub fn cases() -> impl Strategy<Value = Case> {
    let spelling = prop_oneof![
        Just(String::new()),
        Just("main".into()),
        "[a-zA-Z0-9._-]{0,24}"
    ];
    let branch = (0u8..4, spelling, "[a-zA-Z0-9._-]{0,24}", any::<u32>()).prop_map(
        |(depth, spelling, endpoint, order)| Branch {
            depth,
            spelling,
            endpoint,
            order,
        },
    );
    let content = || {
        (any::<u16>(), any::<bool>())
            .prop_map(|(version, text_type)| Content { version, text_type })
    };
    (
        prop::collection::vec(branch, 2..7),
        content(),
        content(),
        "[a-z]{0,12}",
        "[a-z]{1,12}",
    )
        .prop_map(|(branches, content, alternate, canonical, alias)| Case {
            branches,
            content,
            alternate,
            canonical,
            alias,
        })
}

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Path,
    Git,
    Archive,
    Registry,
}

#[derive(Clone, Copy)]
pub enum Change {
    None,
    Source(usize),
    Unreachable(usize),
    Trust(usize),
}

impl Case {
    pub fn order(&self) -> Vec<usize> {
        (0..self.branches.len()).collect()
    }

    pub fn chosen(&self) -> usize {
        self.branches
            .iter()
            .enumerate()
            .min_by_key(|(_, branch)| branch.order)
            .map_or(0, |(index, _)| index)
    }

    pub fn shuffled(&self) -> Vec<usize> {
        let mut order = self.order();
        order.sort_by_key(|index| self.branches.get(*index).map(|branch| branch.order));
        order
    }
}

#[derive(Clone, Copy, Default)]
pub struct Naming {
    pub rename_aliases: bool,
    pub shared_references: bool,
    pub shared_endpoints: bool,
}

impl Naming {
    pub fn variations() -> impl Iterator<Item = Self> {
        [false, true].into_iter().flat_map(|rename_aliases| {
            [false, true]
                .into_iter()
                .flat_map(move |shared_references| {
                    [false, true].into_iter().map(move |shared_endpoints| Self {
                        rename_aliases,
                        shared_references,
                        shared_endpoints,
                    })
                })
        })
    }
}
