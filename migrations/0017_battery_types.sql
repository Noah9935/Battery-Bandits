-- Katalog der Batterien: Wer sie bestellt, legt sie mit Preis an. Beim Wechsel wählt man sie nur noch aus.
-- Gelöschte Typen werden archiviert, damit frühere Wechsel erklärbar bleiben.
create table battery_types (
    id              bigserial primary key,
    name            text not null check (length(btrim(name)) between 1 and 60),
    price_cents     integer check (price_cents between 0 and 1000000),
    note            text check (length(note) <= 300),
    created_by_name text,
    created_at      timestamptz not null default now(),
    archived_at     timestamptz
);
create unique index battery_types_name_idx on battery_types (lower(btrim(name))) where archived_at is null;
