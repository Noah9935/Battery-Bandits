-- Seit wann ist ein Gerät in seinem jetzigen Zustand? Die Überwachung setzt den Wert bei jedem Statuswechsel.
alter table devices add column status_changed_at timestamptz;

-- Bestehende Geräte: Beginn der offenen Meldung, sonst die letzte Messung, sonst das Anlegen.
update devices d
set status_changed_at = coalesce(
    (select max(a.created_at) from alerts a where a.device_id = d.id and a.status = 'open'),
    d.last_seen_at,
    d.created_at
);

-- Abgeschlossene Batteriezyklen. Bei jedem Batteriewechsel hält die Überwachung fest, was die alte Batterie
-- geleistet hat. Daraus entsteht der Vergleich zwischen den Zyklen eines Geräts.
create table battery_cycles (
    id               bigserial primary key,
    device_id        text not null references devices (id) on delete cascade,
    started_at       timestamptz not null,
    ended_at         timestamptz not null,
    discharged_ah    double precision not null,
    start_voltage_mv integer,
    end_voltage_mv   integer,
    avg_temperature_c real,
    constraint battery_cycles_order check (ended_at >= started_at)
);

create index battery_cycles_device_idx on battery_cycles (device_id, ended_at desc);

-- Weiterleitung aufs Handy: je Person eine Regel, welche Meldungen über welche Dienste gehen.
-- Die Zugänge der Dienste (Bot-Token, SMTP, Webhooks) stehen nur in der Umgebung des Servers, nie hier.
create table notification_rules (
    username         text primary key,
    enabled          boolean not null default false,
    only_critical    boolean not null default true,
    whatsapp         boolean not null default false,
    whatsapp_number  text    not null default '',
    telegram         boolean not null default false,
    telegram_chat_id text    not null default '',
    sms              boolean not null default false,
    sms_number       text    not null default '',
    email            boolean not null default false,
    email_address    text    not null default '',
    updated_at       timestamptz not null default now()
);

-- Was wurde wohin gesendet, und hat es geklappt? Für die Nachvollziehbarkeit, ohne Inhalte der Zugänge.
create table notification_log (
    id       bigserial primary key,
    at       timestamptz not null default now(),
    username text not null,
    channel  text not null check (channel in ('whatsapp', 'telegram', 'sms', 'email')),
    alert_id bigint,
    ok       boolean not null,
    detail   text not null default ''
);

create index notification_log_user_idx on notification_log (username, at desc);
