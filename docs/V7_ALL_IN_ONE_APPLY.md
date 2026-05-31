# Applying the v7 all-in-one patch

This package includes v3, v4, v5, v6, and v7 overlays in one patch directory.
It can be applied directly to `main` or to any earlier feature branch.

## Apply

```bash
unzip 0xDSI-CET-standing-v7-all-in-one-patch.zip
cd 0xDSI-CET-standing-v7-all-in-one-patch
./apply_all_in_one.sh /workspaces/0xDSI-CET
```

## Validate

```bash
cd /workspaces/0xDSI-CET
./ci/all_in_one_v7_regression.sh
```

For a faster v7-only validation:

```bash
./ci/v7_standing_runtime_regression.sh
```

## Commit

```bash
git checkout -b feature/cet-standing-runtime-v7
git add .
git commit -m "Add CET standing runtime v7 all-in-one stack"
git push -u origin feature/cet-standing-runtime-v7
```
