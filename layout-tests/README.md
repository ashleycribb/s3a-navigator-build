Acceptance tests for the v24.1 build plan workspace, vendor, and pack-skeleton steps.

- `test_workspace_layout.py` — step 1 DD-121 cargo workspace layout (§11.1).
- `test_vendor_tree.py` — step 2 vendored Navigator Rust core, LICENSE/NOTICE,
  native-host / getrandom / wasm_bindgen gates, Apache change notices (AT-02,
  AT-03), and the GitHub Actions workflow (§11.2).
- `test_pack_skeleton.py` — step 3 s3a-pack-kite skeleton, crate types,
  out-of-workspace membership, AT-01 cargo-tree guard, and CI of pack tests.
  AT-42 itself lives in `kite-pack-tests/tests/at_42.rs`.
- `test_step4_fonts.py` / `test_step4_spec.py` — step 4 required CJK and
  monochrome emoji fonts, and the v24.2 wording already applied.
- `test_step5_spec.py` — step 5 v24.3 host and fetch hardening wording
  (F-04, F-05, F-06, F-07, F-09, F-10, F-11).
- `test_step7_nesting.py` — step 7 F-14 nesting-depth bound wording and
  AT-71 deeply nested YAML, Markdown, and HTML seeds.

These tests are not a product crate and must not be added as a workspace
member. Do not edit them to make them pass.

Run from the repo root:

    python3 -m unittest discover -s layout-tests -p 'test_*.py' -v
