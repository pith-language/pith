use super::model::{Case, Change, Content, Kind, Naming};
use super::store::{Location, Module, Request, Store};

pub fn build(case: &Case, kind: Kind, order: &[usize], change: Change, naming: Naming) -> Store {
    let mut store = Store::default();
    let mut registries = String::new();
    let canonical = Location::Source(case.canonical.clone());
    let alternate = Location::Source(format!("{}/alternate", case.canonical));
    store
        .modules
        .insert(canonical.clone(), content(&case.content));
    store
        .modules
        .insert(alternate.clone(), content(&case.alternate));
    for (index, branch) in case.branches.iter().enumerate() {
        let alias = format!(
            "{}r{index}",
            if naming.rename_aliases {
                "renamed"
            } else {
                &case.alias
            }
        );
        let token = if naming.shared_references {
            "main"
        } else {
            &branch.spelling
        };
        let endpoint = if naming.shared_endpoints {
            "shared"
        } else {
            &branch.endpoint
        };
        let root_key = if matches!(change, Change::Trust(chosen) if chosen == index) {
            "B"
        } else {
            "A"
        };
        registries.push_str(&format!(
            "registry {alias} = \"registry-{endpoint}\" root \"ed25519:{root_key}\"\n"
        ));
        let (clause, request) = route(kind, &alias, token, endpoint);
        let location = Location::Branch(index, branch.depth);
        let target = if matches!(change, Change::Source(chosen) if chosen == index) {
            &alternate
        } else {
            &canonical
        };
        if !matches!(change, Change::Unreachable(chosen) if chosen == index) {
            store
                .routes
                .insert((location.clone(), request), target.clone());
        }
        store
            .modules
            .insert(location, node(index, branch.depth, "example/dep", &clause));
        for depth in 0..branch.depth {
            let next_depth = depth.saturating_add(1);
            let current = Location::Branch(index, depth);
            let next = Location::Branch(index, next_depth);
            store
                .routes
                .insert((current.clone(), Request::Path("next".into())), next);
            store.modules.insert(
                current,
                node(
                    index,
                    depth,
                    &subject(index, next_depth),
                    "from path \"next\"",
                ),
            );
        }
        store.routes.insert(
            (Location::Root, Request::Path(format!("branch{index}"))),
            Location::Branch(index, 0),
        );
    }
    let uses = order
        .iter()
        .map(|index| {
            format!(
                "use branch{index} = {} from path \"branch{index}\"\n",
                subject(*index, 0)
            )
        })
        .collect::<String>();
    store.modules.insert(
        Location::Root,
        Module {
            manifest: format!("module example/root 1\n{registries}{uses}"),
            source: "nominal Root = Bool\n".into(),
        },
    );
    store
}

fn content(content: &Content) -> Module {
    Module {
        manifest: format!("module example/dep {}\n", content.version),
        source: content.source(),
    }
}

fn subject(index: usize, depth: u8) -> String {
    format!("example/branch{index}n{depth}")
}

fn node(index: usize, depth: u8, dependency: &str, route: &str) -> Module {
    Module {
        manifest: format!(
            "module {} 1\nuse child = {dependency} {route}\n",
            subject(index, depth)
        ),
        source: "nominal Node = Bool\n".into(),
    }
}

fn route(kind: Kind, alias: &str, token: &str, endpoint: &str) -> (String, Request) {
    match kind {
        Kind::Path => (
            format!("from path \"dep-{token}\""),
            Request::Path(format!("dep-{token}")),
        ),
        Kind::Git => (
            format!("from git \"repo-{endpoint}\" revision \"{token}\""),
            Request::Git(format!("repo-{endpoint}"), token.into(), None),
        ),
        Kind::Archive => (
            format!("from archive \"archive-{endpoint}\" digest \"{token}\""),
            Request::Archive(format!("archive-{endpoint}"), token.into()),
        ),
        Kind::Registry => (
            format!("from registry {alias}"),
            Request::Registry(format!("registry-{endpoint}")),
        ),
    }
}
