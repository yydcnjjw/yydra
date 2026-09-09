// SPDX-License-Identifier: MIT OR Apache-2.0

#[test]
fn rejects_an_invalid_contract_before_requiring_frontend_tools() {
    let root = tempfile::tempdir().unwrap();
    let error = yydra_build::generate_api(
        b"{}",
        &yydra_build::ApiBuild {
            frontend: &root.path().join("frontend"),
            out_dir: root.path(),
        },
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("API_OPENAPI_PROFILE_INVALID"));
    assert!(!root.path().join("yydra-api").exists());
}
