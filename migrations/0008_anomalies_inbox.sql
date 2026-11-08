-- Neue Meldungsarten: Messwerte bleiben stehen, Spannung bricht ein, Ausfall trotz hohem Ladestand.
alter type alert_kind add value if not exists 'stagnation';
alter type alert_kind add value if not exists 'drop';
alter type alert_kind add value if not exists 'fault';

-- Schwellen für die Erkennung (eine Zeile, wie system_settings).
create table anomaly_settings (
    id                  smallint primary key default 1 check (id = 1),
    enabled             boolean not null default true,
    stagnation_hours    real    not null default 6   check (stagnation_hours >= 1 and stagnation_hours <= 168),
    drop_mv             integer not null default 600 check (drop_mv between 50 and 5000),
    drop_window_minutes integer not null default 60  check (drop_window_minutes between 5 and 1440),
    fault_min_pct       real    not null default 50  check (fault_min_pct between 0 and 100)
);
insert into anomaly_settings default values;

-- Postfach: was eine Person nicht übersehen soll (neue Meldungen, Zuweisungen, Kommentare).
create table inbox_items (
    id        bigserial primary key,
    username  text not null,
    at        timestamptz not null default now(),
    kind      text not null check (kind in ('alert', 'assigned', 'comment')),
    text      text not null,
    device_id text,
    task_id   bigint,
    read_at   timestamptz
);
create index inbox_user_idx on inbox_items (username, at desc);
create index inbox_unread_idx on inbox_items (username) where read_at is null;
