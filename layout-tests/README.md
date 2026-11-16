Step 1 acceptance tests for the v24.1 build plan.

These tests assert the cargo workspace layout required by SPEC.md §5.1 / DD-121 / §11 step 1. They are not a product crate and must not be added as a workspace member of the scaffold.

Run from this directory or from the repo root:

    python3 layout-tests/test_workspace_layout.py

They are expected to fail until the scaffold in build-plan step 1 is implemented. Do not edit these tests to make them pass.
