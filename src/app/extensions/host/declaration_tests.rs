//! Checks over the product's linked handler declarations, which only exist once
//! the application's handlers are linked in.

use knot_protocol::declarations::{HostError, TransportError, registry};

#[test]
fn linked_product_handlers_are_discovered_in_test_binaries() {
    let registry = registry();
    assert!(registry.get("openDocument").is_some());
    assert!(registry.get("writeClipboardText").is_some());
    assert!(registry.error_codes().contains(&"Cancelled"));
    assert!(registry.error_codes().contains(&"ResourceNotFound"));
}

#[test]
fn declaration_codes_match_public_typescript() {
    let mut codes = registry().error_codes();
    codes.extend(TransportError::codes());
    codes.sort_unstable();
    codes.dedup();
    let declaration = include_str!("../../../../crates/knot-host/src/js/knot.d.ts")
        .split("export type HostErrorCode =")
        .nth(1)
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let mut declared = declaration
        .split('"')
        .enumerate()
        .filter_map(|(index, value)| (index % 2 == 1).then_some(value))
        .collect::<Vec<_>>();
    declared.sort_unstable();
    assert_eq!(
        codes, declared,
        "update HostErrorCode from handler and transport descriptors"
    );
}

#[test]
fn responses_keep_the_extension_and_request_identity() {
    use super::handlers::{ContextArguments, active_buffer};
    use knot_protocol::{
        BufferHandle, ExtensionId, ExtensionLifecycleId, HostRequest, HostResponse, RequestId,
        declarations::{DeclaredRequest, Responder},
    };
    let request = HostRequest {
        extension: ExtensionId::new(7),
        lifecycle: ExtensionLifecycleId::new(3),
        id: RequestId::new(11),
        invocation: None,
        operation: DeclaredRequest::new::<active_buffer>(ContextArguments { target: None }),
    };
    let response = HostResponse {
        extension: request.extension,
        lifecycle: request.lifecycle,
        id: request.id,
        result: Ok(Responder::<active_buffer>::new().respond(Ok(Some(BufferHandle::new(3))))),
    };
    assert_eq!(response.extension, request.extension);
    assert_eq!(response.lifecycle, request.lifecycle);
    assert_eq!(response.id, request.id);
}

#[test]
fn buffer_errors_keep_their_wire_code() {
    assert_eq!(
        serde_json::to_value(
            super::handlers::BufferError::RevisionConflict
                .into_failure()
                .code
        )
        .unwrap(),
        serde_json::json!("RevisionConflict")
    );
}
