-- Meldungs-Service: Meldungen zu kritischen Geräten.
create table incidents.incident (
    id         bigserial primary key,
    device_id  text             not null,
    opened_at  timestamptz      not null,
    closed_at  timestamptz,
    state      text             not null default 'open'
               check (state in ('open', 'dismissed', 'resolved')),
    charge_pct double precision not null
);

-- Höchstens eine offene Meldung je Gerät.
create unique index one_open_incident_per_device
    on incidents.incident (device_id) where state = 'open';

create index incident_opened_at_idx on incidents.incident (opened_at desc);
