# Main Merge Runbook

Use this runbook to promote v4 + v5 functionality into `main`.

1. Start from the latest v4 branch.
2. Apply the v5 patch.
3. Run `make main-ready` or the individual CI scripts.
4. Push a clean branch.
5. Open PR into `main`.
6. Require green GitHub Actions.
7. Squash merge or create a signed merge commit.
8. Tag a release, build wheel, generate SBOM, and deploy Databricks bundle.
