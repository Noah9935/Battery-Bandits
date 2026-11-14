-- Geräte austragen: Ein Gerät, das nichts mehr zu tun hat, ersetzt oder weggeworfen wurde, verschwindet aus Übersicht,
-- Karte und Meldungen. Seine Messwerte und Einsätze bleiben gespeichert, es lässt sich jederzeit wieder aufnehmen.
alter table devices
    add column retired_at     timestamptz,
    add column retired_reason text check (retired_reason in ('done', 'replaced', 'discarded')),
    add column retired_note   text check (retired_note is null or length(retired_note) <= 500);
create index devices_active_idx on devices (id) where retired_at is null;
