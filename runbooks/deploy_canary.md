# Canary Deployment
1. Deploy bridge/library to canary cluster.
2. Run stream in shadow mode for 30 min.
3. Compare trend counts and DLQ ratio to control.
4. Promote to 25%, 50%, 100% traffic.
5. Roll back if p95 or DLQ SLO breach.
