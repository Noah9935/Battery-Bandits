-- Überwachungs-Service: Konfiguration und zuletzt bewerteter Zustand je Gerät.
create table monitor.setting (
    key   text primary key,
    value double precision not null
);

create table monitor.device_state (
    device_id  text primary key,
    charge_pct double precision not null,
    reading_ts timestamptz      not null,
    status     text             not null check (status in ('ok', 'critical')),
    changed_at timestamptz      not null default now()
);

create index device_state_status_idx on monitor.device_state (status);
