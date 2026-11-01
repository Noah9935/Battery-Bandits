-- Erfassungs-Service: Geräte und Messwerte (F02).
create extension if not exists timescaledb schema public;

create table ingest.device (
    id              text primary key,
    first_seen_at   timestamptz      not null default now(),
    last_reading_at timestamptz      not null,
    last_charge_pct double precision not null
);

create table ingest.reading (
    device_id   text             not null,
    ts          timestamptz      not null,
    charge_pct  double precision not null check (charge_pct between 0 and 100),
    received_at timestamptz      not null default now(),
    -- Ein Messwert je Gerät und Zeitstempel: Duplikate durch MQTT QoS 1 werden ignoriert.
    primary key (device_id, ts)
);

select create_hypertable('ingest.reading', by_range('ts', interval '7 days'));
