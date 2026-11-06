-- Battery Bandits: Grundschema (PostgreSQL mit TimescaleDB)

create extension if not exists timescaledb;

create type device_status as enum ('normal', 'warning', 'critical', 'offline');
create type alert_kind as enum ('critical', 'offline');
create type alert_status as enum ('open', 'dismissed', 'resolved');
create type maintenance_status as enum ('planned', 'completed');

-- Ein TrafficNode mit seinem letzten bekannten Zustand.
create table devices (
    id                   text primary key,
    name                 text,
    status               device_status not null default 'normal',
    soc_percent          real,
    voltage_mv           integer,
    current_ma           integer,
    temperature_c        real,
    lat                  double precision,
    lon                  double precision,
    altitude_m           double precision,
    satellites           smallint,
    last_seen_at         timestamptz,
    -- Startwert und entnommene Ladung der aktuellen Batterie (SoC-Integration nach Vorlesung)
    soc_start_percent    real,
    discharged_ah        double precision not null default 0,
    battery_installed_at timestamptz,
    created_at           timestamptz not null default now(),
    constraint devices_position_pair check ((lat is null) = (lon is null))
);
create index devices_status_idx on devices (status);

-- Zeitreihe der Messwerte.
create table readings (
    time          timestamptz not null,
    device_id     text not null references devices (id) on delete cascade,
    voltage_mv    integer not null,
    current_ma    integer not null,
    temperature_c real not null,
    soc_percent   real not null,
    lat           double precision,
    lon           double precision,
    altitude_m    double precision,
    satellites    smallint
);
select create_hypertable('readings', 'time');
create index readings_device_time_idx on readings (device_id, time desc);

alter table readings set (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'device_id',
    timescaledb.compress_orderby = 'time desc'
);
select add_compression_policy('readings', interval '7 days');
-- Aufbewahrung 12 Monate laut Aufgabenstellung.
select add_retention_policy('readings', interval '12 months');

-- Verworfene Nachrichten (F06).
create table invalid_messages_log (
    id          bigserial primary key,
    received_at timestamptz not null default now(),
    topic       text not null,
    payload     text not null,
    reason      text not null
);
create index invalid_messages_received_idx on invalid_messages_log (received_at desc);

-- Meldungen zu kritischen oder offline gegangenen Geräten.
create table alerts (
    id         bigserial primary key,
    device_id  text not null references devices (id) on delete cascade,
    kind       alert_kind not null,
    status     alert_status not null default 'open',
    created_at timestamptz not null default now(),
    closed_at  timestamptz
);
-- Pro Gerät höchstens eine offene Meldung.
create unique index alerts_one_open_per_device on alerts (device_id) where status = 'open';
create index alerts_status_idx on alerts (status, created_at desc);

-- Geplante und erledigte Batteriewechsel.
create table maintenance_tasks (
    id           bigserial primary key,
    device_id    text not null references devices (id) on delete cascade,
    alert_id     bigint references alerts (id) on delete set null,
    status       maintenance_status not null default 'planned',
    note         text,
    created_at   timestamptz not null default now(),
    completed_at timestamptz
);
create index maintenance_status_idx on maintenance_tasks (status, created_at desc);

-- Einstellungen: genau eine Zeile (NF07).
create table system_settings (
    id                      smallint primary key default 1 check (id = 1),
    threshold_critical_pct  real not null default 20,
    threshold_warning_pct   real not null default 40,
    offline_timeout_seconds integer not null default 900,
    min_voltage_mv          integer not null default 10500,
    max_voltage_mv          integer not null default 12700,
    battery_capacity_ah     real not null default 80,
    constraint settings_thresholds check (
        threshold_critical_pct >= 0 and threshold_warning_pct <= 100
        and threshold_critical_pct < threshold_warning_pct),
    constraint settings_timeout check (offline_timeout_seconds >= 60),
    constraint settings_voltage check (min_voltage_mv < max_voltage_mv),
    constraint settings_capacity check (battery_capacity_ah > 0)
);
insert into system_settings default values;
