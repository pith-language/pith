//! Build requests phloem produces against interfaces xylem already declares.
//!
//! The package library is a peer of the build library and a consumer of it
//! (decisions 0009, 0039): a package version's description carries build
//! inputs and this module turns them into requests against xylem's declared
//! interfaces — the compile entry, the same one a package-less build drives.
//! The dependency runs package-to-build; xylem names phloem nowhere, which
//! the manifest test in `tests/dependency_direction.rs` asserts.

use pith_core::{Pure, Request, Value};

use crate::description::Description;

/// One compile request per build input the description prescribes, under
/// `toolchain_value`, against xylem's compile interface. The description
/// prescribes content; which rule serves it is 0015's selection over the
/// interfaces xylem declares, not a package-library decision.
#[must_use]
pub fn compile_requests(toolchain_value: Value, description: &Description) -> Box<[Request<Pure>]> {
    description
        .inputs
        .iter()
        .map(|source| xylem::types::compile_request(toolchain_value.clone(), *source))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceBinding;
    use pith_ids::ContentId;

    fn description(inputs: &[&[u8]]) -> Description {
        Description {
            name: "zlib".into(),
            source: SourceBinding::Path {
                path: "vendor/zlib".into(),
                content: ContentId::of_blob(b"zlib-tree"),
            },
            inputs: inputs
                .iter()
                .map(|bytes| ContentId::of_blob(bytes))
                .collect(),
            options: Box::new([]),
        }
    }

    #[test]
    fn one_request_per_prescribed_input_against_xylems_compile_interface() {
        let requests = compile_requests(
            xylem::types::toolchain("/nix/store/cc"),
            &description(&[b"zlib.c", b"adler32.c"]),
        );
        assert_eq!(requests.len(), 2);
        for request in requests.iter() {
            assert_eq!(request.interface, xylem::types::compile_interface());
        }
    }

    #[test]
    fn a_description_prescribing_nothing_requests_nothing() {
        let requests =
            compile_requests(xylem::types::toolchain("/nix/store/cc"), &description(&[]));
        assert!(requests.is_empty());
    }
}
