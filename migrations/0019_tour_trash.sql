-- Papierkorb: Gelöschte Touren bleiben 30 Tage erhalten und lassen sich zurückholen, danach werden sie endgültig gelöscht.
alter table tours add column deleted_at timestamptz;

-- Der Name muss nur unter den Touren eindeutig sein, die nicht im Papierkorb liegen.
drop index tours_owner_name_idx;
create unique index tours_owner_name_idx on tours (owner, lower(btrim(name))) where deleted_at is null;
create index tours_deleted_idx on tours (deleted_at) where deleted_at is not null;
