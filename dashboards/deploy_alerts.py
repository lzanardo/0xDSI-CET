"""Deploy alert specs to monitoring backend (placeholder integration hook)."""
import yaml
spec=yaml.safe_load(open('dashboards/alert_rules.yaml'))
for a in spec.get('alerts',[]):
    print(f"deploying alert: {a['name']} expr={a['expr']}")
print('alert deployment hook completed')
