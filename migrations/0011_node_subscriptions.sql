-- Eintragen bei Nodes: Wer sich bei einem Node einträgt, bekommt dessen Eilmeldungen. Hat ein Node niemanden
-- eingetragen, bekommt jeder die Meldung (zum Beispiel wenn die Batterie leer ist).
create table node_subscriptions (
    username   text not null,
    device_id  text not null references devices (id) on delete cascade,
    created_at timestamptz not null default now(),
    primary key (username, device_id)
);

create index node_subscriptions_device_idx on node_subscriptions (device_id);
