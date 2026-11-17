Acceptance tests for the v24.1 build plan workspace and vendor steps.

- `test_workspace_layout.py` — step 1 DD-121 cargo workspace layout (§11.1).
- `test_vendor_tree.py` — step 2 vendored Navigator Rust core, LICENSE/NOTICE,
  native-host / getrandom / wasm_bindgen gates, Apache change notices (AT-02,
  AT-03), and the GitHub Actions workflow (§11.2).

These tests are not a product crate and must not be added as a workspace
member. Do not edit them to make them pass.

Run from the repo root:

    python3 -m unittest discover -s layout-tests -p 'test_*.py' -v
