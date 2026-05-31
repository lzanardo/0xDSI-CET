# Supply Chain Security

Recommended production controls:

- CodeQL and dependency scanning in GitHub Advanced Security.
- SBOM generation for Python wheel and native shared object.
- Signed release artifacts.
- Immutable release tags.
- `OXDSI_CET_SHA256` enforcement in production jobs.
- Unity Catalog least privilege for source, state, trends, metrics, and DLQ.
- Secret scopes for all credentials.
