-- Pro Gerät höchstens ein geplanter Batteriewechsel.
create unique index maintenance_one_planned_per_device on maintenance_tasks (device_id) where status = 'planned';
